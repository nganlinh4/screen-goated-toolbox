use super::config::{BakedCursorFrame, ParsedBakedCursorFrame};
use crate::overlay::screen_record::embedded_assets::{CURSOR_ATLAS_SLOT_COUNT, cursor_atlas_slot};

/// Atlas slot for a baked cursor type: `<type>-<pack>` names resolve through
/// the one pack/type table; bare legacy names keep their historical slots and
/// anything unknown renders the default cursor.
pub fn cursor_type_to_id(c_type: &str) -> f32 {
    let legacy = match c_type {
        "default" => Some("default-screenstudio"),
        "text" => Some("text-screenstudio"),
        "pointer" => Some("pointer-screenstudio"),
        "wait" => Some("wait-screenstudio"),
        "appstarting" => Some("appstarting-screenstudio"),
        "crosshair" | "cross" => Some("crosshair-screenstudio"),
        "resize_ns" | "sizens" => Some("resize-ns-screenstudio"),
        "resize_we" | "sizewe" => Some("resize-we-screenstudio"),
        "resize_nwse" | "sizenwse" => Some("resize-nwse-screenstudio"),
        "resize_nesw" | "sizenesw" => Some("resize-nesw-screenstudio"),
        "openhand" | "move" | "sizeall" => Some("openhand-macos26"),
        "grabbing" => Some("closehand-macos26"),
        "other" => Some("default-macos26"),
        _ => None,
    };
    cursor_atlas_slot(legacy.unwrap_or(c_type)).unwrap_or(0) as f32
}

pub fn collect_used_cursor_slots(baked_cursor: &[BakedCursorFrame]) -> Vec<u32> {
    let mut seen = [false; CURSOR_ATLAS_SLOT_COUNT as usize];
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
        .map(|frame| {
            // `is_clicked` is part of the baked-cursor wire/data model (set during
            // path generation and deserialized from the export config) but the parsed
            // export pipeline does not consume it. Reference it so the model field
            // stays warning-free without dropping it from the contract.
            let _ = frame.is_clicked;
            ParsedBakedCursorFrame {
                time: frame.time,
                x: frame.x,
                y: frame.y,
                scale: frame.scale,
                type_id: cursor_type_to_id(frame.cursor_type.as_str()),
                opacity: frame.opacity,
                rotation: frame.rotation,
            }
        })
        .collect()
}

#[cfg(test)]
mod slot_tests {
    use super::cursor_type_to_id;

    #[test]
    fn legacy_names_keep_their_slots_and_unknown_names_use_the_default() {
        for (name, slot) in [
            ("default", 0.0),
            ("cross", 7.0),
            ("sizenesw", 11.0),
            ("move", 15.0),
            ("grabbing", 16.0),
            ("other", 12.0),
            ("pointer-sgtvietnam", 122.0),
            ("not-a-cursor", 0.0),
        ] {
            assert_eq!(cursor_type_to_id(name), slot, "{name}");
        }
    }
}
