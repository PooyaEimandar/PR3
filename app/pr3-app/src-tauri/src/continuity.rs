//! App coordination for continuity, enabled by pairing and disabled by unpairing.
use crate::{NodeHandle, host::Host, hybrid};
use pr3_memory_sqlite::{SqliteMemory, continuity::Info};
use pr3_protocol::{ContinuityReply, ContinuityRequest};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub struct State {
    pub store: Arc<Mutex<SqliteMemory>>,
    pairing: Mutex<()>,
    exchange: Mutex<()>,
    problem: Mutex<Option<String>>,
}
impl State {
    pub fn new(store: Arc<Mutex<SqliteMemory>>) -> Self {
        Self {
            store,
            pairing: Mutex::new(()),
            exchange: Mutex::new(()),
            problem: Mutex::new(None),
        }
    }
}
#[derive(serde::Serialize)]
pub struct View {
    #[serde(flatten)]
    info: Info,
    problem: Option<String>,
}
fn state(app: &tauri::AppHandle) -> Result<tauri::State<'_, State>, String> {
    app.try_state::<State>()
        .ok_or("Continuity storage is unavailable".into())
}
fn now() -> pr3_core::Timestamp {
    crate::host::now()
}
fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("continuity-changed", ());
}
fn peer(app: &tauri::AppHandle, id: &str) -> Result<String, String> {
    if let Some(link) = app.try_state::<hybrid::Link>() {
        return link
            .node()
            .filter(|p| p.id() == id)
            .map(|p| p.public_key)
            .ok_or("Pair this computer first".into());
    }
    app.try_state::<NodeHandle>()
        .and_then(|h| {
            h.0.as_ref()
                .and_then(|n| n.phones().into_iter().find(|p| p.id() == id))
        })
        .map(|p| p.public_key)
        .ok_or("Pair this phone first".into())
}
/// The pairing files are authoritative, including after an upgrade or interrupted
/// unpair. There is no independent continuity preference to become out of sync.
pub fn refresh_pairing(app: &tauri::AppHandle) {
    let _ = pairing_change(app, || Ok(()));
}
/// Serialize local pairing mutations, status refreshes and phone reply application.
/// A refresh started before Unpair cannot re-enable continuity after it returns.
pub fn pairing_change<T>(
    app: &tauri::AppHandle,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let Some(s) = app.try_state::<State>() else {
        return change();
    };
    let _guard = s.pairing.lock().map_err(|_| "Pairing unavailable")?;
    let result = change();
    reconcile_pairing(app, &s);
    result
}
fn reconcile_pairing(app: &tauri::AppHandle, s: &State) {
    let peers: Vec<String> = if let Some(link) = app.try_state::<hybrid::Link>() {
        link.node().into_iter().map(|p| p.public_key).collect()
    } else {
        app.try_state::<NodeHandle>()
            .and_then(|h| h.0.clone())
            .map(|n| n.phones().into_iter().map(|p| p.public_key).collect())
            .unwrap_or_default()
    };
    let result = (|| {
        let mut db = s
            .store
            .lock()
            .map_err(|_| "Memory unavailable".to_string())?;
        let before = db.continuity_info().map_err(|e| e.to_string())?;
        db.continuity_pairing(&peers).map_err(|e| e.to_string())?;
        let after = db.continuity_info().map_err(|e| e.to_string())?;
        Ok::<_, String>(before.enabled != after.enabled || before.peer != after.peer)
    })();
    if let Ok(mut problem) = s.problem.lock() {
        match result {
            Ok(true) => *problem = None,
            Ok(false) => {}
            Err(reason) => *problem = Some(reason),
        }
    }
}
#[tauri::command]
pub fn continuity_info(app: tauri::AppHandle) -> Result<View, String> {
    refresh_pairing(&app);
    let s = state(&app)?;
    let info = s
        .store
        .lock()
        .map_err(|_| "Memory unavailable")?
        .continuity_info()
        .map_err(|e| e.to_string())?;
    let problem = s
        .problem
        .lock()
        .map_err(|_| "Sync status unavailable")?
        .clone();
    Ok(View { info, problem })
}
#[tauri::command]
pub fn share_item(
    app: tauri::AppHandle,
    kind: String,
    id: i64,
    move_ownership: bool,
) -> Result<(), String> {
    refresh_pairing(&app);
    let s = state(&app)?;
    // Only an explicitly paired peer can receive a handoff, including after unpair.
    let configured = s
        .store
        .lock()
        .map_err(|_| "Memory unavailable")?
        .continuity_info()
        .map_err(|e| e.to_string())?
        .peer
        .ok_or("Pair a device first")?;
    if peer(&app, &configured[..16])? != configured {
        return Err("Pairing changed".into());
    }
    app.state::<Host>().continuity_edit(&s.store, |db| {
        db.continuity_share(&kind, id, move_ownership, now())
    })?;
    changed(&app);
    Ok(())
}
#[tauri::command]
pub fn recover_item(app: tauri::AppHandle, kind: String, id: i64) -> Result<i64, String> {
    refresh_pairing(&app);
    let s = state(&app)?;
    let id = app
        .state::<Host>()
        .continuity_edit(&s.store, |db| db.continuity_recover(&kind, id, now()))?;
    changed(&app);
    Ok(id)
}

pub struct NodeSync(pub tauri::AppHandle);
impl pr3_hybrid::Continuity for NodeSync {
    fn exchange(&self, key: &str, request: ContinuityRequest) -> Result<ContinuityReply, String> {
        let s = state(&self.0)?;
        let _serial = s
            .exchange
            .try_lock()
            .map_err(|_| "Another sync is in progress; retry shortly.")?;
        let result = self.0.state::<Host>().continuity_edit(&s.store, |db| {
            // Node holds its peer lock through this callback. This key was
            // authenticated and remains paired until the exchange completes.
            db.continuity_enable(key, true)?;
            db.continuity_exchange(key, request, now())
        });
        if let Ok(mut problem) = s.problem.lock() {
            *problem = result.as_ref().err().cloned();
        }
        if result.is_ok() {
            changed(&self.0);
        }
        result
    }
}
#[tauri::command]
pub async fn sync_continuity(app: tauri::AppHandle) -> Result<View, String> {
    let work = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || sync_phone(&work))
        .await
        .map_err(|e| e.to_string())?;
    let s = state(&app)?;
    if let Ok(mut problem) = s.problem.lock() {
        *problem = result.as_ref().err().cloned();
    }
    result?;
    continuity_info(app)
}
fn sync_phone(app: &tauri::AppHandle) -> Result<(), String> {
    refresh_pairing(app);
    let s = state(app)?;
    let _serial = s
        .exchange
        .try_lock()
        .map_err(|_| "Sync is already running")?;
    let link = app
        .try_state::<hybrid::Link>()
        .ok_or("Open PR3 on the paired phone to sync with this computer.")?;
    let client = link.client().ok_or("Pair your computer first")?;
    let key = &client.node().public_key;
    // Several bounded pages per foreground tick. Remaining pages resume next tick.
    for _ in 0..8 {
        let request = s
            .store
            .lock()
            .map_err(|_| "Memory unavailable")?
            .continuity_request(key, now())
            .map_err(|e| e.to_string())?;
        let sent = request.page.changes.len();
        let reply = client.continuity(request).map_err(|e| e.to_string())?;
        let received = reply.page.changes.len();
        pairing_change(app, || {
            if link.node().as_ref().map(|p| p.public_key.as_str()) != Some(key) {
                return Err("Pairing changed; reply discarded".into());
            }
            app.state::<Host>()
                .continuity_edit(&s.store, |db| db.continuity_accept(key, reply, now()))
        })?;
        if sent == 0 && received == 0 {
            break;
        }
    }
    changed(app);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::host::MockBackend;
    use pr3_core::SessionId;
    use pr3_memory_sqlite::{Said, TaskRecord, TaskState};
    #[test]
    fn host_handoff_refreshes_cached_ownership_and_reconnect_deletes_core_history() {
        let phone = "a".repeat(64);
        let computer = "b".repeat(64);
        let store = Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()));
        {
            let mut db = store.lock().unwrap();
            db.continuity_init(&phone).unwrap();
            db.continuity_pairing(std::slice::from_ref(&computer))
                .unwrap();
            db.save_task(&TaskRecord {
                id: 7,
                title: "Keep this goal".into(),
                parent: None,
                state: TaskState::Done,
                updated_at: now(),
            })
            .unwrap();
            db.add_said(7, now(), &Said::User("Remember the goal".into()))
                .unwrap();
        }
        let host = crate::compose_host(
            Arc::new(MockBackend::default()),
            Some(store.clone()),
            None,
            365,
            None,
        )
        .unwrap();
        host.continue_task(SessionId(7)).unwrap();
        host.continuity_edit(&store, |db| db.continuity_share("task", 7, true, now()))
            .unwrap();
        assert!(!host.task(SessionId(7)).unwrap().owned);
        assert!(host.continue_task(SessionId(7)).is_err());
        assert!(host.finish_task(SessionId(7)).is_err());
        assert!(host.current_task().unwrap().is_none());
        let mut remote = SqliteMemory::open_in_memory().unwrap();
        remote.continuity_init(&computer).unwrap();
        remote
            .continuity_pairing(std::slice::from_ref(&phone))
            .unwrap();
        let req = store
            .lock()
            .unwrap()
            .continuity_request(&computer, now())
            .unwrap();
        let reply = remote.continuity_exchange(&phone, req, now()).unwrap();
        host.continuity_edit(&store, |db| db.continuity_accept(&computer, reply, now()))
            .unwrap();
        let imported = remote.tasks().unwrap()[0].record.id;
        assert_eq!(remote.tasks().unwrap()[0].chat.len(), 1);
        remote.delete_task(imported, now()).unwrap();
        let req = store
            .lock()
            .unwrap()
            .continuity_request(&computer, now())
            .unwrap();
        let reply = remote.continuity_exchange(&phone, req, now()).unwrap();
        host.continuity_edit(&store, |db| db.continuity_accept(&computer, reply, now()))
            .unwrap();
        assert!(host.task(SessionId(7)).is_err());
    }
    #[test]
    fn real_host_turn_moves_resumes_and_recovery_blocks_retry_of_stale_results() {
        tauri::async_runtime::block_on(async {
            let a = "a".repeat(64);
            let b = "b".repeat(64);
            let store = Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()));
            {
                let mut db = store.lock().unwrap();
                db.continuity_init(&a).unwrap();
                db.continuity_pairing(std::slice::from_ref(&b)).unwrap();
            }
            let host = crate::compose_host(
                Arc::new(MockBackend::default()),
                Some(store.clone()),
                None,
                365,
                None,
            )
            .unwrap();
            host.send("Plan a quiet weekend".into(), 0, |_| {})
                .await
                .unwrap();
            let id = host.current_task().unwrap().unwrap().card.id;
            host.continuity_edit(&store, |db| {
                db.continuity_share("task", id as i64, true, now())
            })
            .unwrap();
            let remote = Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()));
            {
                let mut db = remote.lock().unwrap();
                db.continuity_init(&b).unwrap();
                db.continuity_pairing(std::slice::from_ref(&a)).unwrap();
                let request = store.lock().unwrap().continuity_request(&b, now()).unwrap();
                db.continuity_exchange(&a, request, now()).unwrap();
            }
            let other = crate::compose_host(
                Arc::new(MockBackend::default()),
                Some(remote.clone()),
                None,
                365,
                None,
            )
            .unwrap();
            let received = other.task_cards().unwrap()[0].id;
            other.continue_task(SessionId(received)).unwrap();
            other
                .send("Include a walk".into(), 0, |_| {})
                .await
                .unwrap();
            assert!(other.task(SessionId(received)).unwrap().chat.iter().any(|step|matches!(&step.entry,crate::task::Entry::User{text} if text=="Plan a quiet weekend")));
            // The phone has an older replica and cannot know what just happened remotely.
            store.lock().unwrap().continuity_pairing(&[]).unwrap();
            let recovered = host
                .continuity_edit(&store, |db| db.continuity_recover("task", id as i64, now()))
                .unwrap();
            let view = host.task(SessionId(recovered as u64)).unwrap();
            assert!(view.owned);
            assert!(!view.retryable);
            assert!(view.chat.iter().any(|step|matches!(&step.entry,crate::task::Entry::Failed{reason} if crate::task::unknown(reason))));
        });
    }
}
