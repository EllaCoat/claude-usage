mod aggregator;
mod official_usage;
mod paths;
mod pricing;
mod remote_jsonl;
mod usage;
mod win;

use chrono::Utc;

#[tauri::command]
fn get_usage_summary() -> Result<aggregator::UsageSummary, String> {
    let dir = paths::claude_projects_dir()
        .ok_or_else(|| "could not resolve home directory".to_string())?;
    if !dir.exists() {
        return Err(format!("{} does not exist", dir.display()));
    }
    let entries = usage::scan_dir(&dir);
    Ok(aggregator::aggregate(&entries, Utc::now()))
}

#[tauri::command]
fn get_official_usage() -> Result<official_usage::OfficialUsage, String> {
    official_usage::fetch()
}

#[tauri::command]
fn get_remote_jsonl_summary(host: String) -> Result<aggregator::UsageSummary, String> {
    remote_jsonl::fetch(&host)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_usage_summary,
            get_official_usage,
            get_remote_jsonl_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
