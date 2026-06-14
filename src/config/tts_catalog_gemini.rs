pub const GEMINI_VOICES: &[(&str, &str)] = &[
    ("Achernar", "Female"),
    ("Achird", "Male"),
    ("Algenib", "Male"),
    ("Algieba", "Male"),
    ("Alnilam", "Male"),
    ("Aoede", "Female"),
    ("Autonoe", "Female"),
    ("Callirrhoe", "Female"),
    ("Charon", "Male"),
    ("Despina", "Female"),
    ("Enceladus", "Male"),
    ("Erinome", "Female"),
    ("Fenrir", "Male"),
    ("Gacrux", "Female"),
    ("Iapetus", "Male"),
    ("Kore", "Female"),
    ("Laomedeia", "Female"),
    ("Leda", "Female"),
    ("Orus", "Male"),
    ("Puck", "Male"),
    ("Pulcherrima", "Female"),
    ("Rasalgethi", "Male"),
    ("Sadachbia", "Male"),
    ("Sadaltager", "Male"),
    ("Schedar", "Male"),
    ("Sulafat", "Female"),
    ("Umbriel", "Male"),
    ("Vindemiatrix", "Female"),
    ("Zephyr", "Female"),
    ("Zubenelgenubi", "Male"),
];

pub const SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES: &[(&str, &str)] = &[
    ("afr", "Afrikaans"),
    ("ara", "Arabic"),
    ("aze", "Azerbaijani"),
    ("bel", "Belarusian"),
    ("ben", "Bengali"),
    ("bul", "Bulgarian"),
    ("cat", "Catalan"),
    ("ces", "Czech"),
    ("cmn", "Mandarin Chinese"),
    ("dan", "Danish"),
    ("deu", "German"),
    ("ell", "Greek"),
    ("eng", "English"),
    ("epo", "Esperanto"),
    ("est", "Estonian"),
    ("eus", "Basque"),
    ("fin", "Finnish"),
    ("fra", "French"),
    ("guj", "Gujarati"),
    ("heb", "Hebrew"),
    ("hin", "Hindi"),
    ("hrv", "Croatian"),
    ("hun", "Hungarian"),
    ("ind", "Indonesian"),
    ("ita", "Italian"),
    ("jpn", "Japanese"),
    ("kan", "Kannada"),
    ("kat", "Georgian"),
    ("kor", "Korean"),
    ("lat", "Latin"),
    ("lav", "Latvian"),
    ("lit", "Lithuanian"),
    ("mal", "Malayalam"),
    ("mar", "Marathi"),
    ("mkd", "Macedonian"),
    ("mya", "Burmese"),
    ("nep", "Nepali"),
    ("nld", "Dutch"),
    ("nno", "Norwegian Nynorsk"),
    ("nob", "Norwegian Bokmal"),
    ("ori", "Oriya"),
    ("pan", "Punjabi"),
    ("pes", "Persian"),
    ("pol", "Polish"),
    ("por", "Portuguese"),
    ("ron", "Romanian"),
    ("rus", "Russian"),
    ("sin", "Sinhala"),
    ("slk", "Slovak"),
    ("slv", "Slovenian"),
    ("som", "Somali"),
    ("spa", "Spanish"),
    ("sqi", "Albanian"),
    ("srp", "Serbian"),
    ("swe", "Swedish"),
    ("tam", "Tamil"),
    ("tel", "Telugu"),
    ("tgl", "Tagalog"),
    ("tha", "Thai"),
    ("tur", "Turkish"),
    ("ukr", "Ukrainian"),
    ("urd", "Urdu"),
    ("uzb", "Uzbek"),
    ("vie", "Vietnamese"),
    ("yid", "Yiddish"),
    ("zho", "Chinese"),
];

#[cfg(test)]
mod tests {
    use super::{GEMINI_VOICES, SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES};
    use serde::Deserialize;

    // Shared cross-platform parity fixture. Rust is canonical; the Android
    // (Kotlin) catalog asserts against the same file so the two cannot drift.
    // See .claude/parity/gemini-voice-catalog.md.
    const FIXTURE: &str =
        include_str!("../../parity-fixtures/gemini-voice-catalog/catalog.json");

    #[derive(Deserialize)]
    struct Catalog {
        voices: Vec<Voice>,
        #[serde(rename = "instructionLanguages")]
        instruction_languages: Vec<Language>,
    }

    #[derive(Deserialize)]
    struct Voice {
        name: String,
        gender: String,
    }

    #[derive(Deserialize)]
    struct Language {
        code: String,
        name: String,
    }

    #[test]
    fn gemini_voices_match_parity_fixture() {
        let catalog: Catalog = serde_json::from_str(FIXTURE).expect("fixture parses");
        let fixture: Vec<(&str, &str)> = catalog
            .voices
            .iter()
            .map(|v| (v.name.as_str(), v.gender.as_str()))
            .collect();
        assert_eq!(
            GEMINI_VOICES.to_vec(),
            fixture,
            "GEMINI_VOICES drifted from the shared parity fixture (same entries, same order)"
        );
    }

    #[test]
    fn gemini_instruction_languages_match_parity_fixture() {
        let catalog: Catalog = serde_json::from_str(FIXTURE).expect("fixture parses");
        let fixture: Vec<(&str, &str)> = catalog
            .instruction_languages
            .iter()
            .map(|l| (l.code.as_str(), l.name.as_str()))
            .collect();
        assert_eq!(
            SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES.to_vec(),
            fixture,
            "SUPPORTED_GEMINI_INSTRUCTION_LANGUAGES drifted from the shared parity fixture (same entries, same order)"
        );
    }
}
