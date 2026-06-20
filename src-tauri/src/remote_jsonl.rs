use crate::aggregator::{aggregate, UsageSummary, WINDOW_HOURS};
use crate::usage::parse_line;
use crate::win;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chrono::{Duration, Utc};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration as StdDuration;
use wait_timeout::ChildExt;

// ssh 全体の hard timeout (秒)。 ConnectTimeout / ServerAliveInterval だけでは
// auth フェーズや known_hosts 確認等で粘られるケースを救えないので、
// プロセス丸ごとの kill 用に上限を別途設ける。 繋がるときは一瞬で繋がる経験則に合わせて短めに。
const SSH_HARD_TIMEOUT_SEC: u64 = 10;

// Windows GUI exe (= windows_subsystem = "windows") から spawn される ssh.exe は、
// PATH に複数の ssh.exe (Git Bash / MSYS2 等) があると意図しない側が呼ばれることがある。
// Win11 標準の OpenSSH 配置を明示し、 無ければ素の "ssh" にフォールバック。
#[cfg(windows)]
fn resolve_ssh() -> std::ffi::OsString {
    let sysroot = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let p = PathBuf::from(sysroot).join("System32\\OpenSSH\\ssh.exe");
    if p.exists() {
        p.into_os_string()
    } else {
        "ssh".into()
    }
}
#[cfg(not(windows))]
fn resolve_ssh() -> std::ffi::OsString {
    "ssh".into()
}

// 失敗診断用に ssh の verbose log を吐かせる tempfile path。
// process id 単位なので 1 セッション中ずっと上書きされていく (= 蓄積しない)。
fn debug_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("claude-usage-ssh-{}.log", std::process::id()))
}

// EC2 上の jsonl を ssh + jq で 5h window 分だけ事前 filter してから取得 → 既存 aggregator で集計。
// 全 jsonl を流すと数百 MB になるので、 EC2 側で絞ってから流すことで転送量を数 KB レベルに抑える。
pub fn fetch(host: &str) -> Result<UsageSummary, String> {
    let now = Utc::now();
    // window 跨ぎを考慮して 5h より少し広めに取る (session 起点が cutoff より前にあるケースを救う)。
    let cutoff = now - Duration::hours(WINDOW_HOURS + 1);
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    // -mmin -360 でファイル mtime も 6 時間以内に絞る (= active session の jsonl のみスキャン)。
    let inner_cmd = format!(
        r#"find ~/.claude/projects -name '*.jsonl' -mmin -360 -exec jq -c 'select(.type == "assistant" and (.timestamp // "") > "{}")' {{}} +"#,
        cutoff_str
    );

    // Windows の std::process::Command は引数を CommandLineToArgvW 互換で quote するが、
    // ssh は引数群を「ローカルで join → リモート shell に 1 string で渡す」 という二段挙動なので、
    // single/double quote 混在の inner_cmd を素で渡すと Rust の自動 escape と ssh の join
    // どちらかで壊れるケースに当たる。 base64 でくるんでリモート側で復号 → bash に流せば、
    // shell に届く文字種が英数 + `=` `+` `/` + ` ` `|` だけになり quote 問題が消える。
    let encoded = B64.encode(inner_cmd.as_bytes());
    let remote_cmd = format!("echo {} | base64 -d | bash", encoded);

    let ssh_exe = resolve_ssh();
    let log_path = debug_log_path();
    let log_str = log_path.to_string_lossy().to_string();

    // ssh の挙動を「不通なら速やかに諦める」 寄りに固定:
    // - ConnectTimeout=5             : TCP connect を 5s で諦める
    // - ServerAliveInterval/CountMax : 確立後の通信途絶を ~20s で検知 → 切断
    // - BatchMode=yes                : password / passphrase 等の対話 prompt を全面禁止 (= 即 fail)
    // - ClearAllForwardings=yes      : ~/.ssh/config 由来の RemoteForward / LocalForward を全部 disable
    //                                  (= 不要な forward fail warning + 余分な setup を消す)
    // - -vvv -E <log>                : 失敗診断用、 stderr にではなく log file に書く (= 通常時は無害)
    let mut cmd = Command::new(&ssh_exe);
    cmd.args([
        "-vvv",
        "-E",
        &log_str,
        "-o",
        "ConnectTimeout=5",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=2",
        "-o",
        "BatchMode=yes",
        "-o",
        "ClearAllForwardings=yes",
        host,
        &remote_cmd,
    ])
    // GUI exe には対話 stdin が無い。 ssh が何かを input から読もうとして詰まる線を潰すため明示で塞ぐ。
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    win::hide_window(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("ssh spawn failed: {}", e))?;

    // ! 重要: Windows pipe の OS buffer は ~64 KB しかない。 wait_with_output は child 終了後に
    // ! まとめて読むので、 jq 出力が 64 KB を超えると ssh client → stdout pipe → block で
    // ! child が完了せず、 こちら側の wait も帰らないという pipe deadlock になる。
    // ! → spawn 直後に stdout / stderr を別 thread で drain して buffer を空け続ける。
    let stdout = child.stdout.take().ok_or("ssh stdout missing")?;
    let stderr = child.stderr.take().ok_or("ssh stderr missing")?;
    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(64 * 1024 * 1024).read_to_end(&mut buf);
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(1 * 1024 * 1024).read_to_end(&mut buf);
        buf
    });

    // wait_timeout crate で「指定秒以内に終わらなければ None を返す」 wait を取得。
    let timeout = StdDuration::from_secs(SSH_HARD_TIMEOUT_SEC);
    let status_opt = child
        .wait_timeout(timeout)
        .map_err(|e| format!("ssh wait failed: {}", e))?;

    let status = match status_opt {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "ssh {} timed out after {}s (log: {})",
                host, SSH_HARD_TIMEOUT_SEC, log_str
            ));
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    if !status.success() {
        let err = String::from_utf8_lossy(&stderr_bytes);
        return Err(format!(
            "ssh {} exited {} (log: {}): {}",
            host,
            status,
            log_str,
            err.trim()
        ));
    }

    let entries: Vec<_> = stdout_bytes
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| std::str::from_utf8(l).ok())
        .filter_map(|s| parse_line(s))
        .collect();

    Ok(aggregate(&entries, now))
}
