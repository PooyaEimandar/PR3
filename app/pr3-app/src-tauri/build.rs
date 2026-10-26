fn main() {
    // Folders are named in the singular, so Tauri is told where the capabilities are.
    let attributes = tauri_build::Attributes::new().capabilities_path_pattern("./capability/**/*");
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("{error:#}");
    }
}
