//! Rejects a translation that invents writing systems.
//!
//! Models fail in two different ways and only one of them is visible downstream.
//! A dead endpoint errors and the retry chain advances. A model that answers
//! *confidently wrong* returns HTTP 200 with fluent-looking text, and nothing
//! notices. Observed on real user input, one reply mixed Portuguese, Italian,
//! Spanish and Tamil into a Vietnamese translation; another fused a lone Hangul
//! character onto a Vietnamese word, giving `polyp đơn독`.
//!
//! The check is deliberately structural rather than a list of known failures.
//! There will be many more models and far more real inputs than anyone can write
//! cases for, so this derives its expectation from the request itself: a
//! translation may carry a term through untranslated, but it may not introduce
//! script that was never in the source.
//!
//! Concretely, the reply's dominant writing system is whatever it is mostly
//! written in — the target language, without needing to be told what that is.
//! Every run in some *other* system must appear verbatim in the source, which is
//! what a preserved proper noun looks like. A fragment of one does not match, so
//! `검사 운영` carried through passes while a stray `독` does not.
//!
//! It only rejects. It cannot repair a translation, so a failure returns an error
//! and the chain moves to the next model, which is also the honest outcome when a
//! model cannot follow the request: it stops being used.

/// Writing systems, grouped as languages actually use them.
///
/// Japanese mixes Han and Kana in ordinary prose and Korean admits Han, so those
/// are one family: a Han run inside Kana text is not evidence of drift. Families
/// that do not co-occur that way stay separate, which is what makes a Hangul
/// fragment inside Vietnamese detectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    /// Latin, including every Vietnamese diacritic.
    Latin,
    /// Han, Kana and Hangul, which share text in Japanese and Korean.
    CjkFamily,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Tamil,
    Other,
}

/// The script a character belongs to, or `None` when it carries no evidence.
///
/// Ranges are consulted before any character-class test, because combining marks
/// are not alphabetic yet belong to the run they modify: the Tamil virama would
/// otherwise cut a word in half and let a fragment match nothing.
fn script_of(c: char) -> Option<Script> {
    let code = c as u32;
    let script = match code {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Script::Latin,
        0x0370..=0x03FF => Script::Greek,
        0x0400..=0x04FF => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x0900..=0x097F => Script::Devanagari,
        0x0B80..=0x0BFF => Script::Tamil,
        0x0E00..=0x0E7F => Script::Thai,
        0x1100..=0x11FF | 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF => {
            Script::CjkFamily
        }
        _ => {
            // Everything shared between languages -- spaces, digits, punctuation --
            // says nothing about which language this is.
            if c.is_whitespace() || !c.is_alphabetic() {
                return None;
            }
            Script::Other
        }
    };
    Some(script)
}

/// The system the text is mostly written in.
fn dominant_script(text: &str) -> Option<Script> {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for script in text.chars().filter_map(script_of) {
        match counts.iter_mut().find(|(known, _)| *known == script) {
            Some((_, count)) => *count += 1,
            None => counts.push((script, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script)
}

/// A run of the reply written in some other system than the dominant one, which
/// the source never contained.
///
/// `None` means the reply introduced nothing, which is the ordinary case.
pub(super) fn introduced_script<'a>(source: &str, reply: &'a str) -> Option<&'a str> {
    let dominant = dominant_script(reply)?;

    let mut index = 0usize;
    while index < reply.len() {
        let rest = &reply[index..];
        let Some(c) = rest.chars().next() else { break };
        let width = c.len_utf8();
        let foreign = script_of(c).is_some_and(|script| script != dominant);
        if !foreign {
            index += width;
            continue;
        }
        // Take the whole run, so a preserved term is compared as a unit rather
        // than character by character.
        let run_len = rest
            .char_indices()
            .take_while(|(_, c)| script_of(*c).is_some_and(|script| script != dominant))
            .map(|(offset, c)| offset + c.len_utf8())
            .last()
            .unwrap_or(width);
        let run = &rest[..run_len];
        if !source.contains(run) {
            return Some(run);
        }
        index += run_len;
    }
    None
}

#[cfg(test)]
mod tests;
