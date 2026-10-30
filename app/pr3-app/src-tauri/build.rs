fn main() {
    // The bundled frontend: Tauri embeds the files it finds there and so notices when
    // one changes, but not when one appears, as ui/main.js does once src/ is compiled.
    println!("cargo:rerun-if-changed=../ui");
    // Android's library, with its segments aligned for 16 KB memory pages, which phones
    // from Android 15 on may use and Google Play requires; NDK 27's linker aligns them for
    // 4 KB, and Android then warns on opening the app that it is not 16 KB-compatible.
    // Here and not in rustflags, which the Tauri CLI sets for Android through the
    // environment.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-z,max-page-size=16384");
    }
    // Folders are named in the singular, so Tauri is told where the capabilities are.
    let attributes = tauri_build::Attributes::new().capabilities_path_pattern("./capability/**/*");
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("{error:#}");
    }
}
