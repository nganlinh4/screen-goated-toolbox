use std::sync::LazyLock;

use super::html::DOCUMENT;

static COMPOSITOR_DOCUMENT: LazyLock<String> = LazyLock::new(build_compositor_document);

pub(super) fn compositor_document() -> String {
    COMPOSITOR_DOCUMENT.clone()
}

fn build_compositor_document() -> String {
    let scene_runtime = [
        include_str!("scene_runtime.js"),
        include_str!("scene_frame_runtime.js"),
        include_str!("scene_command_helpers.js"),
        include_str!("scene_batch_runtime.js"),
    ]
    .concat();
    let card_css = format!(
        r#"
:host{{display:block;width:100%;height:100%;overflow:hidden}}
.result-body,.result-body *{{font-family:'Google Sans Flex' !important}}
.result-body{{width:100%;height:100%;min-height:0;overflow:hidden;user-select:text}}
.result-body[data-sgt-mode="refining"]{{display:flex;align-items:center;justify-content:center;
text-align:center;padding:12px;font-style:italic;color:#aaa;font-size:16px}}
{}
{}
{}
"#,
        crate::overlay::result::markdown_view::css::MARKDOWN_CSS
            .replace(":root", ":host")
            .replace("body", ".result-body"),
        crate::overlay::html_components::grid_js::get_css(),
        crate::overlay::html_components::grid_js::get_lib_css(),
    );
    let card_css_json = serde_json::to_string(&card_css).expect("card CSS must serialize");
    let direct_runtime = include_str!("direct_runtime.js");

    DOCUMENT
        .replace(
            "__SGT_GRID_RUNTIME__",
            crate::overlay::html_components::grid_js::get_lib_js(),
        )
        // Inlined first: the scene runtime carries placeholders of its own
        // (`__SGT_CARD_CSS_JSON__`, `__SGT_BOX_RADIUS_PX__`), so it has to be
        // in the document before the substitution pass reaches them.
        .replace("__SGT_SCENE_RUNTIME__", &scene_runtime)
        .replace(
            "__SGT_SURFACE_RUNTIME__",
            &[
                include_str!("footprint_runtime.js"),
                include_str!("surface_runtime.js"),
            ]
            .concat(),
        )
        .replace("__SGT_FONT_FACE__", &super::font::face_css("font.woff2"))
        .replace("__SGT_BUTTON_CSS__", super::control_surface::document_css())
        .replace(
            "__SGT_BUTTON_SCRIPT__",
            &super::control_surface::document_script(),
        )
        .replace(
            "__SGT_BUTTON_SCENE_RUNTIME__",
            include_str!("button_scene_runtime.js"),
        )
        .replace(
            "__SGT_BOX_RADIUS_PX__",
            &crate::overlay::BOX_CORNER_RADIUS_PHYSICAL_PX.to_string(),
        )
        .replace("__SGT_CARD_CSS_JSON__", &card_css_json)
        .replace("__SGT_SHAPE_RUNTIME__", include_str!("shape_runtime.js"))
        .replace(
            "__SGT_DOM_PATCH_RUNTIME__",
            include_str!("dom_patch_runtime.js"),
        )
        .replace("__SGT_REVEAL_RUNTIME__", include_str!("reveal_runtime.js"))
        .replace("__SGT_DIRECT_RUNTIME__", direct_runtime)
        .replace(
            "__SGT_PROCESSING_RUNTIME__",
            &[
                include_str!("rectangle_glow.js"),
                include_str!("contour_field.js"),
                include_str!("contour_motion.js"),
                include_str!("contour_gpu.js"),
                include_str!("contour_runtime.js"),
            ]
            .concat(),
        )
        .replace(
            "__SGT_RESIZE_TYPOGRAPHY_RUNTIME__",
            include_str!("resize_typography_runtime.js"),
        )
        .replace("__SGT_RESIZE_RUNTIME__", include_str!("resize_runtime.js"))
        .replace(
            "__SGT_MIN_WINDOW_WIDTH_PX__",
            &crate::overlay::result::event_handler::MIN_WINDOW_WIDTH.to_string(),
        )
        .replace(
            "__SGT_MIN_WINDOW_HEIGHT_PX__",
            &crate::overlay::result::event_handler::MIN_WINDOW_HEIGHT.to_string(),
        )
        .replace(
            "__SGT_HOST_COMMAND_RUNTIME__",
            &[
                include_str!("source_image_runtime.js"),
                include_str!("host_command_runtime.js"),
            ]
            .concat(),
        )
        .replace(
            "__SGT_EXPORT_FONT_CSS_JSON__",
            &serde_json::to_string(&super::font::face_css("__SGT_EXPORT_FONT_URL__"))
                .expect("font CSS must serialize"),
        )
        .replace(
            "__SGT_SETTLED_REVEAL_RUNTIME__",
            include_str!("settled_reveal_runtime.js"),
        )
        .replace(
            "__SGT_RENDERER_BOOTSTRAP__",
            include_str!("renderer_bootstrap.js"),
        )
        .replace(
            "__SGT_FIT_RUNTIME__",
            &crate::overlay::result::markdown_view::fit::runtime_fit_script(),
        )
}

#[cfg(test)]
mod tests {
    use super::compositor_document;
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// `__SGT_NAME__` tokens left in the document. Runtime globals are always
    /// reached as `window.__SGT_NAME__`; the export font URL is substituted at
    /// export time.
    fn unsubstituted_placeholders(document: &str) -> Vec<&str> {
        let mut found = Vec::new();
        for (start, _) in document.match_indices("__SGT_") {
            if document[..start].ends_with('.') {
                continue;
            }
            let tail = &document[start + "__SGT_".len()..];
            let length = tail
                .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .unwrap_or(tail.len());
            let token = &document[start..start + "__SGT_".len() + length];
            if token.ends_with("__") && token != "__SGT_EXPORT_FONT_URL__" {
                found.push(token);
            }
        }
        found
    }

    #[test]
    fn every_document_placeholder_is_substituted() {
        assert_eq!(
            unsubstituted_placeholders(&compositor_document()),
            Vec::<&str>::new()
        );
        assert_eq!(
            unsubstituted_placeholders("a __SGT_LEFT__ window.__SGT_GLOBAL__ b"),
            vec!["__SGT_LEFT__"]
        );
    }

    /// Boots the composed document in a DOM and drives it through host
    /// commands and pointer input (`compositor_document_probe.cjs`).
    #[test]
    fn composed_document_boots_renders_cards_and_controls_and_handles_gestures() {
        let probe = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/overlay/result/scene_compositor/compositor_document_probe.cjs"
        );
        let mut child = Command::new("node")
            .arg(probe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Node.js is required for the compositor document test");
        child
            .stdin
            .take()
            .expect("probe stdin")
            .write_all(compositor_document().as_bytes())
            .expect("document must reach the probe");
        let output = child.wait_with_output().expect("probe must finish");
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
