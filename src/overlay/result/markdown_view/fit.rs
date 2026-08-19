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
    fn streaming_retargets_adapt_velocity_to_prevent_scale_debt() {
        let script = runtime_fit_script();

        assert!(script.contains("55 + fsDelta * 7"));
        assert!(script.contains("120 + wDelta * 5"));
        assert!(script.contains("var maximumDuration = usesStreamingMotion ? 180 : 900"));
        assert!(script.contains("fitState._sgtStreamingMotionActive = true"));
        assert!(script.contains("var eased = usesStreamingMotion"));
        assert!(script.contains("? t"));
        assert!(!script.contains("_sgtContainmentTransition"));
        assert!(script.contains("fitState._sgtMotionController"));
        assert!(script.contains("motion.fontVelocity +="));
        assert!(script.contains("var steps = Math.max(1, Math.ceil(elapsed * 120))"));
        assert!(script.contains("if (motion.frame !== null) return"));
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

        assert!(!script.contains("MAX_STREAMING_REFINEMENT_PROBES"));
        assert!(script.contains("previousTarget.fontSize"));
        assert!(script.contains("needsStreamingRefinement = estimate > minSize"));
        assert!(script.contains("fitContext.requestRefinement()"));
        assert!(script.contains("if (!isStreamingFit && !foundFittingSize && !fits())"));
        assert!(script.contains("layoutProbes: layoutProbeCount"));
        assert!(script.contains("paintedShrinkPxPerSec: paintedShrinkPxPerSec"));
        assert!(!script.contains("hasPathologicalWrap"));
    }

    #[test]
    fn final_fit_reuses_contained_or_floor_streaming_geometry() {
        let script = runtime_fit_script();

        assert!(script.contains("var finalStreamingTarget = fitState._sgtLastReportedFitTarget"));
        assert!(script.contains("&& !isShortContent"));
        assert!(script.contains("finalTargetFits || targetAtReadableFloor"));
        assert!(script.contains("activeMotion.finalizing = true"));
    }

    #[test]
    fn hidden_final_settle_cancels_stale_scale_motion_before_reveal() {
        let script = runtime_fit_script();
        assert!(script.contains("if (settleBeforeReveal)"));
        assert!(script.contains("cancelFitFrame(staleMotion.frame)"));
        assert!(script.contains("fitState._sgtMotionController = null"));
    }

    #[test]
    fn ordinary_result_fitter_has_no_source_replacement_policy() {
        let script = runtime_fit_script();

        assert!(!script.contains("isSourceReplacement"));
        assert!(!script.contains("preferredFontSize"));
        assert!(!script.contains("verifiedSourceFit"));
        assert!(script.contains("for (var testWdth = 85; testWdth >= 55"));
        assert!(script.contains("for (var rescueWdth = 90; rescueWdth >= 45"));
    }

    #[test]
    fn remaining_vertical_space_keeps_the_established_result_composition() {
        let script = runtime_fit_script();

        assert!(script.contains("Math.floor(finalGap * 0.3)"));
        assert!(script.contains("Math.floor(finalGap * 0.7)"));
        assert!(!script.contains("var centeredTop"));
    }
}
