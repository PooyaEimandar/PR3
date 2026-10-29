//! Whether a phone is on mobile data, so that a model download waits for Wi-Fi unless the
//! user allows mobile data in Settings (docs Sec. 6.1). iOS asks the Network framework,
//! which calls a path expensive on mobile data or a personal hotspot and constrained in
//! Low Data Mode; Android asks whether the active network is metered (`NetworkPlugin.kt`
//! in the app's Android project). Computers download on any network.

/// Whether this device asks before downloading over mobile data.
pub const PHONE: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Whether the network may cost the user money. When a phone cannot tell, it says yes,
/// so a download waits rather than spending someone's data plan.
pub fn metered(app: &tauri::AppHandle) -> bool {
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        apple::metered()
    }
    #[cfg(target_os = "android")]
    {
        android::metered(app)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = app;
        false
    }
}

#[cfg(target_os = "ios")]
mod apple {
    use std::ffi::{c_char, c_void};
    use std::sync::Once;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    use block2::{Block, RcBlock};

    const UNKNOWN: u8 = 0;
    const FREE: u8 = 1;
    const METERED: u8 = 2;

    /// The network as the monitor last saw it.
    static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
    static START: Once = Once::new();

    #[link(name = "Network", kind = "framework")]
    unsafe extern "C" {
        fn nw_path_monitor_create() -> *mut c_void;
        fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: *mut c_void);
        fn nw_path_monitor_set_update_handler(
            monitor: *mut c_void,
            handler: &Block<dyn Fn(*mut c_void)>,
        );
        fn nw_path_monitor_start(monitor: *mut c_void);
        fn nw_path_is_expensive(path: *mut c_void) -> bool;
        fn nw_path_is_constrained(path: *mut c_void) -> bool;
    }

    unsafe extern "C" {
        fn dispatch_queue_create(label: *const c_char, attr: *mut c_void) -> *mut c_void;
    }

    pub fn metered() -> bool {
        START.call_once(|| {
            let handler = RcBlock::new(|path: *mut c_void| {
                // SAFETY: the monitor passes a path that lives for the call.
                let costly = unsafe { nw_path_is_expensive(path) || nw_path_is_constrained(path) };
                STATE.store(if costly { METERED } else { FREE }, Ordering::Relaxed);
            });
            // SAFETY: plain calls into the Network framework and libdispatch. A null
            // attribute makes a serial queue, so updates arrive in order. The monitor
            // copies the handler, and is never released: it watches for the app's life.
            unsafe {
                let monitor = nw_path_monitor_create();
                if monitor.is_null() {
                    return;
                }
                let queue =
                    dispatch_queue_create(c"ai.pooya.pr3.network".as_ptr(), std::ptr::null_mut());
                nw_path_monitor_set_queue(monitor, queue);
                nw_path_monitor_set_update_handler(monitor, &handler);
                nw_path_monitor_start(monitor);
            }
        });
        // The monitor's first answer comes within milliseconds of starting it.
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match STATE.load(Ordering::Relaxed) {
                FREE => return false,
                METERED => return true,
                _ if Instant::now() >= until => return true,
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use serde::Deserialize;
    use tauri::Manager;
    use tauri::Wry;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};

    /// The Kotlin plugin, registered at startup.
    pub struct NetworkHandle(pub PluginHandle<Wry>);

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("pr3-network")
            .setup(|app, api| {
                let handle = api.register_android_plugin("ai.pooya.pr3", "NetworkPlugin")?;
                app.manage(NetworkHandle(handle));
                Ok(())
            })
            .build()
    }

    #[derive(Deserialize)]
    struct Status {
        metered: bool,
    }

    pub fn metered(app: &tauri::AppHandle) -> bool {
        app.try_state::<NetworkHandle>().is_none_or(|handle| {
            handle
                .0
                .run_mobile_plugin::<Status>("status", ())
                .map_or(true, |status| status.metered)
        })
    }
}

#[cfg(target_os = "android")]
pub use android::plugin;
