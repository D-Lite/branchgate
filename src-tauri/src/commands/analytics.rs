use tauri::State;

#[tauri::command]
pub fn get_analytics_opt_in() -> Result<bool, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(crate::analytics::get_opt_in(&pool))
}

#[tauri::command]
pub fn set_analytics_opt_in(enabled: bool) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(crate::analytics::set_opt_in(&pool, enabled))
}

#[tauri::command]
pub fn get_first_launch_done() -> Result<bool, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(crate::analytics::is_first_launch_done(&pool))
}

#[tauri::command]
pub fn mark_first_launch_done(
    state: State<crate::analytics::AnalyticsState>,
) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(crate::analytics::mark_first_launch(&pool, &state))
}

#[tauri::command]
pub fn get_pending_distinct_id(
    state: State<crate::analytics::AnalyticsState>,
) -> Result<Option<String>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    tauri::async_runtime::block_on(crate::analytics::pending_distinct_id(&pool, &state))
}
