use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::process::Command;
use std::sync::OnceLock;

#[derive(Serialize, Clone, Debug)]
pub struct OfficialUsage {
    pub session_pct: Option<u32>,
    pub session_reset: Option<String>,
    pub week_all_pct: Option<u32>,
    pub week_all_reset: Option<String>,
    pub week_sonnet_pct: Option<u32>,
    pub fetched_at: DateTime<Utc>,
}

static SESSION_RE: OnceLock<Regex> = OnceLock::new();
static WEEK_ALL_RE: OnceLock<Regex> = OnceLock::new();
static WEEK_SONNET_RE: OnceLock<Regex> = OnceLock::new();

fn re(slot: &OnceLock<Regex>, pat: &str) -> &Regex {
    slot.get_or_init(|| Regex::new(pat).expect("invalid regex"))
}

pub fn fetch() -> Result<OfficialUsage, String> {
    let output = Command::new("claude")
        .args(["--print", "/usage"])
        .output()
        .map_err(|e| format!("failed to spawn claude: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "claude exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse(&text))
}

fn parse(text: &str) -> OfficialUsage {
    let session = re(
        &SESSION_RE,
        r"Current session:\s*(\d+)%\s*used(?:\s*·\s*resets\s+(.+))?",
    );
    let week_all = re(
        &WEEK_ALL_RE,
        r"Current week \(all models\):\s*(\d+)%\s*used(?:\s*·\s*resets\s+(.+))?",
    );
    let week_sonnet = re(
        &WEEK_SONNET_RE,
        r"Current week \(Sonnet only\):\s*(\d+)%\s*used",
    );

    let session_cap = text.lines().find_map(|l| session.captures(l));
    let week_all_cap = text.lines().find_map(|l| week_all.captures(l));
    let week_sonnet_cap = text.lines().find_map(|l| week_sonnet.captures(l));

    OfficialUsage {
        session_pct: session_cap
            .as_ref()
            .and_then(|c| c.get(1)?.as_str().parse().ok()),
        session_reset: session_cap
            .as_ref()
            .and_then(|c| c.get(2))
            .map(|m| m.as_str().trim().to_string()),
        week_all_pct: week_all_cap
            .as_ref()
            .and_then(|c| c.get(1)?.as_str().parse().ok()),
        week_all_reset: week_all_cap
            .as_ref()
            .and_then(|c| c.get(2))
            .map(|m| m.as_str().trim().to_string()),
        week_sonnet_pct: week_sonnet_cap
            .as_ref()
            .and_then(|c| c.get(1)?.as_str().parse().ok()),
        fetched_at: Utc::now(),
    }
}
