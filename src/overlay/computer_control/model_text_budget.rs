//! Bounded model-facing text; original observations retain their full identity.

pub(super) const MAX_FIELD_BYTES: usize = 256;
pub(super) const MAX_WORLD_BYTES: usize = 12 * 1024;
pub(super) const MAX_READOUT_BYTES: usize = 3200;
const FIELD_SUFFIX: &str = "…";
pub(super) const OMITTED_LINE: &str = "[additional items omitted]\n";

/// Bytes occupied when embedded in a JSON string, excluding surrounding quotes.
pub(super) fn wire_len(value: &str) -> usize {
    value.chars().fold(0usize, |length, character| {
        let encoded = match character {
            '"' | '\\' | '\u{0008}' | '\t' | '\n' | '\u{000c}' | '\r' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            _ => character.len_utf8(),
        };
        length.saturating_add(encoded)
    })
}

/// Clip at a UTF-8 boundary and mark a field whose content was shortened.
pub(super) fn field(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    if limit < FIELD_SUFFIX.len() {
        return String::new();
    }
    let end = value.floor_char_boundary(limit - FIELD_SUFFIX.len());
    format!("{}{FIELD_SUFFIX}", &value[..end])
}

/// Append whole, already bounded lines while reserving an omission marker.
pub(super) struct Lines {
    text: String,
    limit: usize,
    omitted: bool,
    encoded_len: usize,
}

impl Lines {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            limit,
            omitted: false,
            encoded_len: 0,
        }
    }

    pub(super) fn push(&mut self, line: &str) -> bool {
        if self.omitted {
            return false;
        }
        let line_len = wire_len(line);
        if self
            .encoded_len
            .saturating_add(line_len)
            .saturating_add(wire_len(OMITTED_LINE))
            > self.limit
        {
            self.omitted = true;
            return false;
        }
        self.text.push_str(line);
        self.encoded_len += line_len;
        true
    }

    pub(super) fn finish(mut self) -> String {
        if self.omitted && self.encoded_len + wire_len(OMITTED_LINE) <= self.limit {
            self.text.push_str(OMITTED_LINE);
        }
        self.text
    }
}

#[cfg(test)]
mod tests {
    use super::wire_len;

    #[test]
    fn json_byte_count_matches_serializer_for_unicode_and_escapes() {
        for sample in [
            "ascii",
            "界𠀀",
            "\"\\\n\t\r\u{0000}\u{0001}\u{0008}\u{000c}",
        ] {
            assert_eq!(
                wire_len(sample),
                serde_json::to_string(sample).unwrap().len() - 2
            );
        }
    }
}
