fn main() {
    // The bundled frontend: Tauri embeds the files it finds there and so notices when
    // one changes, but not when one appears, as ui/main.js does once src/ is compiled.
    println!("cargo:rerun-if-changed=../ui");
    // Folders are named in the singular, so Tauri is told where the capabilities are.
    let attributes = tauri_build::Attributes::new().capabilities_path_pattern("./capability/**/*");
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("{error:#}");
    }
}
