//! JSON escaping for inline `<script>` elements, shared by the host and the
//! recorder worker (which path-includes this file).

/// Serialized JSON made safe to embed in an inline `<script>` element.
///
/// `<`, `>` and `&` only occur inside JSON strings, so their `\u` escapes keep
/// the value identical while no data can end the element (`</script`) or open
/// an HTML comment (`<!--`) that keeps the parser from ever closing it.
pub fn inline_script_json(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::inline_script_json;

    #[test]
    fn inline_script_json_cannot_close_the_script_and_keeps_its_value() {
        let json = serde_json::to_string("</script><!-- a & b").unwrap();
        let escaped = inline_script_json(&json);
        assert!(!escaped.contains('<') && !escaped.contains('>') && !escaped.contains('&'));
        let value: String = serde_json::from_str(&escaped).unwrap();
        assert_eq!(value, "</script><!-- a & b");
    }
}
