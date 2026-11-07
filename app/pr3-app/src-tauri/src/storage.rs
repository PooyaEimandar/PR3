//! App-only key custody and recovery. Keys never enter model tools or task history.
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex};

use pr3_memory_sqlite::{
    SqliteMemory,
    encryption::{DatabaseKey, requires_existing_key, verify_key},
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

pub struct State {
    key: Option<DatabaseKey>,
    problem: Option<String>,
    #[cfg(feature = "e2e")]
    fixture_dir: Option<std::path::PathBuf>,
}

#[derive(Serialize)]
pub struct Info {
    encrypted: bool,
    locked: bool,
    problem: Option<String>,
}

trait KeyStore {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, key: &str) -> Result<(), String>;
}

struct Platform<'a>(&'a AppHandle);

impl KeyStore for Platform<'_> {
    fn load(&self) -> Result<Option<String>, String> {
        #[cfg(target_os = "android")]
        {
            self.0
                .state::<crate::android_vault::VaultHandle>()
                .storage_key()
        }
        #[cfg(not(target_os = "android"))]
        {
            pr3_vault::storage_key(&service(self.0)).map_err(|e| e.to_string())
        }
    }
    fn save(&self, key: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            self.0
                .state::<crate::android_vault::VaultHandle>()
                .save_storage_key(key)
        }
        #[cfg(not(target_os = "android"))]
        {
            pr3_vault::save_storage_key(&service(self.0), key).map_err(|e| e.to_string())
        }
    }
}

#[cfg(not(target_os = "android"))]
fn service(app: &AppHandle) -> String {
    // Stable across iOS container changes; isolated from the E2E app identifier.
    format!("{}.storage", app.config().identifier)
}

fn key_lock(dir: &Path) -> Result<File, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let lock = options
        .open(dir.join("storage-key.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "Storage key is being used by another PR3 process".to_string())?;
    Ok(lock)
}

fn open_with(dir: &Path, keys: &dyn KeyStore) -> Result<(SqliteMemory, DatabaseKey), String> {
    let _lock = key_lock(dir)?;
    let path = dir.join("memory.sqlite");
    let key = match keys.load()? {
        Some(text) => DatabaseKey::from_hex(&Zeroizing::new(text)).map_err(|e| e.to_string())?,
        None if requires_existing_key(&path).map_err(|e| e.to_string())? => {
            return Err(
                "Your storage key is missing. Use your recovery code to unlock your saved data."
                    .into(),
            );
        }
        None => {
            let key = DatabaseKey::generate().map_err(|e| e.to_string())?;
            // Persist in the OS vault before creating or migrating any database.
            keys.save(key.hex().as_str())?;
            key
        }
    };
    let store = SqliteMemory::open_encrypted(&path, &key).map_err(|e| e.to_string())?;
    Ok((store, key))
}

pub fn open(app: &AppHandle) -> Option<Arc<Mutex<SqliteMemory>>> {
    let result = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| open_with(&dir, &Platform(app)));
    match result {
        Ok((store, key)) => {
            app.manage(State {
                key: Some(key),
                problem: None,
                #[cfg(feature = "e2e")]
                fixture_dir: None,
            });
            Some(Arc::new(Mutex::new(store)))
        }
        Err(problem) => {
            app.manage(State {
                key: None,
                problem: Some(problem),
                #[cfg(feature = "e2e")]
                fixture_dir: None,
            });
            None
        }
    }
}

pub fn require_ready(app: &AppHandle) -> Result<(), String> {
    let ready = app
        .try_state::<State>()
        .map_or_else(|| synthetic(app), |state| state.key.is_some());
    if !ready {
        Err("Storage is locked. Open Settings → Storage to recover access. Your data has not been reset.".into())
    } else {
        Ok(())
    }
}

fn synthetic(app: &AppHandle) -> bool {
    cfg!(feature = "e2e") && app.config().identifier == "ai.pooya.pr3.e2e"
}

#[tauri::command]
pub fn storage_info(app: AppHandle) -> Info {
    match app.try_state::<State>() {
        Some(state) => Info {
            encrypted: state.key.is_some(),
            locked: state.key.is_none(),
            problem: state.problem.clone(),
        },
        // Synthetic E2E fixtures deliberately do not access the platform keychain.
        None if synthetic(&app) => Info {
            encrypted: false,
            locked: false,
            problem: None,
        },
        None => Info {
            encrypted: false,
            locked: true,
            problem: Some("Storage is unavailable".into()),
        },
    }
}

#[tauri::command]
pub async fn storage_recovery_code(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        require_ready(&app)?;
        #[cfg(feature = "e2e")]
        if app.state::<State>().fixture_dir.is_some() {
            return app
                .state::<State>()
                .key
                .as_ref()
                .map(|key| key.recovery_code().to_string())
                .ok_or("Storage is locked".into());
        }
        #[cfg(target_os = "android")]
        app.state::<crate::android_vault::VaultHandle>()
            .authorize_storage_recovery()?;
        #[cfg(not(target_os = "android"))]
        pr3_vault::authorize_storage_recovery().map_err(|e| e.to_string())?;
        let state = app.try_state::<State>().ok_or("No encrypted storage")?;
        let key = state.key.as_ref().ok_or("Storage is locked")?;
        Ok(key.recovery_code().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn recover_with(dir: &Path, keys: &dyn KeyStore, code: &str) -> Result<(), String> {
    let _lock = key_lock(dir)?;
    let key = DatabaseKey::from_recovery_code(code).map_err(|e| e.to_string())?;
    // Verify every page first. Wrong codes, corrupt databases and missing files never
    // replace a vault entry and never initialize an empty database.
    verify_key(&dir.join("memory.sqlite"), &key).map_err(|e| e.to_string())?;
    keys.save(key.hex().as_str())
}

#[tauri::command]
pub async fn recover_storage(app: AppHandle, code: String) -> Result<(), String> {
    let code = Zeroizing::new(code);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.try_state::<State>().ok_or("No encrypted storage")?;
        if state.key.is_some() {
            return Err("Storage is already unlocked".into());
        }
        #[cfg(feature = "e2e")]
        if let Some(dir) = &state.fixture_dir {
            return recover_with(dir, &FixtureKeys(dir), &code);
        }
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        recover_with(&dir, &Platform(&app), &code)
    })
    .await
    .map_err(|e| e.to_string())?
}

// Debug-only native UI fixtures. setup() validates the marked private temp directory
// before calling this. No real keychain, biometric prompt, or user data is involved.
#[cfg(feature = "e2e")]
struct FixtureKeys<'a>(&'a Path);
#[cfg(feature = "e2e")]
impl KeyStore for FixtureKeys<'_> {
    fn load(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(self.0.join("fixture-storage.key")) {
            Ok(key) => Ok(Some(key)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn save(&self, key: &str) -> Result<(), String> {
        fs::write(self.0.join("fixture-storage.key"), key).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "e2e")]
pub(crate) fn open_fixture(app: &AppHandle, dir: &Path) -> Option<Arc<Mutex<SqliteMemory>>> {
    let (store, key, problem) = match open_with(dir, &FixtureKeys(dir)) {
        Ok((store, key)) => (Some(Arc::new(Mutex::new(store))), Some(key), None),
        Err(problem) => (None, None, Some(problem)),
    };
    app.manage(State {
        key,
        problem,
        fixture_dir: Some(dir.to_path_buf()),
    });
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Keys {
        value: RefCell<Option<String>>,
        fail: bool,
        unavailable: bool,
    }
    impl KeyStore for Keys {
        fn load(&self) -> Result<Option<String>, String> {
            if self.unavailable {
                return Err("vault locked".into());
            }
            Ok(self.value.borrow().clone())
        }
        fn save(&self, value: &str) -> Result<(), String> {
            if self.fail {
                return Err("vault locked".into());
            }
            *self.value.borrow_mut() = Some(value.to_string());
            Ok(())
        }
    }

    #[test]
    fn missing_key_never_resets_data_and_only_the_matching_code_restores_access() {
        let dir = std::env::temp_dir().join(format!("pr3-storage-recovery-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let keys = Keys::default();
        let (store, key) = open_with(&dir, &keys).unwrap();
        let code = key.recovery_code();
        drop(store);
        let before = fs::read(dir.join("memory.sqlite")).unwrap();
        *keys.value.borrow_mut() = None;
        assert!(open_with(&dir, &keys).is_err());
        assert!(keys.value.borrow().is_none());
        let wrong = DatabaseKey::generate().unwrap().recovery_code();
        assert!(recover_with(&dir, &keys, &wrong).is_err());
        assert!(keys.value.borrow().is_none());
        assert_eq!(fs::read(dir.join("memory.sqlite")).unwrap(), before);
        recover_with(&dir, &keys, &code).unwrap();
        drop(open_with(&dir, &keys).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn vault_write_failure_leaves_plaintext_and_new_database_untouched() {
        let dir =
            std::env::temp_dir().join(format!("pr3-storage-vault-failure-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let keys = Keys {
            fail: true,
            ..Default::default()
        };
        assert!(open_with(&dir, &keys).is_err());
        assert!(!dir.join("memory.sqlite").exists());
        drop(SqliteMemory::open(&dir.join("memory.sqlite")).unwrap());
        let before = fs::read(dir.join("memory.sqlite")).unwrap();
        assert!(open_with(&dir, &keys).is_err());
        assert_eq!(fs::read(dir.join("memory.sqlite")).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_vault_and_truncated_database_do_not_trigger_key_creation() {
        let dir =
            std::env::temp_dir().join(format!("pr3-storage-locked-vault-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let keys = Keys {
            unavailable: true,
            ..Default::default()
        };
        assert!(open_with(&dir, &keys).is_err());
        assert!(!dir.join("memory.sqlite").exists());
        assert!(keys.value.borrow().is_none());
        fs::write(dir.join("memory.sqlite"), []).unwrap();
        let keys = Keys::default();
        assert!(open_with(&dir, &keys).is_err());
        assert!(keys.value.borrow().is_none());
        assert!(fs::read(dir.join("memory.sqlite")).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
