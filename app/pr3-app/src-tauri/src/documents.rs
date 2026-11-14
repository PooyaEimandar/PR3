//! User-selected imports only: the renderer supplies bytes from the OS file chooser,
//! never a filesystem path. Storage and tools share the encrypted app database.
use pr3_memory_sqlite::documents::{Document, Passage};
use tauri::{Manager, State};
pub struct Library(pub pr3_tool::documents::Library);
fn library(app: &tauri::AppHandle) -> Result<pr3_tool::documents::Library, String> {
    crate::storage::require_ready(app)?;
    app.try_state::<Library>()
        .map(|s| s.0.clone())
        .ok_or("Document library is unavailable".into())
}
#[tauri::command]
pub fn list_documents(app: tauri::AppHandle) -> Result<Vec<Document>, String> {
    library(&app)?
        .lock()
        .map_err(|_| "Document library is unavailable")?
        .documents()
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn import_document(
    app: tauri::AppHandle,
    name: String,
    text: String,
) -> Result<Document, String> {
    library(&app)?
        .lock()
        .map_err(|_| "Document library is unavailable")?
        .import_document(&name, &text)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn search_documents(app: tauri::AppHandle, query: String) -> Result<Vec<Passage>, String> {
    library(&app)?
        .lock()
        .map_err(|_| "Document library is unavailable")?
        .search_documents(&query)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn document_source(app: tauri::AppHandle, source: String) -> Result<Passage, String> {
    library(&app)?
        .lock()
        .map_err(|_| "Document library is unavailable")?
        .document_passage(&source)
        .map_err(|e| e.to_string())?
        .ok_or("This source was removed".into())
}
#[tauri::command]
pub async fn remove_document(
    app: tauri::AppHandle,
    host: State<'_, crate::host::Host>,
    id: i64,
) -> Result<(), String> {
    host.remove_document(&library(&app)?, id).await
}
