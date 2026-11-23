use super::*;
use std::collections::{BTreeSet, HashMap};
use unicode_normalization::UnicodeNormalization;

const VOWELS: [&str; 12] = ["A", "Ă", "Â", "E", "Ê", "I", "O", "Ô", "Ơ", "U", "Ư", "Y"];
const TONES: [&str; 6] = [
    "", "\u{0300}", "\u{0301}", "\u{0309}", "\u{0303}", "\u{0323}",
];
const FACES: [Face; 2] = [Face::Small, Face::Large];

fn vietnamese_letters() -> Vec<char> {
    let mut letters: Vec<char> = VOWELS
        .iter()
        .flat_map(|vowel| TONES.iter().map(move |tone| format!("{vowel}{tone}")))
        .map(|composed| composed.nfc().next().unwrap())
        .collect();
    letters.push('Đ');
    letters
}

fn cell_set(glyph: &Glyph) -> BTreeSet<(i32, i32)> {
    glyph.cells.iter().copied().collect()
}

#[test]
fn every_vietnamese_letter_is_authored_and_distinct_in_both_faces() {
    for face in FACES {
        let mut seen: HashMap<BTreeSet<(i32, i32)>, char> = HashMap::new();
        for ch in vietnamese_letters() {
            let glyph = letter(ch, face).unwrap_or_else(|| panic!("{face:?} {ch}: no glyph"));
            if let Some(other) = seen.insert(cell_set(&glyph), ch) {
                panic!("{face:?}: {ch} draws the same cells as {other}");
            }
            // Lowercase input draws the uppercase letter.
            let lower = ch.to_lowercase().next().unwrap();
            assert_eq!(cell_set(&letter(lower, face).unwrap()), cell_set(&glyph));
        }
    }
}

#[test]
fn marks_never_touch_or_overwrite_the_letter() {
    for face in FACES {
        let art = face.art();
        for ch in vietnamese_letters().into_iter().filter(|&ch| ch != 'Đ') {
            let base = ch.to_string().nfd().next().unwrap();
            let letter_cells = cell_set(&face.ascii(base));
            for (x, y) in letter(ch, face).unwrap().cells {
                let horn = x == art.width && (-1..=0).contains(&y);
                if horn || letter_cells.contains(&(x, y)) {
                    continue;
                }
                // Marks keep an empty row between themselves and the letter.
                assert!(y <= -2 || y > art.rows, "{face:?} {ch}: mark cell {x},{y}");
            }
        }
    }
}

#[test]
fn every_splash_line_in_english_and_vietnamese_uses_the_authored_font() {
    for code in ["en", "vi"] {
        let shell = crate::gui::locale::LocaleText::get(code).shell;
        for line in [
            shell.splash_click_to_continue,
            shell.splash_loading_translating,
            shell.splash_loading_ocr,
            shell.splash_loading_transcribing,
            shell.splash_ready,
        ] {
            assert!(
                layout(&line.to_uppercase(), true).is_some(),
                "{code}: {line}"
            );
        }
    }
}

#[test]
fn ascii_small_lines_keep_the_compact_face() {
    let line = layout("A!", true).unwrap();
    assert_eq!((line.width, line.height), (7, 5));
    let rows: Vec<String> = (0..5)
        .map(|y| {
            (0..7)
                .map(|x| if line.ink(x, y) { '#' } else { '.' })
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        [".#...#.", "#.#..#.", "###..#.", "#.#....", "#.#..#."]
    );
}

#[test]
fn vietnamese_small_lines_keep_the_compact_face() {
    let line = layout("ĐỂ", true).unwrap();
    // Đ is 4 wide, then a gap, then Ể: its hook beside the circumflex
    // overhangs two columns past the 3-wide E.
    assert_eq!(line.width, 10);
    // The side hook rises five rows above the 5-row letters.
    assert_eq!(line.height, 10);
    // The overhang never widens the advance: "ẾP" spaces like "EP".
    assert_eq!(
        layout("ẾP", true).unwrap().width,
        layout("EP", true).unwrap().width
    );
}

#[test]
fn unauthored_scripts_are_left_to_the_rasterizer() {
    assert!(layout("번역 중...", true).is_none());
    assert!(layout("ß", false).is_none());
}
