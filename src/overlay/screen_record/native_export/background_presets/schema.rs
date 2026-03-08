use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub(super) struct SharedBackgroundCatalogFile {
    #[serde(rename = "defaultId")]
    pub(super) _default_id: String,
    #[serde(rename = "panelOrder")]
    pub(super) _panel_order: Vec<String>,
    pub(super) backgrounds: HashMap<String, BackgroundPresetFile>,
}

#[derive(Deserialize)]
#[serde(tag = "family", rename_all = "kebab-case")]
pub(super) enum BackgroundPresetFile {
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
    WindowlightCaustics {
        colors: WindowlightCausticsColorsFile,
        #[serde(rename = "beamACenter")]
        beam_a_center: [f32; 2],
        #[serde(rename = "beamAAngle")]
        beam_a_angle: f32,
        #[serde(rename = "beamAWidth")]
        beam_a_width: f32,
        #[serde(rename = "beamALength")]
        beam_a_length: f32,
        #[serde(rename = "beamAIntensity")]
        beam_a_intensity: f32,
        #[serde(rename = "beamBCenter")]
        beam_b_center: [f32; 2],
        #[serde(rename = "beamBAngle")]
        beam_b_angle: f32,
        #[serde(rename = "beamBWidth")]
        beam_b_width: f32,
        #[serde(rename = "beamBLength")]
        beam_b_length: f32,
        #[serde(rename = "beamBIntensity")]
        beam_b_intensity: f32,
        #[serde(rename = "highlightCenter")]
        highlight_center: [f32; 2],
        #[serde(rename = "highlightRadius")]
        highlight_radius: f32,
        #[serde(rename = "highlightIntensity")]
        highlight_intensity: f32,
        #[serde(rename = "causticFreq")]
        caustic_freq: f32,
        #[serde(rename = "causticWarp")]
        caustic_warp: f32,
        #[serde(rename = "causticStrength")]
        caustic_strength: f32,
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
    OrbitalArcs {
        colors: OrbitalArcsColorsFile,
        #[serde(rename = "arcACenter")]
        arc_a_center: [f32; 2],
        #[serde(rename = "arcARadius")]
        arc_a_radius: f32,
        #[serde(rename = "arcAThickness")]
        arc_a_thickness: f32,
        #[serde(rename = "arcAIntensity")]
        arc_a_intensity: f32,
        #[serde(rename = "arcBCenter")]
        arc_b_center: [f32; 2],
        #[serde(rename = "arcBRadius")]
        arc_b_radius: f32,
        #[serde(rename = "arcBThickness")]
        arc_b_thickness: f32,
        #[serde(rename = "arcBIntensity")]
        arc_b_intensity: f32,
        #[serde(rename = "arcCCenter")]
        arc_c_center: [f32; 2],
        #[serde(rename = "arcCRadius")]
        arc_c_radius: f32,
        #[serde(rename = "arcCThickness")]
        arc_c_thickness: f32,
        #[serde(rename = "arcCIntensity")]
        arc_c_intensity: f32,
        #[serde(rename = "overlapGlow")]
        overlap_glow: f32,
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
    MeltedGlass {
        colors: MeltedGlassColorsFile,
        #[serde(rename = "poolACenter")]
        pool_a_center: [f32; 2],
        #[serde(rename = "poolARadius")]
        pool_a_radius: f32,
        #[serde(rename = "poolAWeight")]
        pool_a_weight: f32,
        #[serde(rename = "poolBCenter")]
        pool_b_center: [f32; 2],
        #[serde(rename = "poolBRadius")]
        pool_b_radius: f32,
        #[serde(rename = "poolBWeight")]
        pool_b_weight: f32,
        #[serde(rename = "poolCCenter")]
        pool_c_center: [f32; 2],
        #[serde(rename = "poolCRadius")]
        pool_c_radius: f32,
        #[serde(rename = "poolCWeight")]
        pool_c_weight: f32,
        threshold: f32,
        softness: f32,
        #[serde(rename = "rimStrength")]
        rim_strength: f32,
        #[serde(rename = "highlightStrength")]
        highlight_strength: f32,
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
    MatteCollage {
        colors: MatteCollageColorsFile,
        #[serde(rename = "layerACenter")]
        layer_a_center: [f32; 2],
        #[serde(rename = "layerARadii")]
        layer_a_radii: [f32; 2],
        #[serde(rename = "layerAAngle")]
        layer_a_angle: f32,
        #[serde(rename = "layerBCenter")]
        layer_b_center: [f32; 2],
        #[serde(rename = "layerBRadii")]
        layer_b_radii: [f32; 2],
        #[serde(rename = "layerBAngle")]
        layer_b_angle: f32,
        #[serde(rename = "layerCCenter")]
        layer_c_center: [f32; 2],
        #[serde(rename = "layerCRadii")]
        layer_c_radii: [f32; 2],
        #[serde(rename = "layerCAngle")]
        layer_c_angle: f32,
        #[serde(rename = "shadowOffset")]
        shadow_offset: [f32; 2],
        #[serde(rename = "shadowBlur")]
        shadow_blur: f32,
        #[serde(rename = "shadowStrength")]
        shadow_strength: f32,
        #[serde(rename = "noiseIntensity")]
        noise_intensity: f32,
        #[serde(rename = "vignetteStart")]
        vignette_start: f32,
        #[serde(rename = "vignetteEnd")]
        vignette_end: f32,
        #[serde(rename = "vignetteStrength")]
        vignette_strength: f32,
    },
}

#[derive(Deserialize)]
pub(super) struct LinearColorsFile {
    pub(super) start: String,
    pub(super) end: String,
}

#[derive(Deserialize)]
pub(super) struct StackedRadialColorsFile {
    pub(super) start: String,
    pub(super) mid: String,
    pub(super) end: String,
    pub(super) overlay: String,
}

#[derive(Deserialize)]
pub(super) struct DiagonalGlowColorsFile {
    pub(super) start: String,
    pub(super) mid: String,
    pub(super) end: String,
    #[serde(rename = "glowAColorLinear")]
    pub(super) glow_a_color_linear: [f32; 3],
    #[serde(rename = "glowBColorLinear")]
    pub(super) glow_b_color_linear: [f32; 3],
}

#[derive(Deserialize)]
pub(super) struct EdgeRibbonColorsFile {
    pub(super) base: String,
    pub(super) depth: String,
    #[serde(rename = "ribbonA")]
    pub(super) ribbon_a: String,
    #[serde(rename = "ribbonB")]
    pub(super) ribbon_b: String,
    pub(super) glow: String,
}

#[derive(Deserialize)]
pub(super) struct PrismFoldColorsFile {
    pub(super) base: String,
    #[serde(rename = "paneA")]
    pub(super) pane_a: String,
    #[serde(rename = "paneB")]
    pub(super) pane_b: String,
    #[serde(rename = "paneC")]
    pub(super) pane_c: String,
    #[serde(rename = "paneD")]
    pub(super) pane_d: String,
}

#[derive(Deserialize)]
pub(super) struct TopographicFlowColorsFile {
    pub(super) base: String,
    #[serde(rename = "lineA")]
    pub(super) line_a: String,
    #[serde(rename = "lineB")]
    pub(super) line_b: String,
    pub(super) glow: String,
    pub(super) ink: String,
}

#[derive(Deserialize)]
pub(super) struct WindowlightCausticsColorsFile {
    pub(super) base: String,
    #[serde(rename = "beamA")]
    pub(super) beam_a: String,
    #[serde(rename = "beamB")]
    pub(super) beam_b: String,
    pub(super) highlight: String,
    pub(super) ink: String,
}

#[derive(Deserialize)]
pub(super) struct OrbitalArcsColorsFile {
    #[serde(rename = "baseStart")]
    pub(super) base_start: String,
    #[serde(rename = "baseEnd")]
    pub(super) base_end: String,
    #[serde(rename = "arcA")]
    pub(super) arc_a: String,
    #[serde(rename = "arcB")]
    pub(super) arc_b: String,
    #[serde(rename = "arcC")]
    pub(super) arc_c: String,
}

#[derive(Deserialize)]
pub(super) struct MeltedGlassColorsFile {
    #[serde(rename = "baseStart")]
    pub(super) base_start: String,
    #[serde(rename = "baseEnd")]
    pub(super) base_end: String,
    #[serde(rename = "poolA")]
    pub(super) pool_a: String,
    #[serde(rename = "poolB")]
    pub(super) pool_b: String,
    #[serde(rename = "poolC")]
    pub(super) pool_c: String,
}

#[derive(Deserialize)]
pub(super) struct MatteCollageColorsFile {
    pub(super) base: String,
    #[serde(rename = "layerA")]
    pub(super) layer_a: String,
    #[serde(rename = "layerB")]
    pub(super) layer_b: String,
    #[serde(rename = "layerC")]
    pub(super) layer_c: String,
    pub(super) shadow: String,
}
