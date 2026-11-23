//! Interactive controls rendered inside the unified result compositor.

mod actions;
mod css;
mod drag_runtime;
mod refine_runtime;
mod runtime;
mod theme;
mod work_areas;

use crate::overlay::html_components::icons::OverlayIcon;

pub(super) use work_areas::dpi_for_rect as monitor_dpi_for_rect;
pub(super) use work_areas::script as monitor_work_areas_script;

pub(crate) fn document_css() -> &'static str {
    css::get_base_css()
}

pub(crate) fn document_script() -> String {
    let l10n = serde_json::to_string(&super::localization::strings_for("en"))
        .expect("control locale must serialize");
    let icon = |icon: OverlayIcon| icon.svg().to_string();
    let icons = serde_json::json!({
        "arrow_back": icon(OverlayIcon::ArrowBack), "arrow_forward": icon(OverlayIcon::ArrowForward),
        "undo": icon(OverlayIcon::Undo), "redo": icon(OverlayIcon::Redo),
        "hourglass_empty": icon(OverlayIcon::HourglassEmpty), "stop": icon(OverlayIcon::Stop),
        "cleaning_services": icon(OverlayIcon::CleaningServices), "content_copy": icon(OverlayIcon::ContentCopy),
        "check": icon(OverlayIcon::Check), "download": icon(OverlayIcon::Download),
        "filter": icon(OverlayIcon::Filter), "auto_awesome": icon(OverlayIcon::AutoAwesome),
        "volume_up": icon(OverlayIcon::VolumeUp), "mic": icon(OverlayIcon::Mic),
        "send": icon(OverlayIcon::Send), "opacity": icon(OverlayIcon::Opacity), "close": icon(OverlayIcon::Close),
    })
    .to_string();
    [
        runtime::get_javascript(),
        include_str!("localization_runtime.js"),
        include_str!("tooltip_runtime.js"),
        include_str!("source_anchor.js"),
        include_str!("source_dock.js"),
        include_str!("placement.js"),
        include_str!("source_geometry.js"),
        include_str!("processing_preview.js"),
        include_str!("layout_runtime.js"),
        drag_runtime::get_javascript(),
        refine_runtime::get_javascript(),
    ]
    .concat()
    .replace("#L10N_JSON#", &l10n)
    .replace("#ICON_SVGS_JSON#", &icons)
}

pub(crate) fn theme_css(is_dark: bool) -> String {
    theme::get_canvas_theme_css(is_dark)
}

pub(super) use actions::{handle as handle_action, handle_drag_finished};

#[cfg(test)]
mod tests {
    #[test]
    fn result_cards_and_controls_share_one_composition_host() {
        let compositor = include_str!("../child.rs");
        let controls = include_str!("mod.rs");

        assert_eq!(
            compositor.matches("let dcomp_host = build_host(").count(),
            1
        );
        assert!(!controls.contains(&["create_canvas", "_window"].concat()));
        assert!(!controls.contains(&["Web", "Context"].concat()));
    }

    #[test]
    fn control_icons_are_downloaded_catalog_symbols() {
        crate::overlay::html_components::icons::assert_only_catalog_icons(&super::document_script());
    }

    // Control rendering, opacity echoes, refinement editing, the model badge,
    // scaling, contrast surfaces, actions, proximity and drag starts run in
    // controls_document.test.cjs and the other *.test.cjs suites here.
}
