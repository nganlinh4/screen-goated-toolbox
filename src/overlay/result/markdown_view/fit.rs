const FIT_FONT_SCRIPT: &str = concat!(
    include_str!("streaming/fit_impl/fit_font_script_part1.js"),
    include_str!("streaming/fit_impl/fit_font_script_part2.js"),
);

pub(crate) fn runtime_fit_script() -> String {
    FIT_FONT_SCRIPT
        .replace("__FIT_PHASE__", "fit_font_to_window_runtime")
        .replace("__STREAMING_MODE__", "Boolean(streaming)")
}

#[cfg(test)]
mod tests {
    use super::runtime_fit_script;

    #[test]
    fn runtime_text_measurement_excludes_embedded_scripts() {
        let script = runtime_fit_script();

        assert!(script.contains("tag !== 'SCRIPT'"));
        assert!(script.contains("NodeFilter.SHOW_TEXT"));
        assert!(script.contains("required_font_unavailable"));
        assert!(!script.contains("_sgtFontLoadFailed"));
        assert!(script.contains("runFitWhenReady();"));
        assert!(!script.contains("__STREAMING_MODE__"));
    }

    #[test]
    fn streaming_refits_as_soon_as_the_displayed_size_overflows() {
        let script = runtime_fit_script();

        assert!(!script.contains("hystOverRatio"));
        assert!(script.contains("if (fits()) {"));
        assert!(script.contains("preservedSize = true"));
        assert!(script.contains("action: 'fit_target'"));
    }

    #[test]
    fn queued_fits_allow_the_active_scale_to_paint_before_retargeting() {
        let script = runtime_fit_script();
        let readiness_frame = script.find("function runFitWhenReady()").unwrap();
        let cancellation = script.find("cancelFitFrame(fitState._sgtFitAnim)").unwrap();
        let displayed_axis_capture = script
            .find("var priorDisplayedFontSize = parseFloat(body.style.fontSize)")
            .unwrap();

        assert!(cancellation > readiness_frame);
        assert!(cancellation < displayed_axis_capture);
        assert!(script.contains("scheduleFitFrame(tick)"));
    }

    #[test]
    fn streaming_retargets_preserve_constant_scale_velocity() {
        let script = runtime_fit_script();

        assert!(script.contains("var minimumDuration = isStreamingFit ? 16 : 140"));
        assert!(script.contains("var eased = isStreamingFit"));
        assert!(script.contains("? t"));
    }

    #[test]
    fn hidden_final_render_commits_its_target_without_interpolation() {
        let script = runtime_fit_script();

        assert!(script.contains("const settleBeforeReveal = Boolean(fitContext"));
        assert!(script.contains("if (settleBeforeReveal || !hadPriorSize"));
        assert!(script.contains("bodyRef.style.setProperty('opacity', '1', 'important')"));
        assert!(script.contains("settleBeforeReveal: settleBeforeReveal"));
    }

    #[test]
    fn streaming_target_search_has_bounded_layout_work() {
        let script = runtime_fit_script();

        assert!(script.contains("MAX_STREAMING_REFINEMENT_PROBES = 2"));
        assert!(script.contains("previousTarget.fontSize"));
        assert!(script.contains("layoutProbes: layoutProbeCount"));
        assert!(script.contains("paintedShrinkPxPerSec: paintedShrinkPxPerSec"));
        assert!(!script.contains("hasPathologicalWrap"));
    }
}
