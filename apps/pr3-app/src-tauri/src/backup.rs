//! Keeps PR3's data out of cloud and computer backups on iOS, so memory, traces and
//! models stay on the device as their consent scope says (`device_only`). Android does
//! the same in its manifest; on macOS the user's own Time Machine backups are left alone.

use std::path::Path;

/// Marks `dir` and everything in it as excluded from backups.
#[cfg(target_os = "ios")]
pub fn exclude(dir: &Path) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSNumber, NSString, NSURL, NSURLIsExcludedFromBackupKey};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
    let yes = NSNumber::new_bool(true);
    let value: &AnyObject = &yes;
    // SAFETY: NSURLIsExcludedFromBackupKey is a constant Foundation exports, and the
    // value this key takes is an NSNumber holding a BOOL, which `value` is.
    unsafe { url.setResourceValue_forKey_error(Some(value), NSURLIsExcludedFromBackupKey) }
        .map_err(|e| format!("{e:?}"))
}

#[cfg(not(target_os = "ios"))]
pub fn exclude(_dir: &Path) -> Result<(), String> {
    Ok(())
}
