pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn hex_to_linear(r: u8, g: u8, b: u8) -> [f32; 4] {
    [
        srgb_to_linear(r as f32 / 255.0),
        srgb_to_linear(g as f32 / 255.0),
        srgb_to_linear(b as f32 / 255.0),
        1.0,
    ]
}

pub fn get_gradient_colors(bg_type: &str) -> ([f32; 4], [f32; 4]) {
    match bg_type {
        "gradient1" => (
            hex_to_linear(0x4F, 0x7F, 0xD9),
            hex_to_linear(0x8A, 0x72, 0xD8),
        ),
        "gradient2" => (
            hex_to_linear(0xFB, 0x71, 0x85),
            hex_to_linear(0xFD, 0xBA, 0x74),
        ),
        "gradient3" => (
            hex_to_linear(0x10, 0xB9, 0x81),
            hex_to_linear(0x2D, 0xD4, 0xBF),
        ),
        "gradient4" => (
            hex_to_linear(0x06, 0x1A, 0x40),
            hex_to_linear(0xF9, 0x73, 0x16),
        ),
        "gradient5" => (
            hex_to_linear(0x0D, 0x1B, 0x4C),
            hex_to_linear(0xEF, 0x47, 0x6F),
        ),
        "gradient6" => (
            hex_to_linear(0x00, 0xD4, 0xFF),
            hex_to_linear(0xFF, 0x3D, 0x81),
        ),
        "gradient7" => (
            hex_to_linear(0x3F, 0xA7, 0xD6),
            hex_to_linear(0xF2, 0x9E, 0x6D),
        ),
        "white" => (
            hex_to_linear(0xF5, 0xF5, 0xF5),
            hex_to_linear(0xFF, 0xFF, 0xFF),
        ),
        _ => (
            hex_to_linear(0x0A, 0x0A, 0x0A),
            hex_to_linear(0x00, 0x00, 0x00),
        ),
    }
}
