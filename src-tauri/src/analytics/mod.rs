use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const KEY_SHARE_USAGE: &str = "share_anonymous_usage";
const KEY_FIRST_LAUNCH: &str = "analytics_first_launch_done";
const KEY_PENDING_DISTINCT_ID: &str = "pending_distinct_id";

const POSTHOG_HOST: &str = "https://us.i.posthog.com";
const POSTHOG_KEY: &str = "phc_BskG7i7BSi7VRhgLoLrCfW5eDaKwGWLXoP9sQCwzLUrd";
static FLUSH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default)]
pub struct AnalyticsState {
    pub pending_distinct_id: Mutex<Option<String>>,
}

pub async fn ingest_launch_distinct_id(pool: &SqlitePool) -> Result<(), String> {
    if let Some(pid) = parse_pending_distinct_id_from_args() {
        set_optional_string(pool, KEY_PENDING_DISTINCT_ID, Some(&pid)).await?;
    }
    Ok(())
}

pub async fn load_pending_into_state(app: &AppHandle) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let pending = get_optional_string(&pool, KEY_PENDING_DISTINCT_ID).await?;
    if let Some(state) = app.try_state::<AnalyticsState>() {
        *state.pending_distinct_id.lock().unwrap() = pending;
    }
    Ok(())
}

pub async fn get_opt_in(pool: &SqlitePool) -> Result<bool, String> {
    get_bool(pool, KEY_SHARE_USAGE, true).await
}

pub async fn set_opt_in(pool: &SqlitePool, enabled: bool) -> Result<(), String> {
    set_bool(pool, KEY_SHARE_USAGE, enabled).await?;
    if !enabled {
        sqlx::query("DELETE FROM analytics_queue")
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub async fn queue_event(
    pool: &SqlitePool,
    distinct_id: &str,
    event: &str,
    properties: Value,
) -> Result<(), String> {
    if !get_opt_in(pool).await? {
        return Ok(());
    }
    if distinct_id.trim().is_empty() || event.trim().is_empty() {
        return Err("Analytics event and distinct ID are required".into());
    }

    let properties = properties.as_object().cloned().unwrap_or_default();
    sqlx::query(
        r#"
        INSERT INTO analytics_queue (distinct_id, event, properties_json, created_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(distinct_id)
    .bind(event)
    .bind(Value::Object(properties).to_string())
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        DELETE FROM analytics_queue
        WHERE id IN (
            SELECT id FROM analytics_queue
            ORDER BY created_at DESC, id DESC
            LIMIT -1 OFFSET 10000
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn flush_events(pool: &SqlitePool) -> Result<u64, String> {
    let _guard = FLUSH_LOCK.lock().await;
    if !get_opt_in(pool).await? {
        return Ok(0);
    }

    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        r#"
        SELECT id, distinct_id, event, properties_json
        FROM analytics_queue
        ORDER BY created_at ASC, id ASC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut sent = 0;
    for (id, distinct_id, event, properties_json) in rows {
        let properties = serde_json::from_str::<Value>(&properties_json)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        match capture_event(
            &distinct_id,
            "desktop_app",
            &event,
            properties,
            true,
        )
        .await
        {
            Ok(()) => {
                sqlx::query("DELETE FROM analytics_queue WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await
                    .map_err(|error| error.to_string())?;
                sent += 1;
            }
            Err(error) => {
                sqlx::query(
                    "UPDATE analytics_queue SET attempts = attempts + 1, last_error = ? WHERE id = ?",
                )
                .bind(error.to_string())
                .bind(id)
                .execute(pool)
                .await
                .map_err(|db_error| db_error.to_string())?;
                break;
            }
        }
    }
    Ok(sent)
}

pub async fn is_first_launch_done(pool: &SqlitePool) -> Result<bool, String> {
    get_bool(pool, KEY_FIRST_LAUNCH, false).await
}

pub async fn mark_first_launch(pool: &SqlitePool, state: &AnalyticsState) -> Result<(), String> {
    set_bool(pool, KEY_FIRST_LAUNCH, true).await?;
    set_optional_string(pool, KEY_PENDING_DISTINCT_ID, None).await?;
    *state.pending_distinct_id.lock().unwrap() = None;
    Ok(())
}

pub async fn pending_distinct_id(
    pool: &SqlitePool,
    state: &AnalyticsState,
) -> Result<Option<String>, String> {
    if let Some(id) = state.pending_distinct_id.lock().unwrap().clone() {
        return Ok(Some(id));
    }

    let id = get_optional_string(pool, KEY_PENDING_DISTINCT_ID).await?;
    if let Some(ref value) = id {
        *state.pending_distinct_id.lock().unwrap() = Some(value.clone());
    }
    Ok(id)
}

pub async fn capture_event(
    distinct_id: &str,
    site_name: &str,
    event: &str,
    mut properties: Map<String, Value>,
    opted_in: bool,
) -> Result<(), reqwest::Error> {
    if !opted_in {
        return Ok(());
    }

    properties.insert("distinct_id".into(), json!(distinct_id));
    properties.insert("site_name".into(), json!(site_name));

    reqwest::Client::new()
        .post(format!("{POSTHOG_HOST}/capture/"))
        .json(&json!({
            "api_key": POSTHOG_KEY,
            "event": event,
            "properties": properties,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn parse_pending_distinct_id_from_args() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for (index, arg) in args.iter().enumerate() {
        if arg == "--pid" {
            return args
                .get(index + 1)
                .cloned()
                .filter(|value| !value.is_empty());
        }
        if let Some(pid) = arg.strip_prefix("--pid=") {
            if !pid.is_empty() {
                return Some(pid.to_string());
            }
        }
        if arg.contains("pid=") {
            if let Some(pid) = query_param(arg, "pid") {
                return Some(pid);
            }
        }
    }
    None
}

fn query_param(input: &str, key: &str) -> Option<String> {
    let query = input.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next()?;
        if name == key {
            let value = parts.next().unwrap_or("");
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

async fn get_bool(pool: &SqlitePool, key: &str, default: bool) -> Result<bool, String> {
    let value = get_optional_string(pool, key).await?;
    Ok(match value.as_deref() {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        None => default,
        Some(other) => other != "0",
    })
}

async fn set_bool(pool: &SqlitePool, key: &str, value: bool) -> Result<(), String> {
    set_string(pool, key, if value { "1" } else { "0" }).await
}

async fn get_optional_string(pool: &SqlitePool, key: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn set_string(pool: &SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value) VALUES (?, ?)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn set_optional_string(
    pool: &SqlitePool,
    key: &str,
    value: Option<&str>,
) -> Result<(), String> {
    match value {
        Some(text) if !text.is_empty() => set_string(pool, key, text).await,
        _ => {
            sqlx::query("DELETE FROM settings WHERE key = ?")
                .bind(key)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}
