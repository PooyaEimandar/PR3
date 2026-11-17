//! Research settings and saved-source inspection. Credentials never enter IPC replies.
use pr3_memory_sqlite::research::{Evidence, SearchProvider};
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Validate the dialog before it opens a new task. Typed chat requests retain the
/// runtime clarification path; neither path trusts model-generated scope.
#[tauri::command]
pub fn validate_research_topic(topic: String) -> Result<(), String> {
    if topic.trim().is_empty() || topic.len() > 512 || topic.chars().any(char::is_control) {
        return Err("Use a research topic of 1–512 UTF-8 bytes on one line".into());
    }
    if let Some(problem) = pr3_tool::research_scope::SourceScope::from_topic(&topic).problem() {
        return Err(problem.into());
    }
    Ok(())
}

fn library(app: &AppHandle) -> Result<pr3_tool::documents::Library, String> {
    crate::storage::require_ready(app)?;
    app.try_state::<crate::documents::Library>()
        .map(|s| s.0.clone())
        .ok_or("Research storage is unavailable".into())
}
#[derive(Serialize)]
pub struct Settings {
    provider: SearchProvider,
    configured: bool,
    web_enabled: bool,
}
#[tauri::command]
pub fn research_settings(app: AppHandle) -> Result<Settings, String> {
    let library = library(&app)?;
    let db = library
        .lock()
        .map_err(|_| "Research storage is unavailable")?;
    let provider = db.research_search_provider().map_err(|e| e.to_string())?;
    let configured = db
        .research_search_configured(provider)
        .map_err(|e| e.to_string())?;
    Ok(Settings {
        provider,
        configured,
        web_enabled: app
            .try_state::<crate::web::Web>()
            .is_some_and(|s| s.info().on),
    })
}
#[tauri::command]
pub fn set_research_provider(app: AppHandle, provider: SearchProvider) -> Result<(), String> {
    library(&app)?
        .lock()
        .map_err(|_| "Research storage is unavailable")?
        .set_research_search_provider(provider)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_research_key(
    app: AppHandle,
    provider: SearchProvider,
    key: String,
) -> Result<(), String> {
    crate::storage::require_encrypted(&app)?;
    let key = zeroize::Zeroizing::new(key);
    library(&app)?
        .lock()
        .map_err(|_| "Research storage is unavailable")?
        .set_research_search_key(provider, key.trim())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn research_source(app: AppHandle, source: String) -> Result<Evidence, String> {
    let task = source
        .strip_prefix("research:")
        .and_then(|s| s.split(':').next())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("Invalid research source")?;
    library(&app)?
        .lock()
        .map_err(|_| "Research storage is unavailable")?
        .research_source(task, &source)
        .map_err(|e| e.to_string())?
        .ok_or("This research source was removed or is on another device".into())
}
