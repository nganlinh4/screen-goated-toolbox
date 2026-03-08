use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct SharedBackgroundCatalogFile {
    #[serde(rename = "defaultId")]
    _default_id: String,
    #[serde(rename = "panelOrder")]
    _panel_order: Vec<String>,
    backgrounds: HashMap<String, BackgroundPresetFile>,
}

#[derive(Deserialize)]
#[serde(tag = "family", rename_all = "kebab-case")]
enum BackgroundPresetFile {
    Linear {
        axis: String,
        colors: LinearColorsFile,
    },
    StackedRadial {
        #[serde(rename = "gradientAxis")]
        gradient_axis: String,
        colors: StackedRadialColorsFile,
        #[serde(rename = "overlayCenter")]
        overlay_center: [f32; 2],
        #[serde(rename = "overlayRadius")]
        overlay_radius: f32,
        #[serde(rename = "overlayOpacity")]
        overlay_opacity: f32,
    },
    DiagonalGlow {
        colors: DiagonalGlowColorsFile,
        #[serde(rename = "diagWeights")]
        diag_weights: [f32; 2],
        split: f32,
        #[serde(rename = "glowACenter")]
        glow_a_center: [f32; 2],
        #[serde(rename = "glowAOuterRadius")]
        glow_a_outer_radius: f32,
        #[serde(rename = "glowAInnerRadius")]
        glow_a_inner_radius: f32,
        #[serde(rename = "glowAStrength")]
        glow_a_strength: f32,
        #[serde(rename = "glowBCenter")]
        glow_b_center: [f32; 2],
        #[serde(rename = "glowBOuterRadius")]
        glow_b_outer_radius: f32,
        #[serde(rename = "glowBInnerRadius")]
        glow_b_inner_radius: f32,
        #[serde(rename = "glowBStrength")]
        glow_b_strength: f32,
        #[serde(rename = "vignetteStart")]
        vignette_start: f32,
        #[serde(rename = "vignetteEnd")]
        vignette_end: f32,
        #[serde(rename = "vignetteStrength")]
        vignette_strength: f32,
        #[serde(rename = "noiseIntensity")]
        noise_intensity: f32,
    },
    EdgeRibbons {
        colors: EdgeRibbonColorsFile,
        #[serde(rename = "ribbonAStart")]
        ribbon_a_start: [f32; 2],
        #[serde(rename = "ribbonAEnd")]
        ribbon_a_end: [f32; 2],
        #[serde(rename = "ribbonAWidth")]
        ribbon_a_width: f32,
        #[serde(rename = "ribbonACurveAmp")]
        ribbon_a_curve_amp: f32,
        #[serde(rename = "ribbonACurveFreq")]
        ribbon_a_curve_freq: f32,
        #[serde(rename = "ribbonAIntensity")]
        ribbon_a_intensity: f32,
        #[serde(rename = "ribbonBStart")]
        ribbon_b_start: [f32; 2],
        #[serde(rename = "ribbonBEnd")]
        ribbon_b_end: [f32; 2],
        #[serde(rename = "ribbonBWidth")]
        ribbon_b_width: f32,
        #[serde(rename = "ribbonBCurveAmp")]
        ribbon_b_curve_amp: f32,
        #[serde(rename = "ribbonBCurveFreq")]
        ribbon_b_curve_freq: f32,
        #[serde(rename = "ribbonBIntensity")]
        ribbon_b_intensity: f32,
        #[serde(rename = "glowCenter")]
        glow_center: [f32; 2],
        #[serde(rename = "glowRadius")]
        glow_radius: f32,
        #[serde(rename = "glowIntensity")]
        glow_intensity: f32,
        #[serde(rename = "vignetteStart")]
        vignette_start: f32,
        #[serde(rename = "vignetteEnd")]
        vignette_end: f32,
        #[serde(rename = "vignetteStrength")]
        vignette_strength: f32,
        #[serde(rename = "noiseIntensity")]
        noise_intensity: f32,
    },
    PrismFold {
        colors: PrismFoldColorsFile,
        #[serde(rename = "paneALine")]
        pane_a_line: [f32; 4],
        #[serde(rename = "paneBLine")]
        pane_b_line: [f32; 4],
        #[serde(rename = "paneCLine")]
        pane_c_line: [f32; 4],
        #[serde(rename = "paneDLine")]
        pane_d_line: [f32; 4],
        #[serde(rename = "paneStrength")]
        pane_strength: f32,
        #[serde(rename = "foldStrength")]
        fold_strength: f32,
        #[serde(rename = "overlapGain")]
        overlap_gain: f32,
        softness: f32,
        #[serde(rename = "vignetteStart")]
        vignette_start: f32,
        #[serde(rename = "vignetteEnd")]
        vignette_end: f32,
        #[serde(rename = "vignetteStrength")]
        vignette_strength: f32,
        #[serde(rename = "noiseIntensity")]
        noise_intensity: f32,
    },
    TopographicFlow {
        colors: TopographicFlowColorsFile,
        #[serde(rename = "sourceA")]
        source_a: [f32; 2],
        #[serde(rename = "sourceB")]
        source_b: [f32; 2],
        #[serde(rename = "lineScale")]
        line_scale: f32,
        #[serde(rename = "warpFreq")]
        warp_freq: f32,
        #[serde(rename = "warpAmp")]
        warp_amp: f32,
        #[serde(rename = "lineWidth")]
        line_width: f32,
        #[serde(rename = "lineStrength")]
        line_strength: f32,
        #[serde(rename = "glowStrength")]
        glow_strength: f32,
        #[serde(rename = "centerCalm")]
        center_calm: f32,
        #[serde(rename = "vignetteStart")]
        vignette_start: f32,
        #[serde(rename = "vignetteEnd")]
        vignette_end: f32,
        #[serde(rename = "vignetteStrength")]
        vignette_strength: f32,
        #[serde(rename = "noiseIntensity")]
        noise_intensity: f32,
    },
}

#[derive(Deserialize)]
struct LinearColorsFile {
    start: String,
    end: String,
}

#[derive(Deserialize)]
struct StackedRadialColorsFile {
    start: String,
    mid: String,
    end: String,
    overlay: String,
}

#[derive(Deserialize)]
struct DiagonalGlowColorsFile {
    start: String,
    mid: String,
    end: String,
    #[serde(rename = "glowAColorLinear")]
    glow_a_color_linear: [f32; 3],
    #[serde(rename = "glowBColorLinear")]
    glow_b_color_linear: [f32; 3],
}

#[derive(Deserialize)]
struct EdgeRibbonColorsFile {
    base: String,
    depth: String,
    #[serde(rename = "ribbonA")]
    ribbon_a: String,
    #[serde(rename = "ribbonB")]
    ribbon_b: String,
    glow: String,
}

#[derive(Deserialize)]
struct PrismFoldColorsFile {
    base: String,
    #[serde(rename = "paneA")]
    pane_a: String,
    #[serde(rename = "paneB")]
    pane_b: String,
    #[serde(rename = "paneC")]
    pane_c: String,
    #[serde(rename = "paneD")]
    pane_d: String,
}

#[derive(Deserialize)]
struct TopographicFlowColorsFile {
    base: String,
    #[serde(rename = "lineA")]
    line_a: String,
    #[serde(rename = "lineB")]
    line_b: String,
    glow: String,
    ink: String,
}

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
