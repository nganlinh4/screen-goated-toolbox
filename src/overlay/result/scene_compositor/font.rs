use std::sync::LazyLock;

use base64::Engine as _;

static FONT_BYTES: &[u8] = crate::assets::GOOGLE_SANS_FLEX_WEB;
static FONT_DATA_URL: LazyLock<String> = LazyLock::new(|| data_url(FONT_BYTES));

pub(super) fn bytes() -> &'static [u8] {
    FONT_BYTES
}

pub(crate) fn face_css(source: &str) -> String {
    format!(
        "@font-face{{font-family:'Google Sans Flex';font-style:normal;\
         font-weight:100 1000;font-stretch:25% 151%;font-display:block;\
         src:url('{source}') format('woff2')}}"
    )
}

pub(crate) fn isolated_face_css() -> String {
    face_css(FONT_DATA_URL.as_str())
}

fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:font/woff2;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_cards_embed_one_variable_face_for_the_full_axis_range() {
        let css = isolated_face_css();
        assert_eq!(css.matches("data:font/woff2;base64,").count(), 1);
        assert!(css.contains("font-weight:100 1000"));
        assert!(css.contains("font-stretch:25% 151%"));
        assert_eq!(bytes(), FONT_BYTES);
        assert_eq!(bytes(), crate::assets::GOOGLE_SANS_FLEX_WEB);
    }
}
