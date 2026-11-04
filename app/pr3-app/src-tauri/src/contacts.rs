//! Device address book, read only after the user enables sync. Only the preference
//! is persisted: the device already stores contacts for offline use. Imported rows
//! live in memory and are refreshed before every email approval and handoff.

mod native;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pr3_core::trusted::{Contact, Directory};
use serde::{Deserialize, Serialize};
use tauri::Manager;

const ENABLED: &str = "contacts-sync";
pub(super) const MAX_CONTACTS: usize = 20_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    #[default]
    NotDetermined,
    Granted,
    Limited,
    Denied,
    Restricted,
    Unavailable,
}

impl Permission {
    fn allowed(self) -> bool {
        matches!(self, Self::Granted | Self::Limited)
    }
}

#[derive(Clone, Deserialize)]
pub struct Snapshot {
    permission: Permission,
    contacts: Vec<Contact>,
}

pub trait Source: Send + Sync {
    /// `request` is true only for the Settings enable action. Background refreshes
    /// must never show an OS permission dialog.
    fn read(&self, request: bool) -> Result<Snapshot, String>;
}

#[derive(Default)]
struct State {
    on: bool,
    generation: u64,
    permission: Permission,
    contacts: Vec<Contact>,
    problem: Option<String>,
}

#[derive(Serialize)]
pub struct Info {
    on: bool,
    permission: Permission,
    count: usize,
    // The runtime uses the snapshot for recipient resolution. Settings receives no
    // contact rows; the user manages their address book in the device Contacts app.
    #[serde(skip)]
    contacts: Vec<Contact>,
    problem: Option<String>,
}

pub struct Book {
    path: PathBuf,
    source: Box<dyn Source>,
    state: Mutex<State>,
}

impl Book {
    pub fn open(dir: &Path, source: Box<dyn Source>) -> Self {
        let path = dir.join(ENABLED);
        let on = std::fs::read(&path).is_ok_and(|bytes| bytes == b"enabled\n");
        Self {
            path,
            source,
            state: Mutex::new(State {
                on,
                ..State::default()
            }),
        }
    }

    fn info(&self) -> Result<Info, String> {
        let state = self.state.lock().map_err(|_| "contacts are unavailable")?;
        Ok(Info {
            on: state.on,
            permission: state.permission,
            count: state.contacts.len(),
            contacts: state.contacts.clone(),
            problem: state.problem.clone(),
        })
    }

    /// Clear and disable are the same privacy boundary. A slow in-flight native read
    /// cannot repopulate the list after this returns.
    fn disable(&self) -> Result<Info, String> {
        let mut state = self.state.lock().map_err(|_| "contacts are unavailable")?;
        state.generation = state.generation.wrapping_add(1);
        state.on = false;
        state.contacts.clear();
        state.problem = None;
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                state.problem = Some(format!(
                    "Contacts cleared for this session, but could not save sync off: {e}"
                ));
                return Err(state.problem.clone().unwrap_or_default());
            }
        }
        drop(state);
        self.info()
    }

    fn refresh(&self, request: bool) -> Result<Info, String> {
        let generation = {
            let mut state = self.state.lock().map_err(|_| "contacts are unavailable")?;
            if !request && !state.on {
                drop(state);
                return self.info();
            }
            state.generation = state.generation.wrapping_add(1);
            state.generation
        };
        let read = self.source.read(request);
        let mut state = self.state.lock().map_err(|_| "contacts are unavailable")?;
        if generation != state.generation {
            return Err("Contact settings changed while syncing. Try again.".into());
        }
        // A failed refresh cannot leave a stale recipient authorized.
        state.contacts.clear();
        state.problem = None;
        match read {
            Ok(snapshot) => {
                state.permission = snapshot.permission;
                if snapshot.permission.allowed() {
                    match normalize(snapshot.contacts).and_then(|contacts| {
                        if request {
                            std::fs::write(&self.path, b"enabled\n").map_err(|e| {
                                format!("Could not save contact sync preference: {e}")
                            })?;
                        }
                        Ok(contacts)
                    }) {
                        Ok(contacts) => {
                            state.on = true;
                            state.contacts = contacts;
                        }
                        Err(error) => {
                            state.problem = Some(error);
                        }
                    }
                } else {
                    state.on = false;
                    // Keep the cached book empty even if removing a preference fails.
                    match std::fs::remove_file(&self.path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => state.problem = Some(format!("Could not save sync off: {e}")),
                    }
                }
            }
            Err(error) => {
                state.problem = Some(error);
            }
        }
        drop(state);
        self.info()
    }

    pub fn directory(&self) -> Result<Directory, String> {
        let info = self.refresh(false)?;
        if let Some(problem) = info.problem {
            return Err(problem);
        }
        if !info.on || !info.permission.allowed() {
            return Err("Enable Sync address book contacts in Settings → Contacts and allow contact access first.".into());
        }
        Ok(Directory {
            contacts: info.contacts,
        })
    }

    /// Recheck after the approval dialog, including OS revocation, contact edits,
    /// deletion and limited-access changes. Never redirect an already approved mail.
    pub fn with_recipients<T>(
        &self,
        recipients: &[Contact],
        handoff: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.directory()?;
        let state = self.state.lock().map_err(|_| "contacts are unavailable")?;
        if !state.on
            || !state.permission.allowed()
            || !recipients.iter().all(|c| state.contacts.contains(c))
        {
            return Err("A recipient changed or was removed. Prepare the email again and review its recipients.".into());
        }
        // Serialize the last check and the handoff with disable/remove. Once the OS
        // has accepted a draft, clearing PR3 cannot retract it from another app.
        handoff()
    }
}

/// Remove invalid rows, deduplicate, and give duplicate names distinct references.
/// Stable sorting makes a second email address an explicit choice, never an arbitrary
/// first address. Commas are separators in the trusted mail tool's recipient input.
fn normalize(rows: Vec<Contact>) -> Result<Vec<Contact>, String> {
    if rows.len() > MAX_CONTACTS {
        return Err("The address book has too many email entries to sync.".into());
    }
    let mut unique = BTreeMap::new();
    for row in rows {
        let name = row
            .name
            .replace([',', '[', ']'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let address = row.address.trim().to_string();
        if name.is_empty()
            || name.len() > 200
            || name.contains('@')
            || name.chars().any(|c| {
                c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
            || !valid_address(&address)
        {
            continue;
        }
        unique
            .entry((name.to_lowercase(), address.to_lowercase()))
            .or_insert(Contact { name, address });
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for (name, _) in unique.keys() {
        *counts.entry(name.clone()).or_default() += 1;
    }
    let mut indices = BTreeMap::<String, usize>::new();
    Ok(unique
        .into_iter()
        .map(|((key, _), mut row)| {
            if counts.get(&key).copied().unwrap_or(0) > 1 {
                let index = indices.entry(key).or_default();
                *index += 1;
                row.name = format!("{} [email {}]", row.name, index);
            }
            row
        })
        .collect())
}

pub(super) fn valid_address(address: &str) -> bool {
    let Some((local, domain)) = address.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && address.len() <= 254
        && !address.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    '<' | '>' | '(' | ')' | ',' | ';' | ':' | '"' | '\\' | '[' | ']'
                )
        })
        && local
            .chars()
            .all(|c| c.is_alphanumeric() || ".!#$%&'*+-/=?^_`{|}~".contains(c))
        && domain
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

pub struct Handle(pub Arc<Book>);

pub fn open(app: &tauri::AppHandle, dir: &Path) -> Arc<Book> {
    Arc::new(Book::open(dir, Box::new(native::Device(app.clone()))))
}

#[tauri::command]
pub async fn contacts_info(app: tauri::AppHandle) -> Result<Info, String> {
    let book = app.state::<Handle>().0.clone();
    tauri::async_runtime::spawn_blocking(move || book.refresh(false))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_contact_sync(on: bool, app: tauri::AppHandle) -> Result<Info, String> {
    let book = app.state::<Handle>().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if on {
            book.refresh(true)
        } else {
            book.disable()
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_all_contacts(app: tauri::AppHandle) -> Result<Info, String> {
    set_contact_sync(false, app).await
}

#[cfg(target_os = "android")]
pub use native::plugin;

#[cfg(test)]
mod tests;
