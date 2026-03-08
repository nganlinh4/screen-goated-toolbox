mod schema;
use self::schema::*;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInBackgroundPreset {
    pub family_code: f32,
    pub gradient_color1: [f32; 4],
    pub gradient_color2: [f32; 4],
    pub gradient_color3: [f32; 4],
    pub gradient_color4: [f32; 4],
    pub gradient_color5: [f32; 4],
    pub bg_params1: [f32; 4],
    pub bg_params2: [f32; 4],
    pub bg_params3: [f32; 4],
    pub bg_params4: [f32; 4],
    pub bg_params5: [f32; 4],
    pub bg_params6: [f32; 4],
}

static SHARED_BACKGROUND_PRESETS: OnceLock<HashMap<String, BuiltInBackgroundPreset>> =
    OnceLock::new();

pub fn get_builtin_background(bg_type: &str) -> Option<&'static BuiltInBackgroundPreset> {
    SHARED_BACKGROUND_PRESETS
        .get_or_init(load_shared_background_presets)
        .get(bg_type)
}

fn load_shared_background_presets() -> HashMap<String, BuiltInBackgroundPreset> {
    let raw: SharedBackgroundCatalogFile = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/screen-record/src/config/shared-background-presets.json"
    )))
    .expect("shared background preset json must parse");

    raw.backgrounds
        .into_iter()
        .map(|(id, preset)| (id, to_builtin_background_preset(preset)))
        .collect()
}

fn to_builtin_background_preset(raw: BackgroundPresetFile) -> BuiltInBackgroundPreset {
    match raw {
        BackgroundPresetFile::Linear { axis, colors } => BuiltInBackgroundPreset {
            family_code: 0.0,
            gradient_color1: hex_string_to_linear(&colors.start),
            gradient_color2: hex_string_to_linear(&colors.end),
            bg_params1: [if axis == "vertical" { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            ..BuiltInBackgroundPreset::default()
        },
        BackgroundPresetFile::StackedRadial {
            gradient_axis,
            colors,
            overlay_center,
            overlay_radius,
            overlay_opacity,
        } => BuiltInBackgroundPreset {
            family_code: 3.0,
            gradient_color1: hex_string_to_linear(&colors.start),
            gradient_color2: hex_string_to_linear(&colors.mid),
            gradient_color3: hex_string_to_linear(&colors.end),
            gradient_color4: hex_string_to_linear(&colors.overlay),
            bg_params1: [
                if gradient_axis == "horizontal" {
                    1.0
                } else {
                    0.0
                },
                0.0,
                0.0,
                0.0,
            ],
            bg_params2: [
                overlay_center[0],
                overlay_center[1],
                overlay_radius,
                overlay_opacity,
            ],
            ..BuiltInBackgroundPreset::default()
        },
        BackgroundPresetFile::DiagonalGlow {
            colors,
            diag_weights,
            split,
            glow_a_center,
            glow_a_outer_radius,
            glow_a_inner_radius,
            glow_a_strength,
            glow_b_center,
            glow_b_outer_radius,
            glow_b_inner_radius,
            glow_b_strength,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 1.0,
            gradient_color1: hex_string_to_linear(&colors.start),
            gradient_color2: hex_string_to_linear(&colors.mid),
            gradient_color3: hex_string_to_linear(&colors.end),
            gradient_color4: linear_triplet_to_rgba(colors.glow_a_color_linear),
            gradient_color5: linear_triplet_to_rgba(colors.glow_b_color_linear),
            bg_params1: [diag_weights[0], diag_weights[1], split, glow_a_strength],
            bg_params2: [
                glow_a_center[0],
                glow_a_center[1],
                glow_a_outer_radius,
                glow_a_inner_radius,
            ],
            bg_params3: [
                glow_b_center[0],
                glow_b_center[1],
                glow_b_outer_radius,
                glow_b_inner_radius,
            ],
            bg_params4: [
                glow_b_strength,
                vignette_start,
                vignette_end,
                vignette_strength,
            ],
            bg_params5: [noise_intensity, 0.0, 0.0, 0.0],
            ..BuiltInBackgroundPreset::default()
        },
        BackgroundPresetFile::EdgeRibbons {
            colors,
            ribbon_a_start,
            ribbon_a_end,
            ribbon_a_width,
            ribbon_a_curve_amp,
            ribbon_a_curve_freq,
            ribbon_a_intensity,
            ribbon_b_start,
            ribbon_b_end,
            ribbon_b_width,
            ribbon_b_curve_amp,
            ribbon_b_curve_freq,
            ribbon_b_intensity,
            glow_center,
            glow_radius,
            glow_intensity,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 2.0,
            gradient_color1: hex_string_to_linear(&colors.base),
            gradient_color2: hex_string_to_linear(&colors.depth),
            gradient_color3: hex_string_to_linear(&colors.ribbon_a),
            gradient_color4: hex_string_to_linear(&colors.ribbon_b),
            gradient_color5: hex_string_to_linear(&colors.glow),
            bg_params1: [
                ribbon_a_start[0],
                ribbon_a_start[1],
                ribbon_a_end[0],
                ribbon_a_end[1],
            ],
            bg_params2: [
                ribbon_a_width,
                ribbon_a_curve_amp,
                ribbon_a_curve_freq,
                ribbon_a_intensity,
            ],
            bg_params3: [
                ribbon_b_start[0],
                ribbon_b_start[1],
                ribbon_b_end[0],
                ribbon_b_end[1],
            ],
            bg_params4: [
                ribbon_b_width,
                ribbon_b_curve_amp,
                ribbon_b_curve_freq,
                ribbon_b_intensity,
            ],
            bg_params5: [glow_center[0], glow_center[1], glow_radius, glow_intensity],
            bg_params6: [
                vignette_start,
                vignette_end,
                vignette_strength,
                noise_intensity,
            ],
        },
        BackgroundPresetFile::PrismFold {
            colors,
            pane_a_line,
            pane_b_line,
            pane_c_line,
            pane_d_line,
            pane_strength,
            fold_strength,
            overlap_gain,
            softness,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 4.0,
            gradient_color1: hex_string_to_linear(&colors.base),
            gradient_color2: hex_string_to_linear(&colors.pane_a),
            gradient_color3: hex_string_to_linear(&colors.pane_b),
            gradient_color4: hex_string_to_linear(&colors.pane_c),
            gradient_color5: hex_string_to_linear(&colors.pane_d),
            bg_params1: pane_a_line,
            bg_params2: pane_b_line,
            bg_params3: pane_c_line,
            bg_params4: pane_d_line,
            bg_params5: [pane_strength, fold_strength, overlap_gain, softness],
            bg_params6: [
                vignette_start,
                vignette_end,
                vignette_strength,
                noise_intensity,
            ],
        },
        BackgroundPresetFile::TopographicFlow {
            colors,
            source_a,
            source_b,
            line_scale,
            warp_freq,
            warp_amp,
            line_width,
            line_strength,
            glow_strength,
            center_calm,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 5.0,
            gradient_color1: hex_string_to_linear(&colors.base),
            gradient_color2: hex_string_to_linear(&colors.line_a),
            gradient_color3: hex_string_to_linear(&colors.line_b),
            gradient_color4: hex_string_to_linear(&colors.glow),
            gradient_color5: hex_string_to_linear(&colors.ink),
            bg_params1: [source_a[0], source_a[1], source_b[0], source_b[1]],
            bg_params2: [line_scale, warp_freq, warp_amp, line_width],
            bg_params3: [line_strength, glow_strength, center_calm, 0.0],
            bg_params4: [
                vignette_start,
                vignette_end,
                vignette_strength,
                noise_intensity,
            ],
            ..BuiltInBackgroundPreset::default()
        },
        BackgroundPresetFile::WindowlightCaustics {
            colors,
            beam_a_center,
            beam_a_angle,
            beam_a_width,
            beam_a_length,
            beam_a_intensity,
            beam_b_center,
            beam_b_angle,
            beam_b_width,
            beam_b_length,
            beam_b_intensity,
            highlight_center,
            highlight_radius,
            highlight_intensity,
            caustic_freq,
            caustic_warp,
            caustic_strength,
            center_calm,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 6.0,
            gradient_color1: hex_string_to_linear(&colors.base),
            gradient_color2: hex_string_to_linear(&colors.beam_a),
            gradient_color3: hex_string_to_linear(&colors.beam_b),
            gradient_color4: hex_string_to_linear(&colors.highlight),
            gradient_color5: hex_string_to_linear(&colors.ink),
            bg_params1: [
                beam_a_center[0],
                beam_a_center[1],
                beam_a_angle,
                beam_a_width,
            ],
            bg_params2: [
                beam_a_length,
                beam_a_intensity,
                beam_b_center[0],
                beam_b_center[1],
            ],
            bg_params3: [beam_b_angle, beam_b_width, beam_b_length, beam_b_intensity],
            bg_params4: [
                highlight_center[0],
                highlight_center[1],
                highlight_radius,
                highlight_intensity,
            ],
            bg_params5: [caustic_freq, caustic_warp, caustic_strength, center_calm],
            bg_params6: [
                vignette_start,
                vignette_end,
                vignette_strength,
                noise_intensity,
            ],
        },
        BackgroundPresetFile::OrbitalArcs {
            colors,
            arc_a_center,
            arc_a_radius,
            arc_a_thickness,
            arc_a_intensity,
            arc_b_center,
            arc_b_radius,
            arc_b_thickness,
            arc_b_intensity,
            arc_c_center,
            arc_c_radius,
            arc_c_thickness,
            arc_c_intensity,
            overlap_glow,
            center_calm,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 8.0,
            gradient_color1: hex_string_to_linear(&colors.base_start),
            gradient_color2: hex_string_to_linear(&colors.base_end),
            gradient_color3: hex_string_to_linear(&colors.arc_a),
            gradient_color4: hex_string_to_linear(&colors.arc_b),
            gradient_color5: hex_string_to_linear(&colors.arc_c),
            bg_params1: [
                arc_a_center[0],
                arc_a_center[1],
                arc_a_radius,
                arc_a_thickness,
            ],
            bg_params2: [
                arc_a_intensity,
                arc_b_center[0],
                arc_b_center[1],
                arc_b_radius,
            ],
            bg_params3: [
                arc_b_thickness,
                arc_b_intensity,
                arc_c_center[0],
                arc_c_center[1],
            ],
            bg_params4: [arc_c_radius, arc_c_thickness, arc_c_intensity, overlap_glow],
            bg_params5: [center_calm, vignette_start, vignette_end, vignette_strength],
            bg_params6: [noise_intensity, 0.0, 0.0, 0.0],
        },
        BackgroundPresetFile::MeltedGlass {
            colors,
            pool_a_center,
            pool_a_radius,
            pool_a_weight,
            pool_b_center,
            pool_b_radius,
            pool_b_weight,
            pool_c_center,
            pool_c_radius,
            pool_c_weight,
            threshold,
            softness,
            rim_strength,
            highlight_strength,
            center_calm,
            vignette_start,
            vignette_end,
            vignette_strength,
            noise_intensity,
        } => BuiltInBackgroundPreset {
            family_code: 9.0,
            gradient_color1: hex_string_to_linear(&colors.base_start),
            gradient_color2: hex_string_to_linear(&colors.base_end),
            gradient_color3: hex_string_to_linear(&colors.pool_a),
            gradient_color4: hex_string_to_linear(&colors.pool_b),
            gradient_color5: hex_string_to_linear(&colors.pool_c),
            bg_params1: [
                pool_a_center[0],
                pool_a_center[1],
                pool_a_radius,
                pool_a_weight,
            ],
            bg_params2: [
                pool_b_center[0],
                pool_b_center[1],
                pool_b_radius,
                pool_b_weight,
            ],
            bg_params3: [
                pool_c_center[0],
                pool_c_center[1],
                pool_c_radius,
                pool_c_weight,
            ],
            bg_params4: [threshold, softness, rim_strength, highlight_strength],
            bg_params5: [center_calm, vignette_start, vignette_end, vignette_strength],
            bg_params6: [noise_intensity, 0.0, 0.0, 0.0],
        },
        BackgroundPresetFile::MatteCollage {
            colors,
            layer_a_center,
            layer_a_radii,
            layer_a_angle,
            layer_b_center,
            layer_b_radii,
            layer_b_angle,
            layer_c_center,
            layer_c_radii,
            layer_c_angle,
            shadow_offset,
            shadow_blur,
            shadow_strength,
            noise_intensity,
            vignette_start,
            vignette_end,
            vignette_strength,
        } => BuiltInBackgroundPreset {
            family_code: 7.0,
            gradient_color1: hex_string_to_linear(&colors.base),
            gradient_color2: hex_string_to_linear(&colors.layer_a),
            gradient_color3: hex_string_to_linear(&colors.layer_b),
            gradient_color4: hex_string_to_linear(&colors.layer_c),
            gradient_color5: hex_string_to_linear(&colors.shadow),
            bg_params1: [
                layer_a_center[0],
                layer_a_center[1],
                layer_a_radii[0],
                layer_a_radii[1],
            ],
            bg_params2: [
                layer_a_angle,
                layer_b_center[0],
                layer_b_center[1],
                layer_b_radii[0],
            ],
            bg_params3: [
                layer_b_radii[1],
                layer_b_angle,
                layer_c_center[0],
                layer_c_center[1],
            ],
            bg_params4: [
                layer_c_radii[0],
                layer_c_radii[1],
                layer_c_angle,
                shadow_offset[0],
            ],
            bg_params5: [
                shadow_offset[1],
                shadow_blur,
                shadow_strength,
                noise_intensity,
            ],
            bg_params6: [vignette_start, vignette_end, vignette_strength, 0.0],
        },
    }
}

fn linear_triplet_to_rgba(rgb: [f32; 3]) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], 1.0]
}

fn hex_string_to_linear(hex: &str) -> [f32; 4] {
    let raw = hex.trim_start_matches('#');
    let parse_channel = |start: usize| {
        u8::from_str_radix(&raw[start..start + 2], 16)
            .expect("shared background preset hex colors must be valid")
    };
    [
        srgb_to_linear(parse_channel(0) as f32 / 255.0),
        srgb_to_linear(parse_channel(2) as f32 / 255.0),
        srgb_to_linear(parse_channel(4) as f32 / 255.0),
        1.0,
    ]
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
