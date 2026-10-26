//! Apple platforms, through the Swift half in `swift/`: the Speech framework with
//! on-device recognition required, and the system's voices.
//!
//! This is the crate's one audited `unsafe` boundary (docs Sec. 4.2): plain C calls into
//! Swift, with strings in and JSON strings out. No callback crosses it: a thread here
//! reads the session's events, waiting in Swift for each.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use serde_json::Value;

use crate::{Heard, OnHeard, Recognizer, SpeechError, Voice};

unsafe extern "C" {
    fn pr3_speech_free(text: *mut c_char);
    fn pr3_speech_open(locale: *const c_char) -> *mut c_char;
    fn pr3_speech_authorize() -> *mut c_char;
    fn pr3_speech_start() -> *mut c_char;
    fn pr3_speech_next(timeout_ms: isize) -> *mut c_char;
    fn pr3_speech_stop();
    fn pr3_speech_cancel();
    fn pr3_speech_speak(text: *const c_char, locale: *const c_char) -> *mut c_char;
    fn pr3_speech_stop_speaking();
}

/// How long one wait for the next event lasts.
const POLL_MS: isize = 250;

/// Takes ownership of a reply from the Swift side and frees it.
fn take(reply: *mut c_char) -> Value {
    if reply.is_null() {
        return Value::Null;
    }
    // SAFETY: every Swift function returns a NUL-terminated string from strdup, which
    // is read here and then freed exactly once, by the side that allocated it.
    let text = unsafe { CStr::from_ptr(reply) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: as above; `reply` is not used after this.
    unsafe { pr3_speech_free(reply) };
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// The `ok` payload of a reply, or the error it reports.
fn ok(reply: *mut c_char) -> Result<String, SpeechError> {
    let value = take(reply);
    let field = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_string);
    if let Some(ok) = field("ok") {
        return Ok(ok);
    }
    if let Some(what) = field("denied") {
        return Err(SpeechError::Denied(what));
    }
    if let Some(why) = field("unavailable") {
        return Err(SpeechError::Unavailable(why));
    }
    Err(SpeechError::Platform(field("error").unwrap_or_else(|| {
        "no answer from the Swift side".to_string()
    })))
}

fn c_string(text: &str) -> CString {
    // Interior NULs cannot be spoken or recognized; drop any, never fail.
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

pub(crate) struct AppleRecognizer {
    locale: String,
    /// The thread reading the running session's events; it ends with the transcript.
    session: Mutex<Option<JoinHandle<Result<String, SpeechError>>>>,
}

impl AppleRecognizer {
    /// Needs no permission: it only checks that the language has an on-device model.
    pub(crate) fn open(locale: Option<&str>) -> Result<Self, SpeechError> {
        let wanted = c_string(locale.unwrap_or(""));
        // SAFETY: `wanted` is a valid C string for the duration of the call.
        let locale = ok(unsafe { pr3_speech_open(wanted.as_ptr()) })?;
        Ok(Self {
            locale,
            session: Mutex::new(None),
        })
    }
}

/// Reads one session's events until it ends.
fn read_events(on_heard: &OnHeard) -> Result<String, SpeechError> {
    loop {
        // SAFETY: takes a number and returns a reply that `take` frees.
        let event = take(unsafe { pr3_speech_next(POLL_MS) });
        let text = |name: &str| event.get(name).and_then(Value::as_str);
        if let Some(partial) = text("partial") {
            on_heard(Heard::Partial(partial));
        } else if let Some(heard) = text("final") {
            return Ok(heard.trim().to_string());
        } else if event.get("cancelled").is_some() {
            return Ok(String::new());
        } else if event.get("timeout").is_none() {
            let failure = SpeechError::Platform(
                text("error")
                    .unwrap_or("the recognizer stopped")
                    .to_string(),
            );
            on_heard(Heard::Failed(&failure));
            return Err(failure);
        }
    }
}

impl Recognizer for AppleRecognizer {
    fn start(&self, on_heard: OnHeard) -> Result<(), SpeechError> {
        let mut session = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        if session.as_ref().is_some_and(|s| !s.is_finished()) {
            return Err(SpeechError::Busy);
        }
        // SAFETY: takes nothing; blocks on the system's prompts, never on this thread's
        // own work.
        ok(unsafe { pr3_speech_authorize() })?;
        // SAFETY: takes nothing and returns a reply that `ok` frees.
        ok(unsafe { pr3_speech_start() })?;
        *session = Some(std::thread::spawn(move || read_events(&on_heard)));
        Ok(())
    }

    fn stop(&self) -> Result<String, SpeechError> {
        let session = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(SpeechError::NotListening)?;
        // SAFETY: takes nothing; the session then ends within two seconds.
        unsafe { pr3_speech_stop() };
        session
            .join()
            .unwrap_or_else(|_| Err(SpeechError::Platform("the listener failed".to_string())))
    }

    fn cancel(&self) {
        let session = self
            .session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(session) = session {
            // SAFETY: takes nothing; the session ends at once.
            unsafe { pr3_speech_cancel() };
            let _ = session.join();
        }
    }

    fn locale(&self) -> &str {
        &self.locale
    }
}

pub(crate) struct AppleVoice {
    locale: CString,
}

impl AppleVoice {
    pub(crate) fn new(locale: Option<&str>) -> Self {
        Self {
            locale: c_string(locale.unwrap_or("")),
        }
    }
}

impl Voice for AppleVoice {
    fn speak(&self, text: &str) -> Result<(), SpeechError> {
        let text = c_string(text);
        // SAFETY: both strings stay alive for the call; Swift copies them.
        ok(unsafe { pr3_speech_speak(text.as_ptr(), self.locale.as_ptr()) }).map(drop)
    }

    fn stop(&self) {
        // SAFETY: takes nothing.
        unsafe { pr3_speech_stop_speaking() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening needs no permission prompt; listening does, so it is tested in the app.
    /// Skipped where English has no on-device model (CI's VMs).
    #[test]
    fn english_opens_on_device_and_an_unknown_language_does_not() {
        match AppleRecognizer::open(Some("en-US")) {
            Ok(recognizer) => {
                assert!(
                    recognizer.locale().starts_with("en"),
                    "{}",
                    recognizer.locale()
                );
                assert_eq!(recognizer.stop(), Err(SpeechError::NotListening));
                recognizer.cancel();
            }
            Err(SpeechError::Unavailable(why)) => eprintln!("{why}; skipped"),
            Err(other) => panic!("{other}"),
        }
        assert!(matches!(
            AppleRecognizer::open(Some("xx-XX")),
            Err(SpeechError::Unavailable(_))
        ));
    }
}
