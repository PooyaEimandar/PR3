//! Research settings and saved-source inspection. Credentials never enter IPC replies.
use pr3_memory_sqlite::research::Evidence;
use serde::Serialize;
use tauri::{AppHandle, Manager};

fn library(app: &AppHandle) -> Result<pr3_tool::documents::Library, String> {
    crate::storage::require_ready(app)?;
    app.try_state::<crate::documents::Library>()
        .map(|s| s.0.clone())
        .ok_or("Research storage is unavailable".into())
}
#[derive(Serialize)]
pub struct Settings {
    configured: bool,
    web_enabled: bool,
}
#[tauri::command]
pub fn research_settings(app: AppHandle) -> Result<Settings, String> {
    let configured = library(&app)?
        .lock()
        .map_err(|_| "Research storage is unavailable")?
        .research_search_key()
        .map_err(|e| e.to_string())?
        .is_some();
    Ok(Settings {
        configured,
        web_enabled: app
            .try_state::<crate::web::Web>()
            .is_some_and(|s| s.info().on),
    })
}
#[tauri::command]
pub fn set_research_key(app: AppHandle, key: String) -> Result<(), String> {
    crate::storage::require_encrypted(&app)?;
    let key = zeroize::Zeroizing::new(key);
    library(&app)?
        .lock()
        .map_err(|_| "Research storage is unavailable")?
        .set_research_search_key(key.trim())
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
