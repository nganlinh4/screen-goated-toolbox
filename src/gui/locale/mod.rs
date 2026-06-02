mod text;
mod tips;

pub use text::LocaleText;

mod en;
mod ko;
mod vi;

impl LocaleText {
    pub fn get(lang_code: &str) -> Self {
        match lang_code {
            "vi" => vi::get(),
            "ko" => ko::get(),
            _ => en::get(),
        }
    }
}
