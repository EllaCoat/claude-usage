mod aggregator;
mod paths;
mod pricing;
mod usage;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_usage_summary])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
