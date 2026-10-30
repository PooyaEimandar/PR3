//! Hands exported files to the system (docs Sec. 4.8): the share sheet on iOS and
//! Android, where the user picks where they go; on a computer, a save dialog. PR3 sends
//! nothing itself.

use std::path::PathBuf;

use tauri::{AppHandle, WebviewWindow};

/// Shares `files`, returning what to tell the user, if anything.
#[cfg(desktop)]
pub async fn files(
    app: &AppHandle,
    window: &WebviewWindow,
    files: &[PathBuf],
) -> Result<Option<String>, String> {
    use tauri::Manager;

    let Some(first) = files.first() else {
        return Ok(None);
    };
    let name = first
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("Export task")
        .set_file_name(name)
        .set_parent(window);
    if let Ok(downloads) = app.path().download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    let chosen = dialog.save_file().await;
    let saved = chosen.map(|chosen| save_beside(chosen.path(), files));
    if let Some(dir) = first.parent() {
        crate::task::export::clear(dir);
    }
    saved.transpose()
}

/// Saves each file under the name the user chose, keeping its own extension, so the
/// Markdown and the JSON stay side by side.
#[cfg(desktop)]
fn save_beside(chosen: &std::path::Path, files: &[PathBuf]) -> Result<String, String> {
    let mut names = Vec::new();
    for file in files {
        let target = match file.extension() {
            Some(extension) => chosen.with_extension(extension),
            None => chosen.to_path_buf(),
        };
        std::fs::copy(file, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        names.push(
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    let folder = chosen.parent().map(|p| p.display().to_string());
    Ok(format!(
        "Saved {} in {}.",
        names.join(" and "),
        folder.unwrap_or_default()
    ))
}

/// Shares `files`, returning what to tell the user, if anything.
#[cfg(target_os = "ios")]
pub async fn files(
    _app: &AppHandle,
    window: &WebviewWindow,
    files: &[PathBuf],
) -> Result<Option<String>, String> {
    let files = files.to_vec();
    window
        .with_webview(move |webview| {
            // SAFETY: with_webview runs this on the main thread, where UIKit must be
            // used, and `view_controller` is the live UIViewController showing the
            // webview.
            unsafe { present(webview.view_controller().cast(), &files) }
        })
        .map_err(|e| e.to_string())?;
    Ok(None)
}

/// Presents a UIActivityViewController for `files` over `controller`.
///
/// # Safety
///
/// Must run on the main thread, with `controller` a valid UIViewController.
#[cfg(target_os = "ios")]
unsafe fn present(controller: *mut objc2::runtime::AnyObject, files: &[PathBuf]) {
    use block2::Block;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let urls: Vec<Retained<NSURL>> = files
        .iter()
        .map(|f| NSURL::fileURLWithPath(&NSString::from_str(&f.to_string_lossy())))
        .collect();
    let items = NSArray::from_retained_slice(&urls);
    let none: Option<&AnyObject> = None;
    let done: Option<&Block<dyn Fn()>> = None;
    // SAFETY: the selectors are UIActivityViewController's and UIViewController's,
    // called with the argument types they take; the caller is on the main thread.
    unsafe {
        let sheet: Allocated<AnyObject> = msg_send![class!(UIActivityViewController), alloc];
        let sheet: Retained<AnyObject> =
            msg_send![sheet, initWithActivityItems: &*items, applicationActivities: none];
        // On an iPad the sheet is a popover, which needs a view to point at.
        let popover: Option<Retained<AnyObject>> =
            msg_send![&*sheet, popoverPresentationController];
        if let Some(popover) = popover {
            let view: Option<Retained<AnyObject>> = msg_send![controller, view];
            let _: () = msg_send![&*popover, setSourceView: view.as_deref()];
        }
        let _: () = msg_send![
            controller,
            presentViewController: &*sheet,
            animated: Bool::YES,
            completion: done
        ];
    }
}

/// Shares `files`, returning what to tell the user, if anything.
#[cfg(target_os = "android")]
pub async fn files(
    app: &AppHandle,
    _window: &WebviewWindow,
    files: &[PathBuf],
) -> Result<Option<String>, String> {
    use tauri::Manager;

    let handle = app
        .try_state::<android::ShareHandle>()
        .ok_or("sharing is unavailable")?;
    let paths: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();
    handle
        .0
        .run_mobile_plugin::<serde_json::Value>("share", serde_json::json!({ "paths": paths }))
        .map_err(|e| e.to_string())?;
    Ok(None)
}

#[cfg(target_os = "android")]
mod android {
    use tauri::Manager;
    use tauri::Wry;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};

    /// The Kotlin plugin, registered at startup.
    pub struct ShareHandle(pub PluginHandle<Wry>);

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("pr3-share")
            .setup(|app, api| {
                let handle = api.register_android_plugin("ai.pooya.pr3", "SharePlugin")?;
                app.manage(ShareHandle(handle));
                Ok(())
            })
            .build()
    }
}

#[cfg(target_os = "android")]
pub use android::plugin;

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    #[test]
    fn both_files_are_saved_under_the_chosen_name() {
        let dir = std::env::temp_dir().join(format!("pr3-share-{}", std::process::id()));
        let export = dir.join("export");
        std::fs::create_dir_all(&export).unwrap();
        let files: Vec<PathBuf> = ["md", "json"]
            .iter()
            .map(|extension| {
                let file = export.join(format!("pr3-trip.{extension}"));
                std::fs::write(&file, extension).unwrap();
                file
            })
            .collect();

        // The user renamed it and left out the extension.
        let said = save_beside(&dir.join("Lisbon notes"), &files).unwrap();
        assert_eq!(
            said,
            format!(
                "Saved Lisbon notes.md and Lisbon notes.json in {}.",
                dir.display()
            )
        );
        let md = std::fs::read_to_string(dir.join("Lisbon notes.md")).unwrap();
        let json = std::fs::read_to_string(dir.join("Lisbon notes.json")).unwrap();
        assert_eq!((md.as_str(), json.as_str()), ("md", "json"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
