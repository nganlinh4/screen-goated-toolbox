//! Icons embedded in overlay HTML. An enum, so a misspelled icon fails to
//! compile instead of rendering an empty button.

// The recorder worker compiles this module for the auto-copy badge alone.
#[cfg_attr(feature = "recorder-worker", allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayIcon {
    Mic,
    SpeakerGroup,
    VolumeUp,
    Dictionary,
    Filter,
    ContentCopy,
    Remove,
    Add,
    Subtitles,
    Translate,
    ExpandLess,
    ResizeCorner,
    Apps,
    PlayArrow,
    Close,
    Pause,
    Check,
    Download,
    Send,
    ArrowBack,
    ArrowForward,
    Undo,
    Redo,
    HourglassEmpty,
    Stop,
    CleaningServices,
    Opacity,
    GraphicEq,
    Image,
    Keyboard,
    FormatItalic,
    AutoAwesome,
}

/// Size every downloaded symbol carries (`1em`), replaced by an icon's
/// intrinsic pixel size where its surface has no CSS size of its own.
const CATALOG_SIZE: &str = r#"width="1em" height="1em""#;

#[cfg_attr(feature = "recorder-worker", allow(dead_code))]
impl OverlayIcon {
    /// The downloaded Material Symbol, sized for the surfaces that use it.
    pub fn svg(self) -> String {
        match self.intrinsic_size() {
            Some(px) => {
                self.source()
                    .replacen(CATALOG_SIZE, &format!(r#"width="{px}" height="{px}""#), 1)
            }
            None => self.source().to_owned(),
        }
    }

    /// The symbol at an explicit pixel size, for surfaces that size per use.
    pub fn svg_at(self, px: u32) -> String {
        self.source()
            .replacen(CATALOG_SIZE, &format!(r#"width="{px}" height="{px}""#), 1)
    }

    const fn source(self) -> &'static str {
        match self {
            Self::Mic => include_str!("../../../ui-shared/material-symbols/mic.svg"),
            Self::SpeakerGroup => {
                include_str!("../../../ui-shared/material-symbols/speaker_group.svg")
            }
            Self::VolumeUp => include_str!("../../../ui-shared/material-symbols/volume_up.svg"),
            Self::Dictionary => include_str!("../../../ui-shared/material-symbols/dictionary.svg"),
            Self::Filter => include_str!("../../../ui-shared/material-symbols/filter.svg"),
            Self::ContentCopy => {
                include_str!("../../../ui-shared/material-symbols/content_copy.svg")
            }
            Self::Remove => include_str!("../../../ui-shared/material-symbols/remove.svg"),
            Self::Add => include_str!("../../../ui-shared/material-symbols/add.svg"),
            Self::Subtitles => include_str!("../../../ui-shared/material-symbols/subtitles.svg"),
            Self::Translate => include_str!("../../../ui-shared/material-symbols/translate.svg"),
            Self::ExpandLess => include_str!("../../../ui-shared/material-symbols/expand_less.svg"),
            Self::ResizeCorner => {
                include_str!("../../../ui-shared/material-symbols/open_in_full.svg")
            }
            Self::Apps => include_str!("../../../ui-shared/material-symbols/apps.svg"),
            Self::PlayArrow => include_str!("../../../ui-shared/material-symbols/play_arrow.svg"),
            Self::Close => include_str!("../../../ui-shared/material-symbols/close.svg"),
            Self::Pause => include_str!("../../../ui-shared/material-symbols/pause.svg"),
            Self::Check => include_str!("../../../ui-shared/material-symbols/check.svg"),
            Self::Download => include_str!("../../../ui-shared/material-symbols/download.svg"),
            Self::Send => include_str!("../../../ui-shared/material-symbols/send.svg"),
            Self::ArrowBack => include_str!("../../../ui-shared/material-symbols/arrow_back.svg"),
            Self::ArrowForward => {
                include_str!("../../../ui-shared/material-symbols/arrow_forward.svg")
            }
            Self::Undo => include_str!("../../../ui-shared/material-symbols/undo.svg"),
            Self::Redo => include_str!("../../../ui-shared/material-symbols/redo.svg"),
            Self::HourglassEmpty => {
                include_str!("../../../ui-shared/material-symbols/hourglass_empty.svg")
            }
            Self::Stop => include_str!("../../../ui-shared/material-symbols/stop.svg"),
            Self::CleaningServices => {
                include_str!("../../../ui-shared/material-symbols/cleaning_services.svg")
            }
            Self::Opacity => include_str!("../../../ui-shared/material-symbols/opacity.svg"),
            Self::GraphicEq => include_str!("../../../ui-shared/material-symbols/graphic_eq.svg"),
            Self::Image => include_str!("../../../ui-shared/material-symbols/image.svg"),
            Self::Keyboard => include_str!("../../../ui-shared/material-symbols/keyboard.svg"),
            Self::FormatItalic => {
                include_str!("../../../ui-shared/material-symbols/format_italic.svg")
            }
            Self::AutoAwesome => {
                include_str!("../../../ui-shared/material-symbols/auto_awesome.svg")
            }
        }
    }

    /// Pixel size these icons had before they used the shared catalog. The
    /// rest keep `1em`: they already used it, or their surface's CSS sizes them.
    const fn intrinsic_size(self) -> Option<u32> {
        match self {
            Self::Mic
            | Self::SpeakerGroup
            | Self::Remove
            | Self::Subtitles
            | Self::Translate
            | Self::ExpandLess
            | Self::ResizeCorner
            | Self::Apps
            | Self::Send => Some(24),
            Self::VolumeUp
            | Self::ContentCopy
            | Self::Check
            | Self::Download
            | Self::ArrowBack
            | Self::ArrowForward
            | Self::Undo
            | Self::Redo
            | Self::HourglassEmpty
            | Self::Stop
            | Self::CleaningServices
            | Self::Opacity => Some(16),
            Self::PlayArrow | Self::Pause => Some(40),
            Self::AutoAwesome => Some(14),
            Self::Dictionary
            | Self::Filter
            | Self::Add
            | Self::Close
            | Self::GraphicEq
            | Self::Image
            | Self::Keyboard
            | Self::FormatItalic => None,
        }
    }
}

/// Every icon, for tests that check a page embeds only catalog symbols.
#[cfg(test)]
pub(crate) const ALL: &[OverlayIcon] = &[
    OverlayIcon::Mic,
    OverlayIcon::SpeakerGroup,
    OverlayIcon::VolumeUp,
    OverlayIcon::Dictionary,
    OverlayIcon::Filter,
    OverlayIcon::ContentCopy,
    OverlayIcon::Remove,
    OverlayIcon::Add,
    OverlayIcon::Subtitles,
    OverlayIcon::Translate,
    OverlayIcon::ExpandLess,
    OverlayIcon::ResizeCorner,
    OverlayIcon::Apps,
    OverlayIcon::PlayArrow,
    OverlayIcon::Close,
    OverlayIcon::Pause,
    OverlayIcon::Check,
    OverlayIcon::Download,
    OverlayIcon::Send,
    OverlayIcon::ArrowBack,
    OverlayIcon::ArrowForward,
    OverlayIcon::Undo,
    OverlayIcon::Redo,
    OverlayIcon::HourglassEmpty,
    OverlayIcon::Stop,
    OverlayIcon::CleaningServices,
    OverlayIcon::Opacity,
    OverlayIcon::GraphicEq,
    OverlayIcon::Image,
    OverlayIcon::Keyboard,
    OverlayIcon::FormatItalic,
    OverlayIcon::AutoAwesome,
];

/// Panics unless every SVG path in `html` (raw or inside a JS string) is the
/// path of a downloaded catalog symbol, so pasted icon geometry fails a test.
#[cfg(test)]
pub(crate) fn assert_only_catalog_icons(html: &str) {
    fn path_data(text: &str) -> Vec<&str> {
        let mut found = Vec::new();
        for (open, close) in [(" d=\"", "\""), (" d=\\\"", "\\\"")] {
            let mut rest = text;
            while let Some(at) = rest.find(open) {
                let value = &rest[at + open.len()..];
                let Some(len) = value.find(close) else { break };
                found.push(&value[..len]);
                rest = &value[len..];
            }
        }
        found
    }
    let catalog: Vec<&str> = ALL
        .iter()
        .flat_map(|icon| path_data(icon.source()))
        .collect();
    let paths = path_data(html);
    assert!(!paths.is_empty(), "the page shows no icon");
    for path in paths {
        assert!(catalog.contains(&path), "pasted icon path: {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::{ALL, OverlayIcon};

    #[test]
    fn every_icon_is_one_current_color_svg() {
        for icon in ALL {
            let svg = icon.svg();
            let svg = svg.trim();
            assert!(
                svg.starts_with("<svg") && svg.ends_with("</svg>"),
                "{icon:?}"
            );
            assert_eq!(svg.matches("<svg").count(), 1, "{icon:?}");
            assert!(svg.contains("currentColor"), "{icon:?}");
        }
    }

    #[test]
    fn every_icon_is_a_downloaded_symbol_at_a_fixed_or_catalog_size() {
        for icon in ALL {
            // Downloaded symbols share one root; pasted path data would not.
            assert!(
                icon.source().starts_with(concat!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" "#,
                    r#"width="1em" height="1em" viewBox="0 0 24 24">"#
                )),
                "{icon:?}"
            );
            let svg = icon.svg();
            match icon.intrinsic_size() {
                Some(px) => {
                    let size = format!(r#"width="{px}" height="{px}""#);
                    assert!(svg.contains(&size), "{icon:?}");
                }
                None => assert!(svg.contains(super::CATALOG_SIZE), "{icon:?}"),
            }
        }
        assert!(OverlayIcon::PlayArrow.svg().contains(r#"width="40""#));
    }
}
