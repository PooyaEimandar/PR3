//! A reviewed draft handed to the device's default email app. Opening a composer is
//! not delivery: the user still reviews and presses Send there, and that app owns
//! its account, offline outbox and delivery errors.

use pr3_core::trusted::Mail;

pub const PROVIDER: &str = "mail:device";
pub const OPENED: &str = "Email draft opened in your email app";

/// RFC 6068: encode each value independently so data cannot add cc, bcc, another
/// recipient or a URI fragment. Addresses come only from the validated address book.
pub fn uri(mail: &Mail) -> Result<String, String> {
    if mail.to.is_empty()
        || mail
            .to
            .iter()
            .any(|c| !crate::contacts::valid_address(&c.address))
    {
        return Err("The email needs valid address book recipients.".into());
    }
    let to = mail
        .to
        .iter()
        .map(|c| encode(&c.address))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        "mailto:{to}?subject={}&body={}",
        encode(&mail.subject),
        encode(&mail.body.replace("\r\n", "\n").replace('\n', "\r\n"))
    ))
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn open(app: &tauri::AppHandle, mail: &Mail) -> Result<(), String> {
    let uri = uri(mail)?;
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        app.state::<AndroidHandle>()
            .0
            .run_mobile_plugin::<serde_json::Value>("open", serde_json::json!({"uri": uri}))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(target_vendor = "apple")]
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || apple_open(&uri, sender))
            .map_err(|e| e.to_string())?;
        receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| crate::task::UNKNOWN.to_string())?
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "android")))]
    {
        let _ = (app, uri);
        Err("Opening an email app is supported on Android, iOS and macOS.".into())
    }
}

#[cfg(target_vendor = "apple")]
fn apple_open(uri: &str, sender: std::sync::mpsc::Sender<Result<(), String>>) {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::{NSString, NSURL};
    let Some(url) = NSURL::URLWithString(&NSString::from_str(uri)) else {
        let _ = sender.send(Err("Could not prepare the email draft.".into()));
        return;
    };
    let outcome = |opened: bool| {
        if opened {
            Ok(())
        } else {
            Err("No email app could open this draft. Install and configure an email app, then try again.".into())
        }
    };
    // SAFETY: dispatched by run_on_main_thread. These system selectors take NSURL
    // and return BOOL / invoke the completion block with BOOL; the URL is retained
    // through the call and UIKit copies its asynchronous completion block.
    unsafe {
        #[cfg(target_os = "macos")]
        {
            let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
            let opened: Bool = msg_send![&*workspace, openURL: &*url];
            let _ = sender.send(outcome(opened.as_bool()));
        }
        #[cfg(target_os = "ios")]
        {
            let application: Retained<AnyObject> =
                msg_send![class!(UIApplication), sharedApplication];
            let options: Retained<AnyObject> = msg_send![class!(NSDictionary), new];
            let completion = block2::RcBlock::new(move |opened: Bool| {
                let _ = sender.send(outcome(opened.as_bool()));
            });
            let _: () = msg_send![&*application, openURL: &*url, options: &*options, completionHandler: &*completion];
        }
    }
}

#[cfg(target_os = "android")]
struct AndroidHandle(tauri::plugin::PluginHandle<tauri::Wry>);

#[cfg(target_os = "android")]
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Manager;
    tauri::plugin::Builder::new("pr3-mail")
        .setup(|app, api| {
            app.manage(AndroidHandle(
                api.register_android_plugin("ai.pooya.pr3", "MailPlugin")?,
            ));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr3_core::trusted::Contact;

    #[test]
    fn a_draft_cannot_inject_mailto_headers_or_lose_unicode_and_line_breaks() {
        let mail = Mail {
            to: vec![Contact {
                name: "Test".into(),
                address: "a+b&cc=other@example.com".into(),
            }],
            subject: "سلام &bcc=evil@example.com? #".into(),
            body: "First\nSecond + %".into(),
        };
        let url = uri(&mail).unwrap();
        assert_eq!(
            url,
            "mailto:a%2Bb%26cc%3Dother%40example.com?subject=%D8%B3%D9%84%D8%A7%D9%85%20%26bcc%3Devil%40example.com%3F%20%23&body=First%0D%0ASecond%20%2B%20%25"
        );
        let mut invalid = mail;
        invalid.to[0].address = "a@example.com,b@example.com".into();
        assert!(uri(&invalid).is_err());
    }
}
