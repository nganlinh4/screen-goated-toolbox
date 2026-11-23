//! The worker carries only the strings it shows, but each one must read exactly
//! like the host's. These tests compare every worker string with the host
//! locale source for the same section and field. The destructuring patterns
//! make a new worker field fail to compile until it is compared here.

use super::locale::{
    AuxiliaryLocaleText, BadgeLocaleText, GlobalSettingsLocaleText, LocaleText,
    PresetBasicsLocaleText, RealtimeLocaleText, ShellLocaleText, TtsPlaygroundLocaleText,
};

macro_rules! host_sections {
    ($language:literal) => {
        [
            (
                "auxiliary",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/auxiliary.rs"
                )),
            ),
            (
                "badge",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/badge.rs"
                )),
            ),
            (
                "global_settings",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/global_settings.rs"
                )),
            ),
            (
                "preset_basics",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/preset_basics.rs"
                )),
            ),
            (
                "realtime",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/realtime.rs"
                )),
            ),
            (
                "shell",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/shell.rs"
                )),
            ),
            (
                "tts_playground",
                include_str!(concat!(
                    "../../../../src/gui/locale/",
                    $language,
                    "/tts_playground.rs"
                )),
            ),
        ]
    };
}

/// The string literal assigned to `field` in a host locale constructor.
fn host_value(source: &str, field: &str) -> Option<String> {
    let needle = format!("{field}:");
    let start = source.lines().find_map(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&needle).then(|| {
            let offset = line.as_ptr() as usize - source.as_ptr() as usize;
            offset + (line.len() - trimmed.len()) + needle.len()
        })
    })?;
    let literal = source[start..].trim_start().strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = literal.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    None
}

/// Every worker string, with the host section and field that owns it.
fn worker_strings(text: LocaleText) -> Vec<(&'static str, &'static str, &'static str)> {
    let LocaleText {
        auxiliary,
        badge,
        global_settings,
        preset_basics,
        realtime,
        shell,
        tts_playground,
    } = text;
    let AuxiliaryLocaleText {
        win_select_title,
        win_select_subtitle,
        win_select_display_only_badge,
        win_select_display_only_title,
        win_select_display_only_message,
        win_select_display_only_action,
        win_select_count,
    } = auxiliary;
    let BadgeLocaleText {
        export_failed,
        subtitles_saved,
        feature_needs_webview2_fmt,
        feature_window_selector,
        feature_screen_record,
        install_webview2_hint,
    } = badge;
    let GlobalSettingsLocaleText { model_thinking } = global_settings;
    let PresetBasicsLocaleText { cancel_label } = preset_basics;
    let RealtimeLocaleText {
        app_select_title,
        app_select_count,
    } = realtime;
    let ShellLocaleText { auto_copied_badge } = shell;
    let TtsPlaygroundLocaleText {
        screen_record_audio_ffmpeg_downloading,
        screen_record_gif_ffmpeg_downloading,
    } = tts_playground;

    macro_rules! entries {
        ($($section:literal => [$($field:ident),* $(,)?]),* $(,)?) => {
            vec![$($(($section, stringify!($field), $field)),*),*]
        };
    }
    entries![
        "auxiliary" => [
            win_select_title,
            win_select_subtitle,
            win_select_display_only_badge,
            win_select_display_only_title,
            win_select_display_only_message,
            win_select_display_only_action,
            win_select_count,
        ],
        "badge" => [
            export_failed,
            subtitles_saved,
            feature_needs_webview2_fmt,
            feature_window_selector,
            feature_screen_record,
            install_webview2_hint,
        ],
        "global_settings" => [model_thinking],
        "preset_basics" => [cancel_label],
        "realtime" => [app_select_title, app_select_count],
        "shell" => [auto_copied_badge],
        "tts_playground" => [
            screen_record_audio_ffmpeg_downloading,
            screen_record_gif_ffmpeg_downloading,
        ],
    ]
}

#[test]
fn every_worker_string_matches_the_host_locale() {
    let languages = [
        ("en", host_sections!("en")),
        ("ko", host_sections!("ko")),
        ("vi", host_sections!("vi")),
    ];
    for (language, sections) in languages {
        for (section, field, worker) in worker_strings(LocaleText::get(language)) {
            let (_, source) = sections
                .iter()
                .find(|(name, _)| *name == section)
                .unwrap_or_else(|| panic!("no host section {section}"));
            let host = host_value(source, field)
                .unwrap_or_else(|| panic!("host {language}/{section}.rs has no {field}"));
            assert_eq!(worker, host, "{language} {section}.{field}");
        }
    }
}

#[test]
fn host_values_are_read_from_their_string_literals() {
    let source = "    T {\n        plain: \"Plain\",\n        quoted:\n            \"Say \\\"hi\\\"\",\n    }";
    assert_eq!(host_value(source, "plain").as_deref(), Some("Plain"));
    assert_eq!(host_value(source, "quoted").as_deref(), Some("Say \"hi\""));
    assert_eq!(host_value(source, "missing"), None);
}
