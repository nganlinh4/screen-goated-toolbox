use super::cursors::{CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS, CURSOR_MIP_LEVELS, CURSOR_TILE_SIZE};
mod backgrounds;
mod cursor;
use self::backgrounds::COMPOSITOR_SHADER_BACKGROUND_FUNCTIONS;
use self::cursor::COMPOSITOR_SHADER_CURSOR_FUNCTIONS;

const COMPOSITOR_SHADER_HEAD: &str = r#"
struct Uniforms {
    video_offset: vec2<f32>,
    video_scale: vec2<f32>,
    output_size: vec2<f32>,
    video_size: vec2<f32>,
    border_radius: f32,
    shadow_offset: f32,
    shadow_blur: f32,
    shadow_opacity: f32,
    gradient_color1: vec4<f32>,
    gradient_color2: vec4<f32>,
    gradient_color3: vec4<f32>,
    gradient_color4: vec4<f32>,
    gradient_color5: vec4<f32>,
    time: f32,
    render_mode: f32,
    cursor_pos: vec2<f32>,
    cursor_scale: f32,
    cursor_opacity: f32,
    cursor_type_id: f32,
    cursor_rotation: f32,
    cursor_shadow: f32,
    use_background_texture: f32,
    bg_zoom: f32,
    bg_anchor_x: f32,
    bg_anchor_y: f32,
    bg_style: f32,
    bg_tex_w: f32,
    bg_tex_h: f32,
    bg_params1: vec4<f32>,
    bg_params2: vec4<f32>,
    bg_params3: vec4<f32>,
    bg_params4: vec4<f32>,
    bg_params5: vec4<f32>,
    bg_params6: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

@group(1) @binding(0) var video_tex: texture_2d<f32>;
@group(1) @binding(1) var video_samp: sampler;

@group(2) @binding(0) var cursor_tex: texture_2d<f32>;
@group(2) @binding(1) var cursor_samp: sampler;

@group(3) @binding(0) var bg_tex: texture_2d<f32>;
@group(3) @binding(1) var bg_samp: sampler;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = vec4<f32>(pos, 0.0, 1.0);
    out.tex_coord = uv;
    out.pixel_pos = uv * u.output_size;
    return out;
}

fn sd_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}
"#;

const COMPOSITOR_SHADER_TAIL: &str = r#"

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let bg_zoom = max(u.bg_zoom, 0.0001);
    let bg_off = vec2<f32>(
        (1.0 - bg_zoom) * u.bg_anchor_x,
        (1.0 - bg_zoom) * u.bg_anchor_y
    );
    let bg_uv = (in.tex_coord - bg_off) / bg_zoom;

    // render_mode: 0=all, 1=scene-only (no cursor), 2=cursor-only (no scene).
    var col = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if (u.render_mode < 1.5) {
        // 1. Background
        let t = clamp(select(bg_uv.x, bg_uv.y, u.bg_params1.x > 0.5), 0.0, 1.0);
        // Canvas linear gradients interpolate the encoded sRGB stops.
        col = vec4<f32>(
            mix_srgb(u.gradient_color1.rgb, u.gradient_color2.rgb, t),
            mix(u.gradient_color1.a, u.gradient_color2.a, t)
        );
        if (u.use_background_texture > 0.5) {
            // object-fit: cover — scale bg_uv so the texture always fills the canvas.
            let canvas_aspect = u.output_size.x / max(u.output_size.y, 1.0);
            let tex_aspect = u.bg_tex_w / max(u.bg_tex_h, 1.0);
            var cover_uv = bg_uv;
            if (canvas_aspect > tex_aspect) {
                // Canvas wider than image → image fills width, crop top/bottom.
                let scale = tex_aspect / canvas_aspect;
                cover_uv.y = (bg_uv.y - 0.5) * scale + 0.5;
            } else {
                // Canvas taller than image → image fills height, crop left/right.
                let scale = canvas_aspect / tex_aspect;
                cover_uv.x = (bg_uv.x - 0.5) * scale + 0.5;
            }
            col = textureSample(bg_tex, bg_samp, cover_uv);
        }
        // Background family-code dispatch (WYSIWYG export path).
        // `u.bg_style` is the float `family_code` emitted by
        // `native_export/background_presets.rs`. This `> N.5` chain and that
        // table are two hand-kept integer tables that MUST stay in sync — do NOT
        // change a threshold here without updating the family_code there, or a
        // preset renders with the wrong shader function:
        //   bg_style <= 0.5 → base linear gradient (Linear, code 0.0)
        //   > 0.5  → diagonal_glow_color        (DiagonalGlow,        code 1.0)
        //   > 1.5  → edge_ribbons_color         (EdgeRibbons,         code 2.0)
        //   > 2.5  → stacked_radial_color       (StackedRadial,       code 3.0)
        //   > 3.5  → prism_fold_color           (PrismFold,           code 4.0)
        //   > 4.5  → topographic_flow_color     (TopographicFlow,     code 5.0)
        //   > 5.5  → windowlight_caustics_color (WindowlightCaustics, code 6.0)
        //   > 6.5  → matte_collage_color        (MatteCollage,        code 7.0)
        //   > 7.5  → orbital_arcs_color         (OrbitalArcs,         code 8.0)
        //   > 8.5  → melted_glass_color         (MeltedGlass,         code 9.0)
        else if (u.bg_style > 8.5) {
            col = melted_glass_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 7.5) {
            col = orbital_arcs_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 6.5) {
            col = matte_collage_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 5.5) {
            col = windowlight_caustics_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 4.5) {
            col = topographic_flow_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 3.5) {
            col = prism_fold_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 2.5) {
            col = stacked_radial_color(bg_uv);
        } else if (u.bg_style > 1.5) {
            col = edge_ribbons_color(bg_uv, in.pixel_pos);
        } else if (u.bg_style > 0.5) {
            col = diagonal_glow_color(bg_uv, in.pixel_pos);
        }

        // Video positioning
        let vid_center = u.video_offset * u.output_size + u.video_size * 0.5;
        let vid_half = u.video_size * 0.5;
        let dist = sd_box(in.pixel_pos - vid_center, vid_half, u.border_radius);
        let vid_min = u.video_offset * u.output_size;
        let vid_max = vid_min + u.video_size;
        let unmasked_canvas_cover = u.border_radius <= 0.0 &&
            vid_min.x <= 0.001 && vid_min.y <= 0.001 &&
            vid_max.x >= u.output_size.x - 0.001 &&
            vid_max.y >= u.output_size.y - 0.001;

        // 2. Shadow
        if u.shadow_opacity > 0.0 {
            // Match preview shadow direction: vertical drop only (no X offset).
            let sh_center = vid_center + vec2<f32>(0.0, u.shadow_offset);
            let sh_dist = sd_box(in.pixel_pos - sh_center, vid_half, u.border_radius);
            // Improved shadow softness matching canvas
            let sh_alpha = 1.0 - smoothstep(-u.shadow_blur, u.shadow_blur, sh_dist);
            col = mix(col, vec4<f32>(0.0,0.0,0.0, u.shadow_opacity), sh_alpha * u.shadow_opacity);
        }

        // 3. Video Content
        if dist < 0.0 {
            let vid_uv = (in.pixel_pos - u.video_offset * u.output_size) / u.video_size;
            var vid_col = textureSample(video_tex, video_samp, vid_uv);

            if unmasked_canvas_cover {
                // The canvas boundary clips this surface. Mixing the background
                // here would create a visible fringe despite zero corner radius.
                col = vid_col;
            } else {
                let edge = 1.0 - smoothstep(-1.5, 0.0, dist);
                col = mix(col, vid_col, edge);
            }
        }

        // The preview strokes a 1px rgba(0,0,0,0.1) outline on the frame path
        // while clipped to it, so only the inner half-pixel band shows.
        if !unmasked_canvas_cover {
            let band = max(0.0, min(dist + 0.5, 0.0) - max(dist - 0.5, -0.5));
            if band > 0.0 {
                col = vec4<f32>(mix_srgb(col.rgb, vec3<f32>(0.0), 0.1 * band), col.a);
            }
        }
    }

    // 4. Cursor Overlay (drawn over both video and background)
    // render_mode 0 or 2 renders the cursor; render_mode 1 (scene-only) skips it.
    if (u.render_mode < 0.5 || u.render_mode > 1.5) {
        if u.cursor_pos.x > -99.0 {
            let cursor_canvas_size = get_cursor_canvas_size(u.cursor_type_id);
            let cursor_pixel_size = cursor_canvas_size * u.cursor_scale;
            let cursor_px =
                (u.video_offset + (u.cursor_pos * u.video_scale)) * u.output_size +
                (get_cursor_alignment_bias() * u.cursor_scale);
            let hotspot = get_hotspot(cursor_pixel_size);
            let pivot = get_rotation_pivot(u.cursor_type_id);
            let sample_pos = cursor_sample_pos(in.pixel_pos, cursor_px, hotspot, pivot);

            let tile_idx = floor(u.cursor_type_id + 0.5);
            let in_bounds =
                sample_pos.x >= 0.0 && sample_pos.x < cursor_pixel_size.x &&
                sample_pos.y >= 0.0 && sample_pos.y < cursor_pixel_size.y;

            if (u.render_mode > 1.5) {
                // Cursor-only mode: output straight-alpha cursor so ALPHA_BLENDING composites
                // correctly over the already-rendered scene in the framebuffer (clear=false).
                if in_bounds {
                    let cur_col = sample_cursor_color(sample_pos, u.cursor_type_id, tile_idx, u.cursor_scale);
                    col = vec4<f32>(cur_col.rgb, cur_col.a * u.cursor_opacity);
                }
            } else {
                // Normal mode: the preview draws the cursor over its shadow on an
                // offscreen canvas, then composites that layer at the cursor
                // opacity as one group, in encoded sRGB.
                var shadow_alpha = 0.0;
                let shadow_strength = clamp(u.cursor_shadow, 0.0, 2.0);
                if shadow_strength > 0.001 {
                    let base = pow(min(shadow_strength, 1.0), 0.8);
                    let overdrive = max(0.0, shadow_strength - 1.0);
                    let shadow_alpha_gain = min(1.0, (0.95 * base) + (0.85 * overdrive));
                    // Canvas shadow offsets are screen pixels that the cursor's
                    // rotation does not turn, and Chromium snaps them to whole
                    // pixels (round half up).
                    let shadow_offset = floor(vec2<f32>(
                        (1.3 * base) + (1.7 * overdrive),
                        (2.6 * base) + (3.2 * overdrive)
                    ) + vec2<f32>(0.5));
                    let shadow_blur = 1.6 + (11.5 * base) + (14.0 * overdrive);
                    let shadow_pos =
                        cursor_sample_pos(in.pixel_pos - shadow_offset, cursor_px, hotspot, pivot);
                    shadow_alpha = shadow_alpha_gain * cursor_shadow_coverage(
                        shadow_pos,
                        cursor_pixel_size,
                        0.5 * shadow_blur,
                        u.cursor_type_id,
                        tile_idx,
                        u.cursor_scale
                    );
                }

                var cur_col = vec4<f32>(0.0);
                if in_bounds {
                    cur_col = sample_cursor_color(sample_pos, u.cursor_type_id, tile_idx, u.cursor_scale);
                }
                let layer_alpha = cur_col.a + shadow_alpha * (1.0 - cur_col.a);
                let layer_mix = layer_alpha * u.cursor_opacity;
                if layer_mix > 0.0001 {
                    // The shadow is black, so only the cursor adds color.
                    let layer_rgb = srgb_encode(cur_col.rgb) * (cur_col.a / layer_alpha);
                    col = vec4<f32>(
                        srgb_decode(mix(srgb_encode(col.rgb), layer_rgb, layer_mix)),
                        col.a + layer_mix * (1.0 - col.a)
                    );
                }
            }
        }
    }

    return col;
}
"#;

pub(super) fn compositor_shader() -> String {
    format!(
        "const ATLAS_COLS: f32 = {}.0;\nconst ATLAS_ROWS: f32 = {}.0;\n\
         const CURSOR_TILE_PX: f32 = {}.0;\nconst CURSOR_MAX_LOD: f32 = {}.0;\n{}{}{}{}",
        CURSOR_ATLAS_COLS,
        CURSOR_ATLAS_ROWS,
        CURSOR_TILE_SIZE,
        CURSOR_MIP_LEVELS - 1,
        COMPOSITOR_SHADER_HEAD,
        COMPOSITOR_SHADER_CURSOR_FUNCTIONS,
        COMPOSITOR_SHADER_BACKGROUND_FUNCTIONS,
        COMPOSITOR_SHADER_TAIL
    )
}

pub(super) const OVERLAY_SHADER_BODY: &str = r#"
struct OverlayVertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@group(0) @binding(0) var atlas_tex: texture_2d<f32>;
@group(0) @binding(1) var atlas_samp: sampler;

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) alpha: f32,
) -> OverlayVertexOut {
    var out: OverlayVertexOut;
    out.clip_pos = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    out.alpha = alpha;
    return out;
}

@fragment
fn fs_main(in: OverlayVertexOut) -> @location(0) vec4<f32> {
    let col = textureSample(atlas_tex, atlas_samp, in.uv);
    // Canvas2D toDataURL produces straight-alpha. Premultiply RGB by alpha so
    // drop-shadows and text AA composite correctly with (One, OneMinusSrcAlpha).
    return vec4<f32>(col.rgb * col.a * in.alpha, col.a * in.alpha);
}
"#;

pub(super) fn overlay_shader() -> &'static str {
    OVERLAY_SHADER_BODY
}

pub(super) const WEBCAM_SHADER_BODY: &str = r#"
struct WebcamUniforms {
    output_size: vec2<f32>,
    origin: vec2<f32>,
    size: vec2<f32>,
    roundness_px: f32,
    shadow_px: f32,
    opacity: f32,
    mirror: f32,
    render_scale: f32,
    _pad: f32,
}

struct WebcamVertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
}

@group(0) @binding(0) var webcam_tex: texture_2d<f32>;
@group(0) @binding(1) var webcam_samp: sampler;
@group(1) @binding(0) var<uniform> u: WebcamUniforms;

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> WebcamVertexOut {
    var out: WebcamVertexOut;
    out.clip_pos = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    out.pixel_pos = uv * u.output_size;
    return out;
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: WebcamVertexOut) -> @location(0) vec4<f32> {
    let half_size = u.size * 0.5;
    if (half_size.x <= 0.0 || half_size.y <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let radius = min(max(u.roundness_px, 0.0), min(half_size.x, half_size.y));
    let center = u.origin + half_size;
    let fill_dist = sd_round_box(in.pixel_pos - center, half_size, radius);
    let fill_alpha = clamp(1.0 - smoothstep(0.0, 1.35, fill_dist), 0.0, 1.0);

    // The 2 px floor is a preview-canvas length, like `shadow_px` itself.
    let shadow_offset_y = max(2.0 * u.render_scale, u.shadow_px * 0.18);
    let shadow_dist = sd_round_box(
        in.pixel_pos - (center + vec2<f32>(0.0, shadow_offset_y)),
        half_size,
        radius,
    );
    let shadow_extent = max(u.shadow_px * 1.65, 1.0);
    let shadow_alpha = (1.0 - smoothstep(0.0, shadow_extent, shadow_dist)) * 0.32;
    let visible_shadow_alpha = shadow_alpha * (1.0 - fill_alpha) * u.opacity;

    var composed_rgb = vec3<f32>(8.0 / 255.0, 10.0 / 255.0, 20.0 / 255.0) * visible_shadow_alpha;
    var composed_alpha = visible_shadow_alpha;

    if (fill_alpha > 0.0001) {
        var webcam_uv = (in.pixel_pos - u.origin) / max(u.size, vec2<f32>(1.0, 1.0));
        webcam_uv = clamp(webcam_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        if (u.mirror > 0.5) {
            webcam_uv.x = 1.0 - webcam_uv.x;
        }
        let webcam_col = textureSample(webcam_tex, webcam_samp, webcam_uv).rgb;
        let visible_fill_alpha = fill_alpha * u.opacity;
        composed_rgb = composed_rgb + (webcam_col * visible_fill_alpha);
        composed_alpha = composed_alpha + visible_fill_alpha;
    }

    if (composed_alpha <= 0.0001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    return vec4<f32>(composed_rgb / composed_alpha, composed_alpha);
}
"#;

pub(super) fn webcam_shader() -> &'static str {
    WEBCAM_SHADER_BODY
}
