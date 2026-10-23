//! Voice notes (docs Sec. 4.3, "Speech, voice first"): push-to-talk speech-to-text and,
//! by the user's choice, spoken replies. Recognition and voices run on the device
//! (`pr3-speech`); no audio is sent anywhere or kept. A transcript is sent like typed
//! text, so the memories it leads to are proposals with the user's words as source.

use pr3_speech::{Engines, Heard, Recognizer, SpeechError, Voice};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

pub struct Speech {
    recognizer: Result<Box<dyn Recognizer>, SpeechError>,
    voice: Result<Box<dyn Voice>, SpeechError>,
}

impl Speech {
    /// This device's engines, in the system's language. Opening asks for nothing: the
    /// permission prompts come with the first voice note.
    pub fn open(app: &AppHandle) -> Self {
        #[cfg(target_os = "android")]
        let engines = match app.try_state::<crate::android_speech::SpeechHandle>() {
            Some(plugin) => crate::android_speech::open(&plugin.0),
            None => {
                let missing = || SpeechError::Platform("the speech plugin is not loaded".into());
                Engines {
                    recognizer: Err(missing()),
                    voice: Err(missing()),
                }
            }
        };
        #[cfg(not(target_os = "android"))]
        let engines = {
            let _ = app;
            pr3_speech::open_platform(None)
        };
        let Engines { recognizer, voice } = engines;
        match &recognizer {
            Ok(r) => eprintln!(
                "PR3: voice notes in {}, recognized on this device",
                r.locale()
            ),
            Err(e) => eprintln!("PR3: no voice notes: {e}"),
        }
        Self { recognizer, voice }
    }

    fn recognizer(&self) -> Result<&dyn Recognizer, String> {
        self.recognizer.as_deref().map_err(ToString::to_string)
    }

    fn voice(&self) -> Result<&dyn Voice, String> {
        self.voice.as_deref().map_err(ToString::to_string)
    }
}

/// What the UI hears while a voice note runs.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Hearing {
    /// The transcript so far.
    Partial { text: String },
    /// The voice note ended on its own; nothing will be sent.
    Failed { error: String },
}

#[derive(Serialize)]
pub struct SpeechStatus {
    /// Voice notes work here; otherwise `why_not` says why.
    listen: bool,
    why_not: Option<String>,
    /// Replies can be spoken.
    speak: bool,
    locale: Option<String>,
}

#[tauri::command]
pub fn speech_status(speech: tauri::State<'_, Speech>) -> SpeechStatus {
    SpeechStatus {
        listen: speech.recognizer.is_ok(),
        why_not: speech.recognizer.as_ref().err().map(ToString::to_string),
        speak: speech.voice.is_ok(),
        locale: speech
            .recognizer
            .as_ref()
            .ok()
            .map(|r| r.locale().to_string()),
    }
}

/// Starts a voice note: the transcript so far, or a failure, streams through
/// `on_event`. The first time, the system asks for the microphone and speech
/// recognition, and this waits for the answer.
#[tauri::command]
pub async fn start_listening(app: AppHandle, on_event: Channel<Hearing>) -> Result<(), String> {
    blocking(app, move |speech| {
        if let Ok(voice) = speech.voice() {
            voice.stop();
        }
        speech
            .recognizer()?
            .start(Box::new(move |heard| {
                let event = match heard {
                    Heard::Partial(text) => Hearing::Partial {
                        text: text.to_string(),
                    },
                    Heard::Failed(e) => Hearing::Failed {
                        error: e.to_string(),
                    },
                };
                // A closed channel only means the window went away.
                let _ = on_event.send(event);
            }))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Ends the voice note and returns what was heard, empty if nothing.
#[tauri::command]
pub async fn stop_listening(app: AppHandle) -> Result<String, String> {
    blocking(app, |speech| {
        speech.recognizer()?.stop().map_err(|e| e.to_string())
    })
    .await
}

/// Ends the voice note and drops what was heard.
#[tauri::command]
pub async fn cancel_listening(app: AppHandle) -> Result<(), String> {
    blocking(app, |speech| {
        speech.recognizer()?.cancel();
        Ok(())
    })
    .await
}

/// Speaks a reply, read as plain sentences.
#[tauri::command]
pub async fn speak(app: AppHandle, text: String) -> Result<(), String> {
    blocking(app, move |speech| {
        let spoken = pr3_speech::speakable(&text);
        if spoken.is_empty() {
            return Ok(());
        }
        speech.voice()?.speak(&spoken).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn stop_speaking(app: AppHandle) -> Result<(), String> {
    blocking(app, |speech| {
        speech.voice()?.stop();
        Ok(())
    })
    .await
}

/// Engines block (on permission prompts, on the recognizer's last words), so they run
/// off the main thread and off the async runtime's workers.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&Speech) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app.state::<Speech>()))
        .await
        .map_err(|e| e.to_string())?
}
