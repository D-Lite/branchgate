mod analytics;
mod commands;
mod db;
mod editors;
mod error;
mod git;
mod github;
mod history;
mod promote;
mod settings;
mod sync;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(analytics::AnalyticsState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                if let Err(err) = db::init(&handle).await {
                    eprintln!("database init failed: {err}");
                    return;
                }
                if let Ok(pool) = db::pool() {
                    if let Err(err) = analytics::ingest_launch_distinct_id(pool).await {
                        eprintln!("analytics launch id ingest failed: {err}");
                    }
                }
                if let Err(err) = analytics::load_pending_into_state(&handle).await {
                    eprintln!("analytics state load failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::analytics::get_analytics_opt_in,
            commands::analytics::set_analytics_opt_in,
            commands::analytics::get_first_launch_done,
            commands::analytics::mark_first_launch_done,
            commands::analytics::get_pending_distinct_id,
            commands::app::get_app_info,
            commands::app::health_check,
            commands::pipelines::list_pipelines,
            commands::pipelines::pipeline_count,
            commands::repos::inspect_local_repo,
            commands::repos::connect_local_repo,
            commands::repos::list_connected_repos,
            commands::pipeline_sync::sync_pipeline,
            commands::pipeline_sync::get_pipeline_checklist,
            commands::pipeline_sync::get_pipeline_sync_status,
            commands::promote::start_promotion_run,
            commands::promote::abort_promotion_run,
            commands::promote::get_active_promotion_run,
            commands::promote::continue_promotion_run,
            commands::promote::open_conflict_in_editor,
            commands::promote::open_repo_in_editor,
            commands::history::list_promotion_history,
            commands::settings::get_app_settings,
            commands::settings::update_app_settings,
            commands::settings::list_editors,
            commands::settings::detect_editors,
            commands::settings::set_preferred_editor,
            commands::settings::update_repo_settings,
            commands::settings::disconnect_repo,
            commands::settings::clear_sync_cache,
            commands::settings::reset_app_data,
            commands::settings::get_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running branchgate");
}
