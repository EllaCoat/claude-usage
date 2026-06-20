use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct UsageEntry {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_5m_write_tokens: u64,
    pub cache_1h_write_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    typ: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

pub fn scan_dir(dir: &Path) -> Vec<UsageEntry> {
    let mut entries = Vec::new();
    for e in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(file) = File::open(e.path()) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Some(entry) = parse_line(&line) {
                entries.push(entry);
            }
        }
    }
    entries
}

pub fn parse_line(line: &str) -> Option<UsageEntry> {
    let l: Line = serde_json::from_str(line).ok()?;
    if l.typ.as_deref() != Some("assistant") {
        return None;
    }
    let msg = l.message?;
    let model = msg.model?;
    let usage = msg.usage?;
    let timestamp = DateTime::parse_from_rfc3339(&l.timestamp?)
        .ok()?
        .with_timezone(&Utc);

    if model.starts_with('<') {
        return None;
    }

    let (c5m, c1h) = match usage.cache_creation {
        Some(cc) => (
            cc.ephemeral_5m_input_tokens.unwrap_or(0),
            cc.ephemeral_1h_input_tokens.unwrap_or(0),
        ),
        None => (usage.cache_creation_input_tokens.unwrap_or(0), 0),
    };

    Some(UsageEntry {
        timestamp,
        model,
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        cache_5m_write_tokens: c5m,
        cache_1h_write_tokens: c1h,
        cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
    })
}
