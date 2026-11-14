use super::{Snapshot, Source};

pub struct Device(pub tauri::AppHandle);

impl Source for Device {
    fn read(&self, request: bool) -> Result<Snapshot, String> {
        #[cfg(target_vendor = "apple")]
        {
            let _ = &self.0;
            apple::read(request)
        }
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            self.0
                .state::<AndroidHandle>()
                .0
                .run_mobile_plugin("read", serde_json::json!({"request": request}))
                .map_err(|e| e.to_string())
        }
        #[cfg(not(any(target_vendor = "apple", target_os = "android")))]
        {
            let _ = (&self.0, request);
            Ok(Snapshot {
                permission: super::Permission::Unavailable,
                contacts: Vec::new(),
            })
        }
    }
}

#[cfg(target_os = "android")]
struct AndroidHandle(tauri::plugin::PluginHandle<tauri::Wry>);

#[cfg(target_os = "android")]
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Manager;
    tauri::plugin::Builder::new("pr3-contacts")
        .setup(|app, api| {
            app.manage(AndroidHandle(
                api.register_android_plugin("ai.pooya.pr3", "ContactsPlugin")?,
            ));
            Ok(())
        })
        .build()
}

#[cfg(target_vendor = "apple")]
mod apple {
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use super::super::{MAX_CONTACTS, Permission, Snapshot};
    use block2::RcBlock;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use objc2_foundation::{NSArray, NSError, NSString};
    use pr3_core::trusted::Contact;

    #[link(name = "Contacts", kind = "framework")]
    unsafe extern "C" {}

    fn status() -> Permission {
        // SAFETY: CNContactStore's class method takes CNEntityType (NSInteger), with
        // contacts = 0, and returns CNAuthorizationStatus (NSInteger).
        let value: isize =
            unsafe { msg_send![class!(CNContactStore), authorizationStatusForEntityType: 0isize] };
        match value {
            0 => Permission::NotDetermined,
            1 => Permission::Restricted,
            2 => Permission::Denied,
            3 => Permission::Granted,
            4 => Permission::Limited,
            _ => Permission::Unavailable,
        }
    }

    pub fn read(request: bool) -> Result<Snapshot, String> {
        objc2::rc::autoreleasepool(|_| {
            let mut permission = status();
            // SAFETY: all selectors and argument types are from Contacts.framework.
            // CNContactStore is usable on this background thread. No contact field
            // is read unless that key was requested and authorization was granted.
            unsafe {
                let store: Retained<AnyObject> = msg_send![class!(CNContactStore), new];
                if request && permission == Permission::NotDetermined {
                    let (sender, receiver) = mpsc::channel();
                    let done = RcBlock::new(move |_granted: Bool, _error: *mut NSError| {
                        let _ = sender.send(());
                    });
                    let _: () = msg_send![&*store, requestAccessForEntityType: 0isize, completionHandler: &*done];
                    receiver
                        .recv_timeout(Duration::from_secs(120))
                        .map_err(|_| {
                            "Contact permission request did not finish. Try again in Settings."
                                .to_string()
                        })?;
                    permission = status();
                }
                if !permission.allowed() {
                    return Ok(Snapshot {
                        permission,
                        contacts: Vec::new(),
                    });
                }
                let keys = [
                    "givenName",
                    "familyName",
                    "organizationName",
                    "emailAddresses",
                ]
                .map(NSString::from_str);
                let keys = NSArray::from_retained_slice(&keys);
                let fetch: Allocated<AnyObject> = msg_send![class!(CNContactFetchRequest), alloc];
                let fetch: Retained<AnyObject> = msg_send![fetch, initWithKeysToFetch: &*keys];
                let rows = Mutex::new(Vec::new());
                let collect = RcBlock::new(|contact: *mut AnyObject, stop: *mut Bool| {
                    let given: Retained<NSString> = msg_send![contact, givenName];
                    let family: Retained<NSString> = msg_send![contact, familyName];
                    let organization: Retained<NSString> = msg_send![contact, organizationName];
                    let person = format!("{given} {family}").trim().to_string();
                    let name = if person.is_empty() {
                        organization.to_string()
                    } else {
                        person
                    };
                    let emails: Retained<NSArray<AnyObject>> = msg_send![contact, emailAddresses];
                    if let Ok(mut rows) = rows.lock() {
                        for email in emails.iter() {
                            let value: Retained<NSString> = msg_send![&*email, value];
                            rows.push(Contact {
                                name: name.clone(),
                                address: value.to_string(),
                            });
                            if rows.len() > MAX_CONTACTS {
                                *stop = Bool::YES;
                                break;
                            }
                        }
                    }
                });
                let mut error: *mut NSError = std::ptr::null_mut();
                let ok: Bool = msg_send![&*store, enumerateContactsWithFetchRequest: &*fetch, error: &mut error, usingBlock: &*collect];
                if !ok.as_bool() {
                    return Err("Could not read the device address book. Check contact access in system Settings.".into());
                }
                drop(collect);
                let contacts = rows.into_inner().map_err(|_| "contacts are unavailable")?;
                Ok(Snapshot {
                    permission: status(),
                    contacts,
                })
            }
        })
    }
}
