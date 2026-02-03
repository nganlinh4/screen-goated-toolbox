//! Theme CSS generation for button canvas

/// Get theme-specific CSS variables
pub fn get_canvas_theme_css(is_dark: bool) -> &'static str {
    if is_dark {
        r#"
        :root {
            --btn-bg: rgba(30, 30, 30, 0.85);
            --btn-border: rgba(255, 255, 255, 0.1);
            --btn-color: rgba(255, 255, 255, 0.8);
            --btn-hover-bg: rgba(60, 60, 60, 0.95);
            --btn-hover-color: #4fc3f7;
            --btn-active-bg: rgba(30, 30, 30, 0.95);
            --btn-active-color: #4fc3f7;
            --btn-success-color: #81c784;
            --shadow-color: rgba(79, 195, 247, 0.35);

            /* Refine Input Variables (Dark) */
            --refine-bg: #1e1e1e;
            --refine-border: #444;
            --refine-input-bg: #2d2d2d;
            --refine-text: #fff;
            --refine-placeholder: #888;
            --mic-bg: rgba(60, 60, 60, 0.5);
            --mic-fill: #00c8ff;
        }
        "#
    } else {
        r#"
        :root {
            --btn-bg: rgba(255, 255, 255, 0.92);
            --btn-border: rgba(0, 0, 0, 0.08);
            --btn-color: rgba(0, 0, 0, 0.7);
            --btn-hover-bg: #ffffff;
            --btn-hover-color: #0277bd;
            --btn-active-bg: #ffffff;
            --btn-active-color: #0277bd;
            --btn-success-color: #43a047;
            --shadow-color: rgba(2, 119, 189, 0.25);

            /* Refine Input Variables (Light) */
            --refine-bg: #ffffff;
            --refine-border: #ddd;
            --refine-input-bg: #f5f5f5;
            --refine-text: #333;
            --refine-placeholder: #999;
            --mic-bg: rgba(0, 0, 0, 0.05);
            --mic-fill: #0288d1;
        }
        "#
    }
}
