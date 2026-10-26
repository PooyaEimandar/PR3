//! Android's speech engines: the on-device `SpeechRecognizer` and `TextToSpeech`,
//! through `SpeechPlugin.kt` in the app's Android project. They live in the app rather
//! than in `pr3-speech` because both need the app's activity.

use std::sync::{Arc, Mutex, PoisonError};

use pr3_speech::{Engines, Heard, OnHeard, Recognizer, SpeechError, Voice};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::plugin::mobile::PluginInvokeError;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Wry};

/// The Kotlin plugin, registered at startup.
pub struct SpeechHandle(pub PluginHandle<Wry>);

pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new("pr3-speech")
        .setup(|app, api| {
            let handle = api.register_android_plugin("ai.pooya.pr3", "SpeechPlugin")?;
            app.manage(SpeechHandle(handle));
            Ok(())
        })
        .build()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    on_device: bool,
    locale: String,
}

#[derive(Deserialize)]
struct Transcript {
    text: String,
}

type Listener = Arc<Mutex<Option<OnHeard>>>;

pub struct AndroidRecognizer {
    handle: PluginHandle<Wry>,
    locale: String,
    /// Where the running session's events go.
    listener: Listener,
    /// One channel for every session; the plugin sends its events through it.
    events: Channel<Value>,
}

/// The phone's recognizer and voice, both in the phone's language. The recognizer is
/// missing where the phone has no on-device one.
pub fn open(handle: &PluginHandle<Wry>) -> Engines {
    let status: Status = match handle.run_mobile_plugin("status", ()) {
        Ok(status) => status,
        Err(e) => {
            return Engines {
                recognizer: Err(speech_error(&e)),
                voice: Err(speech_error(&e)),
            };
        }
    };
    let voice: Box<dyn Voice> = Box::new(AndroidVoice {
        handle: handle.clone(),
        locale: status.locale.clone(),
    });
    if !status.on_device {
        let none = SpeechError::Unavailable(
            "no on-device speech recognition on this phone (Android 12 or later)".to_string(),
        );
        return Engines {
            recognizer: Err(none),
            voice: Ok(voice),
        };
    }
    let listener: Listener = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&listener);
    let events = Channel::new(move |body| {
        let InvokeResponseBody::Json(json) = body else {
            return Ok(());
        };
        let Ok(event) = serde_json::from_str::<Value>(&json) else {
            return Ok(());
        };
        let guard = sink.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(on_heard) = guard.as_ref() else {
            return Ok(());
        };
        let text = |name: &str| event.get(name).and_then(Value::as_str);
        if let Some(partial) = text("partial") {
            on_heard(Heard::Partial(partial));
        } else if let Some(why) = text("error") {
            on_heard(Heard::Failed(&error_from(text("code"), why.to_string())));
        }
        Ok(())
    });
    Engines {
        recognizer: Ok(Box::new(AndroidRecognizer {
            handle: handle.clone(),
            locale: status.locale,
            listener,
            events,
        })),
        voice: Ok(voice),
    }
}

impl Recognizer for AndroidRecognizer {
    fn start(&self, on_heard: OnHeard) -> Result<(), SpeechError> {
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = Some(on_heard);
        self.handle
            .run_mobile_plugin::<Value>(
                "start",
                json!({ "onEvent": self.events, "locale": self.locale }),
            )
            .map(drop)
            .map_err(|e| speech_error(&e))
    }

    fn stop(&self) -> Result<String, SpeechError> {
        let reply: Result<Transcript, _> = self.handle.run_mobile_plugin("stop", ());
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = None;
        Ok(reply.map_err(|e| speech_error(&e))?.text)
    }

    fn cancel(&self) {
        let _ = self.handle.run_mobile_plugin::<Value>("cancel", ());
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn locale(&self) -> &str {
        &self.locale
    }
}

pub struct AndroidVoice {
    handle: PluginHandle<Wry>,
    locale: String,
}

impl Voice for AndroidVoice {
    fn speak(&self, text: &str) -> Result<(), SpeechError> {
        self.handle
            .run_mobile_plugin::<Value>("speak", json!({ "text": text, "locale": self.locale }))
            .map(drop)
            .map_err(|e| speech_error(&e))
    }

    fn stop(&self) {
        let _ = self.handle.run_mobile_plugin::<Value>("stopSpeaking", ());
    }
}

fn speech_error(error: &PluginInvokeError) -> SpeechError {
    let PluginInvokeError::InvokeRejected(e) = error else {
        return SpeechError::Platform(error.to_string());
    };
    let message = e
        .message
        .clone()
        .unwrap_or_else(|| "speech failed".to_string());
    error_from(e.code.as_deref(), message)
}

/// The plugin's error codes, as `SpeechError`s.
fn error_from(code: Option<&str>, message: String) -> SpeechError {
    match code {
        Some("denied") => SpeechError::Denied(message),
        Some("unavailable") => SpeechError::Unavailable(message),
        Some("busy") => SpeechError::Busy,
        Some("idle") => SpeechError::NotListening,
        _ => SpeechError::Platform(message),
    }
}
