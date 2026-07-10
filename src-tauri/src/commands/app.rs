use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub db_ready: bool,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Branchgate".into(),
        version: "beta".into(),
        platform: std::env::consts::OS.into(),
        db_ready: crate::db::pool().is_ok(),
    }
}

#[tauri::command]
pub async fn health_check() -> Result<String, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok("ok".into())
}
