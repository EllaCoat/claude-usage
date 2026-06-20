use crate::aggregator::{aggregate, UsageSummary, WINDOW_HOURS};
use crate::usage::parse_line;
use crate::win;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chrono::{Duration, Utc};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration as StdDuration, Instant};

// ssh 全体の hard timeout。 ConnectTimeout / ServerAliveInterval だけでは
// auth フェーズや known_hosts 確認等で粘られるケースを救えないので、
// プロセス丸ごとの kill 用に上限を別途設ける。
const SSH_HARD_TIMEOUT_SEC: u64 = 15;

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

    // ssh の挙動を「不通なら速やかに諦める」 寄りに固定:
    // - ConnectTimeout=5             : TCP connect を 5s で諦める
    // - ServerAliveInterval/CountMax : 確立後の通信途絶を ~20s で検知 → 切断
    // - BatchMode=yes                : password / passphrase 等の対話 prompt を全面禁止 (= 即 fail)
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-o",
        "ConnectTimeout=5",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=2",
        "-o",
        "BatchMode=yes",
        host,
        &remote_cmd,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    win::hide_window(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("ssh spawn failed: {}", e))?;

    // hard timeout: 一定時間内に終わらなければ kill。 try_wait + sleep の polling で実装 (deps 追加なし)。
    let start = Instant::now();
    let timeout = StdDuration::from_secs(SSH_HARD_TIMEOUT_SEC);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "ssh {} timed out after {}s",
                        host, SSH_HARD_TIMEOUT_SEC
                    ));
                }
                thread::sleep(StdDuration::from_millis(200));
            }
            Err(e) => return Err(format!("ssh wait failed: {}", e)),
        }
    }

    // wait 完了後に stdout/stderr を一括回収 (jq filter 済みで数 KB 想定、 stderr deadlock の懸念なし)。
    let output = child
        .wait_with_output()
        .map_err(|e| format!("ssh read failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ssh {} exited {}: {}",
            host,
            output.status,
            err.trim()
        ));
    }

    let entries: Vec<_> = output
        .stdout
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| std::str::from_utf8(l).ok())
        .filter_map(|s| parse_line(s))
        .collect();

    Ok(aggregate(&entries, now))
}
