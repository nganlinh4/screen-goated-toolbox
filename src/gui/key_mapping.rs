use eframe::egui;

// Expanded Mapping Function: egui Key -> Windows Virtual Key (VK)
// This covers Function keys, arrows, delete/insert, home/end, and standard alphanumerics
pub fn egui_key_to_vk(key: &egui::Key) -> Option<u32> {
    match key {
        // Numbers
        egui::Key::Num0 => Some(0x30), egui::Key::Num1 => Some(0x31), egui::Key::Num2 => Some(0x32),
        egui::Key::Num3 => Some(0x33), egui::Key::Num4 => Some(0x34), egui::Key::Num5 => Some(0x35),
        egui::Key::Num6 => Some(0x36), egui::Key::Num7 => Some(0x37), egui::Key::Num8 => Some(0x38),
        egui::Key::Num9 => Some(0x39),
        // Letters
        egui::Key::A => Some(0x41), egui::Key::B => Some(0x42), egui::Key::C => Some(0x43),
        egui::Key::D => Some(0x44), egui::Key::E => Some(0x45), egui::Key::F => Some(0x46),
        egui::Key::G => Some(0x47), egui::Key::H => Some(0x48), egui::Key::I => Some(0x49),
        egui::Key::J => Some(0x4A), egui::Key::K => Some(0x4B), egui::Key::L => Some(0x4C),
        egui::Key::M => Some(0x4D), egui::Key::N => Some(0x4E), egui::Key::O => Some(0x4F),
        egui::Key::P => Some(0x50), egui::Key::Q => Some(0x51), egui::Key::R => Some(0x52),
        egui::Key::S => Some(0x53), egui::Key::T => Some(0x54), egui::Key::U => Some(0x55),
        egui::Key::V => Some(0x56), egui::Key::W => Some(0x57), egui::Key::X => Some(0x58),
        egui::Key::Y => Some(0x59), egui::Key::Z => Some(0x5A),
        // Function Keys
        egui::Key::F1 => Some(0x70), egui::Key::F2 => Some(0x71), egui::Key::F3 => Some(0x72),
        egui::Key::F4 => Some(0x73), egui::Key::F5 => Some(0x74), egui::Key::F6 => Some(0x75),
        egui::Key::F7 => Some(0x76), egui::Key::F8 => Some(0x77), egui::Key::F9 => Some(0x78),
        egui::Key::F10 => Some(0x79), egui::Key::F11 => Some(0x7A), egui::Key::F12 => Some(0x7B),
        egui::Key::F13 => Some(0x7C), egui::Key::F14 => Some(0x7D), egui::Key::F15 => Some(0x7E),
        egui::Key::F16 => Some(0x7F), egui::Key::F17 => Some(0x80), egui::Key::F18 => Some(0x81),
        egui::Key::F19 => Some(0x82), egui::Key::F20 => Some(0x83),
        // Navigation / Editing
        egui::Key::Escape => Some(0x1B),
        egui::Key::Insert => Some(0x2D),
        egui::Key::Delete => Some(0x2E),
        egui::Key::Home => Some(0x24),
        egui::Key::End => Some(0x23),
        egui::Key::PageUp => Some(0x21),
        egui::Key::PageDown => Some(0x22),
        egui::Key::ArrowLeft => Some(0x25),
        egui::Key::ArrowUp => Some(0x26),
        egui::Key::ArrowRight => Some(0x27),
        egui::Key::ArrowDown => Some(0x28),
        egui::Key::Backspace => Some(0x08),
        egui::Key::Enter => Some(0x0D),
        egui::Key::Space => Some(0x20),
        egui::Key::Tab => Some(0x09),
        // Symbols
        egui::Key::Backtick => Some(0xC0), // `
        egui::Key::Minus => Some(0xBD),    // -
        egui::Key::Plus => Some(0xBB),     // = (Plus is usually shift+=)
        egui::Key::OpenBracket => Some(0xDB), // [
        egui::Key::CloseBracket => Some(0xDD), // ]
        egui::Key::Backslash => Some(0xDC), // \
        egui::Key::Semicolon => Some(0xBA), // ;
        egui::Key::Comma => Some(0xBC),     // ,
        egui::Key::Period => Some(0xBE),    // .
        egui::Key::Slash => Some(0xBF),     // /
        _ => None,
    }
}
