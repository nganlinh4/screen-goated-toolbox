const FIT_FONT_SCRIPT: &str = concat!(
    include_str!("streaming/fit_impl/fit_font_script_part1.js"),
    include_str!("streaming/fit_impl/fit_font_script_part2.js"),
);

pub(crate) fn runtime_fit_script() -> String {
    FIT_FONT_SCRIPT
        .replace("__FIT_PHASE__", "fit_font_to_window_runtime")
        .replace("__STREAMING_MODE__", "Boolean(streaming)")
}

// Behaviour is covered by markdown_view/fit_runtime.test.cjs, which runs this
// composed script against a text layout model.
#[cfg(test)]
mod tests {
    use super::runtime_fit_script;

    #[test]
    fn runtime_composition_leaves_no_template_placeholder() {
        let script = runtime_fit_script();
        assert!(!script.contains("__FIT_PHASE__"));
        assert!(!script.contains("__STREAMING_MODE__"));
    }
}
