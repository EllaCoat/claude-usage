use crate::aggregator::{aggregate, UsageSummary, WINDOW_HOURS};
use crate::usage::parse_line;
use chrono::{Duration, Utc};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

// EC2 上の jsonl を ssh + jq で 5h window 分だけ事前 filter してから取得 → 既存 aggregator で集計。
// 全 jsonl を流すと数百 MB になるので、 EC2 側で絞ってから流すことで転送量を数 KB レベルに抑える。
pub fn fetch(host: &str) -> Result<UsageSummary, String> {
    let now = Utc::now();
    // window 跨ぎを考慮して 5h より少し広めに取る (session 起点が cutoff より前にあるケースを救う)。
    let cutoff = now - Duration::hours(WINDOW_HOURS + 1);
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    // -mmin -360 でファイル mtime も 6 時間以内に絞る (= active session の jsonl のみスキャン)。
    let remote_cmd = format!(
        r#"find ~/.claude/projects -name '*.jsonl' -mmin -360 -exec jq -c 'select(.type == "assistant" and (.timestamp // "") > "{}")' {{}} +"#,
        cutoff_str
    );

    let mut child = Command::new("ssh")
        .args([host, &remote_cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh spawn failed: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ssh stdout missing".to_string())?;

    let entries: Vec<_> = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| parse_line(&line))
        .collect();

    let status = child
        .wait()
        .map_err(|e| format!("ssh wait failed: {}", e))?;

    if !status.success() {
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            s.read_to_string(&mut err).ok();
        }
        return Err(format!("ssh exited {}: {}", status, err.trim()));
    }

    Ok(aggregate(&entries, now))
}
