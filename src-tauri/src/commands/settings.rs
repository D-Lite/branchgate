use tauri::AppHandle;

#[tauri::command]
pub async fn get_app_settings() -> Result<crate::settings::AppSettings, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::get_settings(pool).await
}

#[tauri::command]
pub async fn update_app_settings(
    request: crate::settings::UpdateAppSettingsRequest,
) -> Result<crate::settings::AppSettings, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::update_settings(pool, request).await
}

#[tauri::command]
pub async fn list_editors() -> Result<Vec<crate::settings::EditorRow>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::list_editors(pool).await
}

#[tauri::command]
pub async fn detect_editors() -> Result<Vec<crate::settings::EditorRow>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::detect_editors(pool).await
}

#[tauri::command]
pub async fn set_preferred_editor(editor_id: i64) -> Result<Vec<crate::settings::EditorRow>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::set_preferred_editor(pool, editor_id).await
}

#[tauri::command]
pub async fn update_repo_settings(
    request: crate::settings::UpdateRepoSettingsRequest,
) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::update_repo_settings(pool, request).await
}

#[tauri::command]
pub async fn disconnect_repo(repo_id: i64) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::disconnect_repo(pool, repo_id).await
}

#[tauri::command]
pub async fn clear_sync_cache() -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::clear_sync_cache(pool).await
}

#[tauri::command]
pub async fn reset_app_data() -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::reset_app_data(pool).await
}

#[tauri::command]
pub async fn get_diagnostics(app: AppHandle) -> Result<crate::settings::DiagnosticsInfo, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::settings::get_diagnostics(&app, pool).await
}
