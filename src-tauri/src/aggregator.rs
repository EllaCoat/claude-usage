use crate::pricing;
use crate::usage::UsageEntry;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

pub const WINDOW_HOURS: i64 = 5;

#[derive(Serialize, Clone, Debug)]
pub struct ModelBreakdown {
    pub model: String,
    pub message_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct UsageSummary {
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
    pub window_progress: f64,
    pub total_messages: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cost_usd: f64,
    pub by_model: Vec<ModelBreakdown>,
}

pub fn aggregate(entries: &[UsageEntry], now: DateTime<Utc>) -> UsageSummary {
    let window_duration = Duration::hours(WINDOW_HOURS);

    let mut sorted: Vec<&UsageEntry> = entries.iter().filter(|e| e.timestamp <= now).collect();
    sorted.sort_by_key(|e| e.timestamp);

    // session 分割: 前 window の終了 (start + 5h) を跨いだ entry を次 window 起点に。
    let mut window_start: Option<DateTime<Utc>> = None;
    for e in &sorted {
        match window_start {
            None => window_start = Some(e.timestamp),
            Some(ws) if e.timestamp >= ws + window_duration => window_start = Some(e.timestamp),
            _ => {}
        }
    }

    let Some(start) = window_start else {
        return empty(now);
    };
    let end = start + window_duration;

    if now >= end {
        return empty(now);
    }

    let elapsed = (now - start).num_seconds().max(0);
    let remaining = (end - now).num_seconds().max(0);
    let progress = (elapsed as f64 / (WINDOW_HOURS * 3600) as f64).clamp(0.0, 1.0);

    let in_window = sorted.iter().filter(|e| e.timestamp >= start);

    let mut groups: BTreeMap<String, ModelBreakdown> = BTreeMap::new();
    for e in in_window {
        let p = pricing::lookup(&e.model);
        let cost = (e.input_tokens as f64 * p.input
            + e.output_tokens as f64 * p.output
            + e.cache_5m_write_tokens as f64 * p.cache_5m_write
            + e.cache_1h_write_tokens as f64 * p.cache_1h_write
            + e.cache_read_tokens as f64 * p.cache_read)
            / 1_000_000.0;

        let g = groups
            .entry(e.model.clone())
            .or_insert_with(|| ModelBreakdown {
                model: e.model.clone(),
                message_count: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                cost_usd: 0.0,
            });
        g.message_count += 1;
        g.input_tokens += e.input_tokens;
        g.output_tokens += e.output_tokens;
        g.cache_creation_tokens += e.cache_5m_write_tokens + e.cache_1h_write_tokens;
        g.cache_read_tokens += e.cache_read_tokens;
        g.cost_usd += cost;
    }

    let mut by_model: Vec<ModelBreakdown> = groups.into_values().collect();
    by_model.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_messages = by_model.iter().map(|m| m.message_count).sum();
    let total_input = by_model.iter().map(|m| m.input_tokens).sum();
    let total_output = by_model.iter().map(|m| m.output_tokens).sum();
    let total_cache_c = by_model.iter().map(|m| m.cache_creation_tokens).sum();
    let total_cache_r = by_model.iter().map(|m| m.cache_read_tokens).sum();
    let total_cost = by_model.iter().map(|m| m.cost_usd).sum();

    UsageSummary {
        window_start: Some(start),
        window_end: Some(end),
        now,
        elapsed_seconds: elapsed,
        remaining_seconds: remaining,
        window_progress: progress,
        total_messages,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_creation_tokens: total_cache_c,
        total_cache_read_tokens: total_cache_r,
        total_cost_usd: total_cost,
        by_model,
    }
}

fn empty(now: DateTime<Utc>) -> UsageSummary {
    UsageSummary {
        window_start: None,
        window_end: None,
        now,
        elapsed_seconds: 0,
        remaining_seconds: 0,
        window_progress: 0.0,
        total_messages: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cache_creation_tokens: 0,
        total_cache_read_tokens: 0,
        total_cost_usd: 0.0,
        by_model: vec![],
    }
}
