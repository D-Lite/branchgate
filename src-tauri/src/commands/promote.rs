#[tauri::command]
pub async fn start_promotion_run(
    pipeline_id: i64,
    pr_ids: Vec<i64>,
) -> Result<crate::promote::PromotionRunResult, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::run_promotion(pool, pipeline_id, pr_ids).await
}

#[tauri::command]
pub async fn abort_promotion_run(run_id: i64) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::abort_promotion_run(pool, run_id).await
}

#[tauri::command]
pub async fn get_active_promotion_run(
    pipeline_id: i64,
) -> Result<Option<crate::promote::PromotionRunResult>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::get_active_run(pool, pipeline_id).await
}

#[tauri::command]
pub async fn continue_promotion_run(
    run_id: i64,
) -> Result<crate::promote::PromotionRunResult, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::prepare_conflict_resolution(pool, run_id).await
}

#[tauri::command]
pub async fn open_conflict_in_editor(run_id: i64, file_path: String) -> Result<String, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::open_conflict_file(pool, run_id, &file_path).await
}

#[tauri::command]
pub async fn open_repo_in_editor(run_id: i64) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::promote::open_repo_in_editor(pool, run_id).await
}
