pub fn generate_icon() -> tray_icon::Icon {
    // Embedded tray icon data
    let icon_bytes = include_bytes!("../assets/tray_icon.png");
    let img = image::load_from_memory(icon_bytes).expect("Failed to load embedded tray icon");
    let img_rgba = img.to_rgba8();
    let (width, height) = img_rgba.dimensions();
    let rgba = img_rgba.into_raw();
    tray_icon::Icon::from_rgba(rgba, width, height).unwrap()
}