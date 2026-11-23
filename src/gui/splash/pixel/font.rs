//! The splash's authored pixel font: a 5x7 face, a compact 3x5 face for the
//! small lines, and Vietnamese built from those letters plus authored marks.
//!
//! A line is laid out once into a [`TextBitmap`] of grid cells, and the painter
//! places that bitmap on the scene grid. Vietnamese letters decompose
//! canonically (NFD) into a Latin base plus combining marks; each mark is a small
//! authored sprite placed by fixed rules, so every accented letter stays on the
//! same cell grid as the base face.

use unicode_normalization::char::decompose_canonical;

/// A laid-out line: `width` x `height` cells, row-major, `true` = ink. Row 0 is
/// the top of the line box, including any marks above the letters.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextBitmap {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<bool>,
}

impl TextBitmap {
    pub(crate) fn ink(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.width + x]
    }

    /// Builds a bitmap from cells whose rows may be negative (marks above the
    /// letters). `rows` is the letter height, which every line box includes.
    fn from_cells(cells: &[(i32, i32)], width: i32, rows: i32) -> Self {
        let top = cells.iter().map(|&(_, y)| y).min().unwrap_or(0).min(0);
        let bottom = cells
            .iter()
            .map(|&(_, y)| y + 1)
            .max()
            .unwrap_or(0)
            .max(rows);
        let (width, height) = (width.max(0) as usize, (bottom - top) as usize);
        let mut out = vec![false; width * height];
        for &(x, y) in cells {
            if (0..width as i32).contains(&x) {
                out[(y - top) as usize * width + x as usize] = true;
            }
        }
        Self {
            width,
            height,
            cells: out,
        }
    }
}

/// Lays out `s` in the authored font, or `None` when a character has no
/// authored glyph. `tiny` picks the compact 3x5 face, otherwise the 5x7 face;
/// both carry the Vietnamese marks.
pub(crate) fn layout(s: &str, tiny: bool) -> Option<TextBitmap> {
    let face = if tiny { Face::Small } else { Face::Large };
    let glyphs: Vec<Glyph> = s
        .chars()
        .map(|ch| letter(ch, face))
        .collect::<Option<_>>()?;
    let mut cells = Vec::new();
    let mut x = 0;
    let mut width = 0;
    for glyph in glyphs {
        cells.extend(glyph.cells.iter().map(|&(gx, gy)| (x + gx, gy)));
        // A mark overhanging the last letter still widens the line.
        let ink_right = glyph.cells.iter().map(|&(gx, _)| gx + 1).max().unwrap_or(0);
        width = x + glyph.width.max(ink_right);
        x += glyph.width + 1;
    }
    Some(TextBitmap::from_cells(&cells, width, face.art().rows))
}

#[derive(Clone, Copy, Debug)]
enum Face {
    Small,
    Large,
}

impl Face {
    fn art(self) -> &'static FaceArt {
        match self {
            Face::Small => &SMALL,
            Face::Large => &LARGE,
        }
    }

    fn ascii(self, ch: char) -> Glyph {
        match self {
            Face::Small => ascii_glyph(3, &small_rows(ch)),
            Face::Large => ascii_glyph(5, &large_rows(ch)),
        }
    }
}

/// One letter's cells relative to its top-left; rows above 0 hold marks.
struct Glyph {
    width: i32,
    cells: Vec<(i32, i32)>,
}

impl Glyph {
    fn stamp(&mut self, pattern: &[&str], top: i32, left: i32) {
        for (dy, line) in pattern.iter().enumerate() {
            for (dx, byte) in line.bytes().enumerate() {
                if byte == b'#' {
                    self.cells.push((left + dx as i32, top + dy as i32));
                }
            }
        }
    }
}

fn ascii_glyph(width: i32, rows: &[u8]) -> Glyph {
    let mut cells = Vec::new();
    for (y, &row) in rows.iter().enumerate() {
        for x in 0..width {
            if (row >> (width - 1 - x)) & 1 == 1 {
                cells.push((x, y as i32));
            }
        }
    }
    Glyph { width, cells }
}

// --- VIETNAMESE MARKS ---------------------------------------------------------
//
// Marks sit above the letter with one empty row between. A tone over a
// circumflex sits beside it (cols 3-4, overhanging a 3-wide letter), as
// Vietnamese type sets it; over a
// breve it stacks, and a tilde stacks with an extra empty row so the two marks
// never merge. A horn hangs off the top-right of O and U.

type Art = &'static [&'static str];

/// Per-face mark sprites, drawn to the width of that face's letters.
struct FaceArt {
    width: i32,
    rows: i32,
    acute: Art,
    grave: Art,
    hook: Art,
    tilde: Art,
    circumflex: Art,
    /// The circumflex drawn when a tone sits beside it, in cols 0-2.
    circumflex_beside_tone: Art,
    breve: Art,
    d_stroke: Art,
}

const LARGE: FaceArt = FaceArt {
    width: 5,
    rows: 7,
    acute: &["...#.", "..#.."],
    grave: &[".#...", "..#.."],
    hook: &[".##..", "...#.", "..#.."],
    tilde: &[".##.#", "#.##."],
    circumflex: &["..#..", ".#.#."],
    circumflex_beside_tone: &[".#.", "#.#"],
    breve: &["#...#", ".###."],
    d_stroke: &[
        ".###..", ".#..#.", ".#...#", "###..#", ".#...#", ".#..#.", ".###..",
    ],
};

const SMALL: FaceArt = FaceArt {
    width: 3,
    rows: 5,
    acute: &["..#", ".#."],
    grave: &["#..", ".#."],
    hook: &["##.", "..#", ".#."],
    tilde: &[".##", "##."],
    circumflex: &[".#.", "#.#"],
    circumflex_beside_tone: &[".#.", "#.#"],
    breve: &["#.#", ".#."],
    d_stroke: &[".##.", ".#.#", "##.#", ".#.#", ".##."],
};

/// A tone beside a circumflex, in cols 3-4 of either face.
const SIDE_ACUTE: Art = &[".#", "#."];
const SIDE_GRAVE: Art = &["#.", ".#"];
const SIDE_HOOK: Art = &["##", ".#", "#."];

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Acute,
    Grave,
    Hook,
    Tilde,
}

impl Tone {
    fn pattern(self, art: &FaceArt) -> Art {
        match self {
            Tone::Acute => art.acute,
            Tone::Grave => art.grave,
            Tone::Hook => art.hook,
            Tone::Tilde => art.tilde,
        }
    }

    fn beside_circumflex(self) -> Option<Art> {
        match self {
            Tone::Acute => Some(SIDE_ACUTE),
            Tone::Grave => Some(SIDE_GRAVE),
            Tone::Hook => Some(SIDE_HOOK),
            Tone::Tilde => None,
        }
    }
}

#[derive(Default)]
struct Marks {
    circumflex: bool,
    breve: bool,
    horn: bool,
    dot_below: bool,
    tone: Option<Tone>,
}

/// A letter in `face`, with Vietnamese marks applied; `None` if not authored.
fn letter(ch: char, face: Face) -> Option<Glyph> {
    // Uppercase only when it maps to one letter: 'ß' must not draw as 'S'.
    let mut upper = ch.to_uppercase();
    let ch = match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => ch,
    };
    let art = face.art();
    if ch == 'Đ' {
        let mut glyph = Glyph {
            width: art.width + 1,
            cells: Vec::new(),
        };
        glyph.stamp(art.d_stroke, 0, 0);
        return Some(glyph);
    }
    if ascii_index(ch).is_some() {
        return Some(face.ascii(ch));
    }
    let mut base = None;
    let mut marks = Marks::default();
    let mut known = true;
    decompose_canonical(ch, |part| match part {
        '\u{0300}' => marks.tone = Some(Tone::Grave),
        '\u{0301}' => marks.tone = Some(Tone::Acute),
        '\u{0303}' => marks.tone = Some(Tone::Tilde),
        '\u{0309}' => marks.tone = Some(Tone::Hook),
        '\u{0302}' => marks.circumflex = true,
        '\u{0306}' => marks.breve = true,
        '\u{031B}' => marks.horn = true,
        '\u{0323}' => marks.dot_below = true,
        other if base.is_none() && other.is_ascii_alphabetic() => base = Some(other),
        _ => known = false,
    });
    if !known {
        return None;
    }
    let mut glyph = face.ascii(base?);
    apply_marks(&mut glyph, &marks, art);
    Some(glyph)
}

/// Places the marks. A mark whose bottom row is `b` starts at
/// `b + 1 - height`; row -1 stays empty above the letter.
fn apply_marks(glyph: &mut Glyph, marks: &Marks, art: &FaceArt) {
    let height = |pattern: Art| pattern.len() as i32;
    // A stacked tilde is as wide as the mark under it, so it keeps one more
    // empty row between them.
    let stacked_top = |tone: Tone| {
        let gap = i32::from(tone == Tone::Tilde);
        -3 - gap - height(tone.pattern(art))
    };
    if marks.circumflex {
        match marks.tone {
            Some(tone) => match tone.beside_circumflex() {
                Some(side) => {
                    glyph.stamp(art.circumflex_beside_tone, -3, 0);
                    // In the small face this overhangs the gap column, above
                    // the next letter's body, so the letter keeps its advance.
                    glyph.stamp(side, -2 - height(side), 3);
                }
                None => {
                    glyph.stamp(art.circumflex, -3, 0);
                    glyph.stamp(tone.pattern(art), stacked_top(tone), 0);
                }
            },
            None => glyph.stamp(art.circumflex, -3, 0),
        }
    } else if marks.breve {
        glyph.stamp(art.breve, -3, 0);
        if let Some(tone) = marks.tone {
            glyph.stamp(tone.pattern(art), stacked_top(tone), 0);
        }
    } else if let Some(tone) = marks.tone {
        let pattern = tone.pattern(art);
        glyph.stamp(pattern, -1 - height(pattern), 0);
    }
    if marks.horn {
        glyph.stamp(&["#", "#"], -1, glyph.width);
        glyph.width += 1;
    }
    if marks.dot_below {
        glyph.stamp(&["#"], art.rows + 1, art.width / 2);
    }
}

// --- AUTHORED ASCII FACES -----------------------------------------------------

/// Index into the ASCII glyph tables, or `None` for an unauthored character.
fn ascii_index(ch: char) -> Option<usize> {
    let c = ch.to_ascii_uppercase();
    match c {
        'A'..='Z' => Some((c as u8 - b'A') as usize),
        '0'..='9' => Some(26 + (c as u8 - b'0') as usize),
        ' ' => Some(36),
        '.' => Some(37),
        '!' => Some(38),
        _ => None,
    }
}

#[rustfmt::skip]
fn large_rows(ch: char) -> [u8; 7] {
    const FONT: [[u8; 7]; 39] = [
        [14,17,17,31,17,17,17], // A
        [30,17,17,30,17,17,30], // B
        [14,17,16,16,16,17,14], // C
        [28,18,17,17,17,18,28], // D
        [31,16,16,30,16,16,31], // E
        [31,16,16,30,16,16,16], // F
        [14,17,16,23,17,17,15], // G
        [17,17,17,31,17,17,17], // H
        [14, 4, 4, 4, 4, 4,14], // I
        [ 7, 2, 2, 2, 2,18,12], // J
        [17,18,20,24,20,18,17], // K
        [16,16,16,16,16,16,31], // L
        [17,27,21,21,17,17,17], // M
        [17,17,25,21,19,17,17], // N
        [14,17,17,17,17,17,14], // O
        [30,17,17,30,16,16,16], // P
        [14,17,17,17,21,18,13], // Q
        [30,17,17,30,20,18,17], // R
        [15,16,16,14, 1, 1,30], // S
        [31, 4, 4, 4, 4, 4, 4], // T
        [17,17,17,17,17,17,14], // U
        [17,17,17,17,17,10, 4], // V
        [17,17,17,21,21,21,10], // W
        [17,17,10, 4,10,17,17], // X
        [17,17,10, 4, 4, 4, 4], // Y
        [31, 1, 2, 4, 8,16,31], // Z
        [14,17,19,21,25,17,14], // 0
        [ 4,12, 4, 4, 4, 4,14], // 1
        [14,17, 1, 6, 8,16,31], // 2
        [31, 2, 4, 2, 1,17,14], // 3
        [ 2, 6,10,18,31, 2, 2], // 4
        [31,16,30, 1, 1,17,14], // 5
        [ 6, 8,16,30,17,17,14], // 6
        [31, 1, 2, 4, 8, 8, 8], // 7
        [14,17,17,14,17,17,14], // 8
        [14,17,17,15, 1, 2,12], // 9
        [ 0, 0, 0, 0, 0, 0, 0], // space
        [ 0, 0, 0, 0, 0,12,12], // .
        [ 4, 4, 4, 4, 4, 0, 4], // !
    ];
    FONT[ascii_index(ch).unwrap_or(36)]
}

/// Compact 3x5 face (bit 2 = leftmost): the same cell size as the 5x7 face,
/// fewer cells per glyph, so the small lines are physically smaller.
#[rustfmt::skip]
fn small_rows(ch: char) -> [u8; 5] {
    const FONT: [[u8; 5]; 39] = [
        [2,5,7,5,5], // A
        [6,5,6,5,6], // B
        [3,4,4,4,3], // C
        [6,5,5,5,6], // D
        [7,4,6,4,7], // E
        [7,4,6,4,4], // F
        [3,4,5,5,3], // G
        [5,5,7,5,5], // H
        [7,2,2,2,7], // I
        [1,1,1,5,2], // J
        [5,6,4,6,5], // K
        [4,4,4,4,7], // L
        [5,7,7,5,5], // M
        [5,7,5,5,5], // N
        [2,5,5,5,2], // O
        [6,5,6,4,4], // P
        [2,5,5,3,1], // Q
        [6,5,6,5,5], // R
        [3,4,2,1,6], // S
        [7,2,2,2,2], // T
        [5,5,5,5,7], // U
        [5,5,5,5,2], // V
        [5,5,7,7,5], // W
        [5,5,2,5,5], // X
        [5,5,2,2,2], // Y
        [7,1,2,4,7], // Z
        [7,5,5,5,7], // 0
        [2,6,2,2,7], // 1
        [6,1,2,4,7], // 2
        [6,1,2,1,6], // 3
        [5,5,7,1,1], // 4
        [7,4,6,1,6], // 5
        [3,4,6,5,3], // 6
        [7,1,2,2,2], // 7
        [2,5,2,5,2], // 8
        [2,5,3,1,6], // 9
        [0,0,0,0,0], // space
        [0,0,0,0,2], // .
        [2,2,2,0,2], // !
    ];
    FONT[ascii_index(ch).unwrap_or(36)]
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod tests;
