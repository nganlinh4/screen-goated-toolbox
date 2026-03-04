use super::config::{BakedCursorFrame, ParsedBakedCursorFrame};

pub fn cursor_type_to_id(c_type: &str) -> f32 {
    match c_type {
        // ScreenStudio set
        "default-screenstudio" | "default" => 0.0,
        "text-screenstudio" | "text" => 1.0,
        "pointer-screenstudio" | "pointer" => 2.0,
        "openhand-screenstudio" => 3.0,
        "closehand-screenstudio" => 4.0,
        "wait-screenstudio" | "wait" => 5.0,
        "appstarting-screenstudio" | "appstarting" => 6.0,
        "crosshair-screenstudio" | "crosshair" | "cross" => 7.0,
        "resize-ns-screenstudio" | "resize_ns" | "sizens" => 8.0,
        "resize-we-screenstudio" | "resize_we" | "sizewe" => 9.0,
        "resize-nwse-screenstudio" | "resize_nwse" | "sizenwse" => 10.0,
        "resize-nesw-screenstudio" | "resize_nesw" | "sizenesw" => 11.0,

        // macos26 expanded
        "default-macos26" => 12.0,
        "text-macos26" => 13.0,
        "pointer-macos26" => 14.0,
        "openhand-macos26" | "openhand" | "move" | "sizeall" => 15.0,
        "closehand-macos26" | "grabbing" => 16.0,
        "wait-macos26" => 17.0,
        "appstarting-macos26" => 18.0,
        "crosshair-macos26" => 19.0,
        "resize-ns-macos26" => 20.0,
        "resize-we-macos26" => 21.0,
        "resize-nwse-macos26" => 22.0,
        "resize-nesw-macos26" => 23.0,
        "default-sgtcute" => 24.0,
        "text-sgtcute" => 25.0,
        "pointer-sgtcute" => 26.0,
        "openhand-sgtcute" => 27.0,
        "closehand-sgtcute" => 28.0,
        "wait-sgtcute" => 29.0,
        "appstarting-sgtcute" => 30.0,
        "crosshair-sgtcute" => 31.0,
        "resize-ns-sgtcute" => 32.0,
        "resize-we-sgtcute" => 33.0,
        "resize-nwse-sgtcute" => 34.0,
        "resize-nesw-sgtcute" => 35.0,
        "default-sgtcool" => 36.0,
        "text-sgtcool" => 37.0,
        "pointer-sgtcool" => 38.0,
        "openhand-sgtcool" => 39.0,
        "closehand-sgtcool" => 40.0,
        "wait-sgtcool" => 41.0,
        "appstarting-sgtcool" => 42.0,
        "crosshair-sgtcool" => 43.0,
        "resize-ns-sgtcool" => 44.0,
        "resize-we-sgtcool" => 45.0,
        "resize-nwse-sgtcool" => 46.0,
        "resize-nesw-sgtcool" => 47.0,
        "default-sgtai" => 48.0,
        "text-sgtai" => 49.0,
        "pointer-sgtai" => 50.0,
        "openhand-sgtai" => 51.0,
        "closehand-sgtai" => 52.0,
        "wait-sgtai" => 53.0,
        "appstarting-sgtai" => 54.0,
        "crosshair-sgtai" => 55.0,
        "resize-ns-sgtai" => 56.0,
        "resize-we-sgtai" => 57.0,
        "resize-nwse-sgtai" => 58.0,
        "resize-nesw-sgtai" => 59.0,
        "default-sgtpixel" => 60.0,
        "text-sgtpixel" => 61.0,
        "pointer-sgtpixel" => 62.0,
        "openhand-sgtpixel" => 63.0,
        "closehand-sgtpixel" => 64.0,
        "wait-sgtpixel" => 65.0,
        "appstarting-sgtpixel" => 66.0,
        "crosshair-sgtpixel" => 67.0,
        "resize-ns-sgtpixel" => 68.0,
        "resize-we-sgtpixel" => 69.0,
        "resize-nwse-sgtpixel" => 70.0,
        "resize-nesw-sgtpixel" => 71.0,
        "default-jepriwin11" => 72.0,
        "text-jepriwin11" => 73.0,
        "pointer-jepriwin11" => 74.0,
        "openhand-jepriwin11" => 75.0,
        "closehand-jepriwin11" => 76.0,
        "wait-jepriwin11" => 77.0,
        "appstarting-jepriwin11" => 78.0,
        "crosshair-jepriwin11" => 79.0,
        "resize-ns-jepriwin11" => 80.0,
        "resize-we-jepriwin11" => 81.0,
        "resize-nwse-jepriwin11" => 82.0,
        "resize-nesw-jepriwin11" => 83.0,
        "default-sgtwatermelon" => 84.0,
        "text-sgtwatermelon" => 85.0,
        "pointer-sgtwatermelon" => 86.0,
        "openhand-sgtwatermelon" => 87.0,
        "closehand-sgtwatermelon" => 88.0,
        "wait-sgtwatermelon" => 89.0,
        "appstarting-sgtwatermelon" => 90.0,
        "crosshair-sgtwatermelon" => 91.0,
        "resize-ns-sgtwatermelon" => 92.0,
        "resize-we-sgtwatermelon" => 93.0,
        "resize-nwse-sgtwatermelon" => 94.0,
        "resize-nesw-sgtwatermelon" => 95.0,
        "default-sgtfastfood" => 96.0,
        "text-sgtfastfood" => 97.0,
        "pointer-sgtfastfood" => 98.0,
        "openhand-sgtfastfood" => 99.0,
        "closehand-sgtfastfood" => 100.0,
        "wait-sgtfastfood" => 101.0,
        "appstarting-sgtfastfood" => 102.0,
        "crosshair-sgtfastfood" => 103.0,
        "resize-ns-sgtfastfood" => 104.0,
        "resize-we-sgtfastfood" => 105.0,
        "resize-nwse-sgtfastfood" => 106.0,
        "resize-nesw-sgtfastfood" => 107.0,
        "default-sgtveggie" => 108.0,
        "text-sgtveggie" => 109.0,
        "pointer-sgtveggie" => 110.0,
        "openhand-sgtveggie" => 111.0,
        "closehand-sgtveggie" => 112.0,
        "wait-sgtveggie" => 113.0,
        "appstarting-sgtveggie" => 114.0,
        "crosshair-sgtveggie" => 115.0,
        "resize-ns-sgtveggie" => 116.0,
        "resize-we-sgtveggie" => 117.0,
        "resize-nwse-sgtveggie" => 118.0,
        "resize-nesw-sgtveggie" => 119.0,
        "default-sgtvietnam" => 120.0,
        "text-sgtvietnam" => 121.0,
        "pointer-sgtvietnam" => 122.0,
        "openhand-sgtvietnam" => 123.0,
        "closehand-sgtvietnam" => 124.0,
        "wait-sgtvietnam" => 125.0,
        "appstarting-sgtvietnam" => 126.0,
        "crosshair-sgtvietnam" => 127.0,
        "resize-ns-sgtvietnam" => 128.0,
        "resize-we-sgtvietnam" => 129.0,
        "resize-nwse-sgtvietnam" => 130.0,
        "resize-nesw-sgtvietnam" => 131.0,
        "other" => 12.0,
        _ => 0.0,
    }
}

pub fn collect_used_cursor_slots(baked_cursor: &[BakedCursorFrame]) -> Vec<u32> {
    let mut seen = [false; 132];
    let mut slots = Vec::new();
    for frame in baked_cursor {
        let slot = cursor_type_to_id(&frame.cursor_type) as u32;
        let idx = slot as usize;
        if idx < seen.len() && !seen[idx] {
            seen[idx] = true;
            slots.push(slot);
        }
    }
    if slots.is_empty() {
        slots.push(0);
    }
    slots
}

pub fn parse_baked_cursor_frames(baked_cursor: &[BakedCursorFrame]) -> Vec<ParsedBakedCursorFrame> {
    baked_cursor
        .iter()
        .map(|frame| ParsedBakedCursorFrame {
            time: {
                let _ = frame.is_clicked;
                frame.time
            },
            x: frame.x,
            y: frame.y,
            scale: frame.scale,
            type_id: cursor_type_to_id(frame.cursor_type.as_str()),
            opacity: frame.opacity,
            rotation: frame.rotation,
        })
        .collect()
}
