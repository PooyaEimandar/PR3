//! Builds and links the Swift half of the speech engines on Apple platforms (swift/).

fn main() {
    println!("cargo:rerun-if-changed=swift/Package.swift");
    println!("cargo:rerun-if-changed=swift/Source");
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        swift::link();
    }
}

/// Swift builds only on a Mac, so swift-rs is a build dependency only there: Cargo
/// matches a build dependency's platform against the machine that builds.
#[cfg(target_vendor = "apple")]
mod swift {
    pub fn link() {
        // Inside an iOS build Xcode exports SDKROOT for the device or simulator, which
        // breaks SwiftPM's manifest step, a host build. Tauri clears it the same way.
        let sdk_root = std::env::var_os("SDKROOT");
        // SAFETY: build scripts are single-threaded; nothing reads the environment
        // concurrently.
        unsafe { std::env::remove_var("SDKROOT") };
        let ios = std::env::var("IPHONEOS_DEPLOYMENT_TARGET").unwrap_or_else(|_| "13.0".into());
        // On-device recognition needs macOS 10.15 and iOS 13; Package.swift asks for 11 and 13.
        swift_rs::SwiftLinker::new("11.0")
            .with_ios(&ios)
            .with_package("PR3Speech", "swift")
            .link();
        if let Some(root) = sdk_root {
            // SAFETY: as above.
            unsafe { std::env::set_var("SDKROOT", root) };
        }
    }
}

#[cfg(not(target_vendor = "apple"))]
mod swift {
    pub fn link() {
        panic!("the Swift half of pr3-speech builds only on macOS");
    }
}
