use super::cursors::{CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS};
mod backgrounds;
use self::backgrounds::COMPOSITOR_SHADER_BACKGROUND_FUNCTIONS;

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

fn get_cursor_canvas_size(type_id: f32) -> vec2<f32> {
    let slot = i32(floor(type_id + 0.5));
    let kind = slot % 12;
    let pack = slot / 12;

    // Match preview image natural dimensions (drawCenteredCursorImage).
    if pack == 0 && kind == 1 {
        return vec2<f32>(43.0, 43.0); // text-screenstudio
    }
    if pack == 0 && (kind == 5 || kind == 6) {
        return vec2<f32>(56.0, 55.0); // wait/appstarting-screenstudio
    }
    if pack == 3 && kind == 5 {
        return vec2<f32>(44.0, 44.0); // wait-sgtcool
    }
    return vec2<f32>(44.0, 43.0);
}

fn get_hotspot(size: vec2<f32>) -> vec2<f32> {
    return size * 0.5;
}

fn get_rotation_pivot(type_id: f32) -> vec2<f32> {
    let slot = i32(floor(type_id + 0.5));
    let kind = slot % 12;
    if kind == 2 || kind == 3 || kind == 4 {
        // pointer/openhand/closehand
        return vec2<f32>(3.0, 8.5);
    }
    if kind == 1 {
        // text i-beam should stay upright
        return vec2<f32>(0.0, 0.0);
    }
    // default arrow
    return vec2<f32>(3.6, 5.6);
}

fn get_cursor_alignment_bias() -> vec2<f32> {
    // Canvas2D drawImage and WGSL texture sampling do not land on identical
    // sub-pixel centers. Apply one global source-space correction so export
    // cursor placement matches preview without per-cursor hacks.
    return vec2<f32>(0.16, -0.16);
}

fn cursor_uv_in_tile(sample_pos: vec2<f32>, type_id: f32, cursor_scale: f32) -> vec2<f32> {
    let canvas_size = get_cursor_canvas_size(type_id);
    let max_dim = max(canvas_size.x, canvas_size.y);
    let pad = (vec2<f32>(max_dim, max_dim) - canvas_size) * 0.5;
    let inv_scale = 1.0 / max(cursor_scale, 0.0001);
    let canvas_pos = sample_pos * inv_scale;
    return (canvas_pos + pad) / max_dim;
}

fn sample_cursor_color(sample_pos: vec2<f32>, type_id: f32, tile_idx: f32, cursor_scale: f32) -> vec4<f32> {
    let uv_in_tile = cursor_uv_in_tile(sample_pos, type_id, cursor_scale);
    let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
    let atlas_row = floor(tile_idx / ATLAS_COLS);
    let atlas_uv = vec2<f32>(
        (uv_in_tile.x + atlas_col) / ATLAS_COLS,
        (uv_in_tile.y + atlas_row) / ATLAS_ROWS
    );
    // Cursor tiles are uploaded from mixed sources (tiny-skia + browser PNG decode).
    // Normalize sampled color to straight alpha to avoid dark fringe/contour artifacts
    // at anti-aliased edges when linearly filtering against transparent pixels.
    var c = textureSample(cursor_tex, cursor_samp, atlas_uv);
    if (c.a > 0.0001) {
        c = vec4<f32>(clamp(c.rgb / c.a, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else {
        c = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return c;
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
        col = mix(u.gradient_color1, u.gradient_color2, t);
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

            // Anti-aliased video edge
            let edge = 1.0 - smoothstep(-1.5, 0.0, dist);
            col = mix(col, vid_col, edge);
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
            let rel = in.pixel_pos - cursor_px;
            let c = cos(-u.cursor_rotation);
            let s = sin(-u.cursor_rotation);
            let rel_pivot = rel - pivot;
            let rel_rot = vec2<f32>(
                rel_pivot.x * c - rel_pivot.y * s,
                rel_pivot.x * s + rel_pivot.y * c
            ) + pivot;
            let sample_pos = rel_rot + hotspot;

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
                // Normal mode (render_mode 0): shadow + cursor composited over scene.
                let shadow_strength = clamp(u.cursor_shadow, 0.0, 2.0);
                if shadow_strength > 0.001 {
                    let base = pow(min(shadow_strength, 1.0), 0.8);
                    let overdrive = max(0.0, shadow_strength - 1.0);
                    let shadow_alpha_gain = min(1.0, (0.95 * base) + (0.85 * overdrive));
                    let shadow_offset = vec2<f32>(
                        (1.3 * base) + (1.7 * overdrive),
                        (2.6 * base) + (3.2 * overdrive)
                    );
                    let shadow_pos = sample_pos - shadow_offset;
                    let shadow_in_bounds =
                        shadow_pos.x >= 0.0 && shadow_pos.x < cursor_pixel_size.x &&
                        shadow_pos.y >= 0.0 && shadow_pos.y < cursor_pixel_size.y;

                    if shadow_in_bounds {
                        let blur = 1.6 + (11.5 * base) + (14.0 * overdrive);
                        let sample_step = max(0.16, blur * 0.11);
                        var shadow_alpha = 0.0;
                        var shadow_weight = 0.0;

                        for (var oy: i32 = -5; oy <= 5; oy = oy + 1) {
                            for (var ox: i32 = -5; ox <= 5; ox = ox + 1) {
                                let o = vec2<f32>(f32(ox), f32(oy));
                                let r2 = dot(o, o);
                                let w = exp(-0.5 * r2 / 8.0);
                                let p = shadow_pos + o * sample_step;
                                if p.x >= 0.0 && p.x < cursor_pixel_size.x && p.y >= 0.0 && p.y < cursor_pixel_size.y {
                                    shadow_alpha = shadow_alpha + sample_cursor_color(p, u.cursor_type_id, tile_idx, u.cursor_scale).a * w;
                                    shadow_weight = shadow_weight + w;
                                }
                            }
                        }

                        if shadow_weight > 0.0001 {
                            shadow_alpha = (shadow_alpha / shadow_weight) * shadow_alpha_gain * u.cursor_opacity;
                            if shadow_alpha > 0.0001 {
                                let shadow_col = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
                                col = mix(col, shadow_col, shadow_col.a);
                            }
                        }
                    }
                }

                if in_bounds {
                    let cur_col = sample_cursor_color(sample_pos, u.cursor_type_id, tile_idx, u.cursor_scale);
                    let faded = vec4<f32>(cur_col.rgb, cur_col.a * u.cursor_opacity);
                    col = mix(col, faded, faded.a);
                }
            }
        }
    }

    return col;
}
"#;

pub(super) fn compositor_shader() -> String {
    format!(
        "const ATLAS_COLS: f32 = {}.0;\nconst ATLAS_ROWS: f32 = {}.0;\n{}{}{}",
        CURSOR_ATLAS_COLS,
        CURSOR_ATLAS_ROWS,
        COMPOSITOR_SHADER_HEAD,
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
