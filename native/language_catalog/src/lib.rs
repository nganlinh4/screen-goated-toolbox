//! Compact, complete ISO 639 lookup data used by the signed host and workers.

use std::fmt::{Debug, Display, Formatter};
use std::io::Read as _;
use std::ops::Range;
use std::str::FromStr;
use std::sync::LazyLock;

const COMPRESSED_CATALOG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/languages.zlib"));

/// Attribution for the upstream ISO lookup data retained by this crate.
pub const THIRD_PARTY_NOTICES: &str = include_str!("../THIRD-PARTY-NOTICES.txt");

#[derive(Clone)]
struct Row {
    code_3: [u8; 3],
    code_1: Option<[u8; 2]>,
    name: Range<usize>,
}

struct Catalog {
    payload: Box<[u8]>,
    rows: Box<[Row]>,
    code_1_rows: Box<[([u8; 2], u16)]>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(decode_catalog);

fn decode_catalog() -> Catalog {
    let mut decoder = flate2::read::ZlibDecoder::new(COMPRESSED_CATALOG);
    let mut payload = Vec::new();
    decoder
        .read_to_end(&mut payload)
        .expect("build-generated language catalog must decompress");

    let mut cursor = 0_usize;
    let count = read_u16(&payload, &mut cursor) as usize;
    let mut rows = Vec::with_capacity(count);
    let mut code_1_rows = Vec::with_capacity(184);
    for index in 0..count {
        let code_3 = read_array::<3>(&payload, &mut cursor);
        let raw_code_1 = read_array::<2>(&payload, &mut cursor);
        let code_1 = (raw_code_1 != [0, 0]).then_some(raw_code_1);
        let name_len = read_u16(&payload, &mut cursor) as usize;
        let name_start = cursor;
        cursor = cursor
            .checked_add(name_len)
            .filter(|end| *end <= payload.len())
            .expect("build-generated language name must be bounded");
        std::str::from_utf8(&payload[name_start..cursor])
            .expect("build-generated language name must be UTF-8");
        if let Some(code) = code_1 {
            code_1_rows.push((code, index as u16));
        }
        rows.push(Row {
            code_3,
            code_1,
            name: name_start..cursor,
        });
    }
    assert_eq!(cursor, payload.len());
    assert!(rows.windows(2).all(|pair| pair[0].code_3 < pair[1].code_3));
    code_1_rows.sort_unstable_by_key(|(code, _)| *code);
    Catalog {
        payload: payload.into_boxed_slice(),
        rows: rows.into_boxed_slice(),
        code_1_rows: code_1_rows.into_boxed_slice(),
    }
}

fn read_u16(payload: &[u8], cursor: &mut usize) -> u16 {
    u16::from_le_bytes(read_array(payload, cursor))
}

fn read_array<const N: usize>(payload: &[u8], cursor: &mut usize) -> [u8; N] {
    let end = cursor
        .checked_add(N)
        .filter(|end| *end <= payload.len())
        .expect("build-generated language catalog must be bounded");
    let value = payload[*cursor..end]
        .try_into()
        .expect("language catalog field has a fixed length");
    *cursor = end;
    value
}

/// Stable handle to one complete ISO 639 entry.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language(u16);

impl Language {
    pub fn from_usize(index: usize) -> Option<Self> {
        (index < CATALOG.rows.len()).then_some(Self(index as u16))
    }

    pub fn from_639_3(code: &str) -> Option<Self> {
        let key: [u8; 3] = code.as_bytes().try_into().ok()?;
        CATALOG
            .rows
            .binary_search_by_key(&key, |row| row.code_3)
            .ok()
            .map(|index| Self(index as u16))
    }

    pub fn from_639_1(code: &str) -> Option<Self> {
        let key: [u8; 2] = code.as_bytes().try_into().ok()?;
        CATALOG
            .code_1_rows
            .binary_search_by_key(&key, |(code, _)| *code)
            .ok()
            .map(|index| Self(CATALOG.code_1_rows[index].1))
    }

    pub fn from_name(name: &str) -> Option<Self> {
        CATALOG
            .rows
            .iter()
            .position(|row| catalog_text(&row.name) == name)
            .map(|index| Self(index as u16))
    }

    pub fn from_locale(locale: &str) -> Option<Self> {
        locale.split('_').next().and_then(Self::from_639_1)
    }

    pub fn to_639_3(&self) -> &'static str {
        let row: &'static Row = &CATALOG.rows[self.0 as usize];
        std::str::from_utf8(&row.code_3).expect("generated ISO 639-3 code must be ASCII")
    }

    pub fn to_639_1(&self) -> Option<&'static str> {
        let row: &'static Row = &CATALOG.rows[self.0 as usize];
        row.code_1
            .as_ref()
            .map(|code| std::str::from_utf8(code).expect("generated ISO 639-1 code must be ASCII"))
    }

    pub fn to_name(&self) -> &'static str {
        let row: &'static Row = &CATALOG.rows[self.0 as usize];
        catalog_text(&row.name)
    }
}

fn catalog_text(range: &Range<usize>) -> &'static str {
    std::str::from_utf8(&CATALOG.payload[range.clone()])
        .expect("generated language name must be UTF-8")
}

impl Default for Language {
    fn default() -> Self {
        Self::from_639_3("und").expect("ISO catalog must contain Undetermined")
    }
}

impl Debug for Language {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.to_639_3())
    }
}

impl Display for Language {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.to_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLanguageError(String);

impl Display for ParseLanguageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "'{}' is not a valid ISO 639-1 or 639-3 code.",
            self.0
        )
    }
}

impl std::error::Error for ParseLanguageError {}

impl FromStr for Language {
    type Err = ParseLanguageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_639_3(value)
            .or_else(|| Self::from_639_1(value))
            .ok_or_else(|| ParseLanguageError(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_catalog_stays_compact() {
        assert!(COMPRESSED_CATALOG.len() < 64 * 1024);
        assert!(THIRD_PARTY_NOTICES.contains("Apache License, Version 2.0"));
    }
}
