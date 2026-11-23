//! The orb's glyphs: downloaded Material Symbols (`ui-shared/material-symbols`,
//! fetched with `scripts/download_material_symbols.sh`). `orb.html` names each
//! glyph with an `<!--sgt-material-symbol:NAME-->` slot, the marker the Android
//! host resolves from its own copy of the catalog; this fills the same slots.

macro_rules! glyph_table {
    ($($name:literal),+ $(,)?) => {
        const GLYPHS: &[(&str, &str)] = &[$((
            $name,
            include_str!(concat!("../../../../ui-shared/material-symbols/", $name, ".svg")),
        )),+];
    };
}

glyph_table![
    "arrow_selector_tool",
    "backlight_high",
    "bug_report",
    "check_small",
    "cognition",
    "comment",
    "emoji_objects",
    "eyeglasses_2",
    "eyeglasses_3",
    "frame_bug",
    "gesture",
    "globe_asia",
    "graph_1",
    "graph_2",
    "graph_3",
    "graph_4",
    "graph_5",
    "graph_6",
    "graph_7",
    "graphic_eq",
    "hourglass_bottom",
    "hourglass_top",
    "hov",
    "keyboard",
    "keyboard_double_arrow_down",
    "keyboard_double_arrow_left",
    "keyboard_double_arrow_right",
    "keyboard_double_arrow_up",
    "lightbulb",
    "location_searching",
    "my_location",
    "person_celebrate",
    "person_play",
    "rtt",
    "sentiment_calm",
    "sentiment_content",
    "sentiment_dissatisfied",
    "sentiment_excited",
    "sentiment_extremely_dissatisfied",
    "sentiment_frustrated",
    "sentiment_neutral",
    "sentiment_sad",
    "sentiment_satisfied",
    "sentiment_stressed",
    "sentiment_very_dissatisfied",
    "sentiment_very_satisfied",
    "sentiment_worried",
    "table_eye",
    "terminal",
    "visibility",
];

const SLOT_OPEN: &str = "<!--sgt-material-symbol:";
const SLOT_CLOSE: &str = "-->";

fn glyph(name: &str) -> Option<&'static str> {
    GLYPHS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, svg)| svg.trim())
}

/// Replace every glyph slot with its symbol. A slot naming a symbol outside
/// the table is left as the inert comment, so the orb shows no glyph for it.
pub(super) fn fill_glyph_slots(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + GLYPHS.len() * 1024);
    let mut rest = html;
    while let Some(start) = rest.find(SLOT_OPEN) {
        let after_open = &rest[start + SLOT_OPEN.len()..];
        let Some(name_len) = after_open.find(SLOT_CLOSE) else {
            break;
        };
        let slot_len = SLOT_OPEN.len() + name_len + SLOT_CLOSE.len();
        out.push_str(&rest[..start]);
        match glyph(&after_open[..name_len]) {
            Some(svg) => out.push_str(svg),
            None => out.push_str(&rest[start..start + slot_len]),
        }
        rest = &rest[start + slot_len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{GLYPHS, SLOT_OPEN, fill_glyph_slots};

    const ORB: &str = include_str!("orb.html");

    #[test]
    fn the_orb_page_carries_no_icon_geometry_of_its_own() {
        for pasted in ["<path", "<svg", " d=\"M", "viewBox"] {
            assert!(!ORB.contains(pasted), "orb.html contains {pasted}");
        }
    }

    #[test]
    fn every_orb_glyph_slot_is_filled_with_a_downloaded_symbol() {
        let html = fill_glyph_slots(ORB);
        assert!(!html.contains(SLOT_OPEN), "an orb glyph has no symbol");
        // Each slot sits in a single-quoted JS string, so a symbol must not
        // break out of it.
        for (name, svg) in GLYPHS {
            let svg = svg.trim();
            assert!(
                svg.starts_with("<svg xmlns=") && svg.ends_with("</svg>"),
                "{name}"
            );
            assert!(svg.contains("fill=\"currentColor\""), "{name}");
            assert!(!svg.contains(['\'', '\\', '\n']), "{name}");
            assert!(
                html.contains(&format!("{name}:'{svg}'")),
                "{name} is unused"
            );
        }
    }

    #[test]
    fn glyphs_the_host_picks_at_run_time_exist() {
        let html = fill_glyph_slots(ORB);
        // overlay.rs set_orb_tool picks the scroll arrow by direction.
        for name in [
            "keyboard_double_arrow_up",
            "keyboard_double_arrow_down",
            "keyboard_double_arrow_left",
            "keyboard_double_arrow_right",
        ] {
            assert!(html.contains(&format!("{name}:'<svg")), "{name}");
        }
    }

    #[test]
    fn an_unknown_slot_stays_an_inert_comment() {
        let html = "a<!--sgt-material-symbol:not_a_symbol-->b<!--sgt-material-symbol:hov-->c";
        let filled = fill_glyph_slots(html);
        assert!(filled.starts_with("a<!--sgt-material-symbol:not_a_symbol-->b<svg"));
        assert!(filled.ends_with("</svg>c"));
    }
}
