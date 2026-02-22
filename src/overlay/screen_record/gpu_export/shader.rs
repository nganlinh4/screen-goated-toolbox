use super::cursors::{CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS};

const COMPOSITOR_SHADER_BODY: &str = r#"
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
    time: f32,
    _pad1: f32,
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
    _pad3_0: f32,
    _pad3_1: f32,
    _pad3_2: f32,
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

// Hotspot offset function
fn get_hotspot(type_id: f32, size: f32) -> vec2<f32> {
    // All cursor types have hotspot at center of the 512x512 tile
    // To align hotspot with cursor_pos, offset by half the rendered size
    return vec2<f32>(size * 0.5, size * 0.5);
}

fn get_rotation_pivot(type_id: f32, size: f32) -> vec2<f32> {
    let unit = size / 48.0;
    let slot = i32(floor(type_id + 0.5));
    let kind = slot % 12;
    if kind == 2 || kind == 3 || kind == 4 {
        // pointer/openhand/closehand
        return vec2<f32>(3.0 * unit, 8.5 * unit);
    }
    if kind == 1 {
        // text i-beam should stay upright
        return vec2<f32>(0.0, 0.0);
    }
    // default arrow
    return vec2<f32>(3.6 * unit, 5.6 * unit);
}

fn gradient4_color(uv_raw: vec2<f32>, c1: vec4<f32>, c2: vec4<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let diag = clamp((uv.x * 0.68) + ((1.0 - uv.y) * 0.32), 0.0, 1.0);
    // Linearized #0353a4 midpoint (matches preview gradient stop).
    let mid = vec4<f32>(0.000911, 0.0865, 0.3712, 1.0);
    var base: vec4<f32>;
    if (diag < 0.55) {
        base = mix(c1, mid, diag / 0.55);
    } else {
        base = mix(mid, c2, (diag - 0.55) / 0.45);
    }

    let cool = vec3<f32>(0.03, 0.33, 0.67);
    let warm = vec3<f32>(0.98, 0.47, 0.09);
    let cool_glow = smoothstep(0.78, 0.05, distance(uv, vec2<f32>(0.18, 0.78)));
    let warm_glow = smoothstep(0.80, 0.08, distance(uv, vec2<f32>(0.86, 0.22)));

    let lit = base.rgb + (cool * cool_glow * 0.18) + (warm * warm_glow * 0.14);
    base = vec4<f32>(lit, 1.0);

    let vignette = smoothstep(0.20, 1.05, distance(uv, vec2<f32>(0.5, 0.5)));
    let shaded = mix(base.rgb, base.rgb * 0.82, vignette * 0.12);
    return vec4<f32>(clamp(shaded, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn hash12(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn gradient5_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>, c1: vec4<f32>, c2: vec4<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let diag = clamp((uv.x * 0.62) + ((1.0 - uv.y) * 0.38), 0.0, 1.0);
    // Linearized #4b4c99 midpoint.
    let mid = vec4<f32>(0.0704, 0.0723, 0.3185, 1.0);
    var base: vec4<f32>;
    if (diag < 0.52) {
        base = mix(c1, mid, diag / 0.52);
    } else {
        base = mix(mid, c2, (diag - 0.52) / 0.48);
    }

    let cool = vec3<f32>(0.14, 0.48, 0.62);
    let warm = vec3<f32>(0.93, 0.28, 0.44);
    let cool_glow = smoothstep(0.76, 0.10, distance(uv, vec2<f32>(0.22, 0.86)));
    let warm_glow = smoothstep(0.74, 0.10, distance(uv, vec2<f32>(0.82, 0.26)));
    let lit = base.rgb + (cool * cool_glow * 0.14) + (warm * warm_glow * 0.16);
    let shaded = mix(lit, lit * 0.84, smoothstep(0.24, 1.02, distance(uv, vec2<f32>(0.5, 0.5))) * 0.09);

    // Tiny deterministic dithering to hide 8-bit contour bands after encode.
    let noise = (hash12(pixel_pos) - 0.5) * (1.6 / 255.0);
    return vec4<f32>(clamp(shaded + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn gradient6_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>, c1: vec4<f32>, c2: vec4<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let diag = clamp((uv.x * 0.66) + ((1.0 - uv.y) * 0.34), 0.0, 1.0);
    // Linearized #ffe45e midpoint.
    let mid = vec4<f32>(1.0, 0.7758, 0.1119, 1.0);
    var base: vec4<f32>;
    if (diag < 0.50) {
        base = mix(c1, mid, diag / 0.50);
    } else {
        base = mix(mid, c2, (diag - 0.50) / 0.50);
    }

    let cool = vec3<f32>(0.00, 0.78, 0.98);
    let warm = vec3<f32>(1.00, 0.89, 0.37);
    let cool_glow = smoothstep(0.78, 0.10, distance(uv, vec2<f32>(0.20, 0.80)));
    let warm_glow = smoothstep(0.72, 0.08, distance(uv, vec2<f32>(0.78, 0.22)));
    let lit = base.rgb + (cool * cool_glow * 0.16) + (warm * warm_glow * 0.18);
    let shaded = mix(lit, lit * 0.88, smoothstep(0.26, 1.02, distance(uv, vec2<f32>(0.5, 0.5))) * 0.06);
    let noise = (hash12(pixel_pos) - 0.5) * (1.6 / 255.0);
    return vec4<f32>(clamp(shaded + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn gradient7_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>, c1: vec4<f32>, c2: vec4<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let diag = clamp((uv.x * 0.64) + ((1.0 - uv.y) * 0.36), 0.0, 1.0);
    // Linearized #8d7ae6 midpoint.
    let mid = vec4<f32>(0.2641, 0.1946, 0.7913, 1.0);
    var base: vec4<f32>;
    if (diag < 0.52) {
        base = mix(c1, mid, diag / 0.52);
    } else {
        base = mix(mid, c2, (diag - 0.52) / 0.48);
    }

    let cool = vec3<f32>(0.25, 0.60, 0.78);
    let warm = vec3<f32>(0.90, 0.58, 0.36);
    let cool_glow = smoothstep(0.78, 0.12, distance(uv, vec2<f32>(0.24, 0.78)));
    let warm_glow = smoothstep(0.76, 0.12, distance(uv, vec2<f32>(0.78, 0.26)));
    let lit = base.rgb + (cool * cool_glow * 0.10) + (warm * warm_glow * 0.10);
    let shaded = mix(lit, lit * 0.90, smoothstep(0.26, 1.02, distance(uv, vec2<f32>(0.5, 0.5))) * 0.08);
    let noise = (hash12(pixel_pos) - 0.5) * (1.2 / 255.0);
    return vec4<f32>(clamp(shaded + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let bg_zoom = max(u.bg_zoom, 0.0001);
    let bg_off = vec2<f32>(
        (1.0 - bg_zoom) * u.bg_anchor_x,
        (1.0 - bg_zoom) * u.bg_anchor_y
    );
    let bg_uv = (in.tex_coord - bg_off) / bg_zoom;

    // 1. Background
    let t = clamp(bg_uv.x, 0.0, 1.0);
    var col = mix(u.gradient_color1, u.gradient_color2, t);
    if (u.use_background_texture > 0.5) {
        col = textureSample(bg_tex, bg_samp, bg_uv);
    } else if (u._pad3_0 > 3.5) {
        col = gradient7_color(bg_uv, in.pixel_pos, u.gradient_color1, u.gradient_color2);
    } else if (u._pad3_0 > 2.5) {
        col = gradient6_color(bg_uv, in.pixel_pos, u.gradient_color1, u.gradient_color2);
    } else if (u._pad3_0 > 1.5) {
        col = gradient5_color(bg_uv, in.pixel_pos, u.gradient_color1, u.gradient_color2);
    } else if (u._pad3_0 > 0.5) {
        col = gradient4_color(bg_uv, u.gradient_color1, u.gradient_color2);
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

    // 4. Cursor Overlay (drawn over both video and background)
    if u.cursor_pos.x > -99.0 {
        let cursor_pixel_size = 48.0 * u.cursor_scale;
        let cursor_px = (u.video_offset + (u.cursor_pos * u.video_scale)) * u.output_size;
        let hotspot = get_hotspot(u.cursor_type_id, cursor_pixel_size);
        let pivot = get_rotation_pivot(u.cursor_type_id, cursor_pixel_size);
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
            sample_pos.x >= 0.0 && sample_pos.x < cursor_pixel_size &&
            sample_pos.y >= 0.0 && sample_pos.y < cursor_pixel_size;

        let shadow_strength = clamp(u.cursor_shadow, 0.0, 2.0);
        if shadow_strength > 0.001 {
            let base = pow(min(shadow_strength, 1.0), 0.8);
            let overdrive = max(0.0, shadow_strength - 1.0);
            let boosted = base + overdrive;
            let shadow_offset = vec2<f32>(
                (2.0 * (0.25 + 0.75 * base)) + (1.4 * overdrive),
                (2.8 * (0.25 + 0.75 * base)) + (2.2 * overdrive)
            );
            let shadow_pos = sample_pos - shadow_offset;
            let shadow_in_bounds =
                shadow_pos.x >= 0.0 && shadow_pos.x < cursor_pixel_size &&
                shadow_pos.y >= 0.0 && shadow_pos.y < cursor_pixel_size;

            if shadow_in_bounds {
                let blur = 1.0 + (3.5 * base) + (3.8 * overdrive);
                let diag = blur * 0.75;
                let offsets = array<vec2<f32>, 9>(
                    vec2<f32>(0.0, 0.0),
                    vec2<f32>(blur, 0.0),
                    vec2<f32>(-blur, 0.0),
                    vec2<f32>(0.0, blur),
                    vec2<f32>(0.0, -blur),
                    vec2<f32>(diag, diag),
                    vec2<f32>(-diag, diag),
                    vec2<f32>(diag, -diag),
                    vec2<f32>(-diag, -diag)
                );
                var shadow_alpha = 0.0;
                for (var i: i32 = 0; i < 9; i = i + 1) {
                    let p = shadow_pos + offsets[i];
                    if p.x >= 0.0 && p.x < cursor_pixel_size && p.y >= 0.0 && p.y < cursor_pixel_size {
                        let uv_in_tile = p / cursor_pixel_size;
                        let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
                        let atlas_row = floor(tile_idx / ATLAS_COLS);
                        let atlas_uv = vec2<f32>(
                            (uv_in_tile.x + atlas_col) / ATLAS_COLS,
                            (uv_in_tile.y + atlas_row) / ATLAS_ROWS
                        );
                        shadow_alpha = shadow_alpha + textureSample(cursor_tex, cursor_samp, atlas_uv).a;
                    }
                }
                shadow_alpha = (shadow_alpha / 9.0) * min(1.0, (0.95 * base) + (0.7 * overdrive)) * u.cursor_opacity;
                if shadow_alpha > 0.0001 {
                    let shadow_col = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
                    col = mix(col, shadow_col, shadow_col.a);
                }
            }
        }

        if in_bounds {
            let uv_in_tile = sample_pos / cursor_pixel_size;
            let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
            let atlas_row = floor(tile_idx / ATLAS_COLS);
            let atlas_uv = vec2<f32>(
                (uv_in_tile.x + atlas_col) / ATLAS_COLS,
                (uv_in_tile.y + atlas_row) / ATLAS_ROWS
            );
            let cur_col = textureSample(cursor_tex, cursor_samp, atlas_uv);
            let faded = vec4<f32>(cur_col.rgb, cur_col.a * u.cursor_opacity);
            col = mix(col, faded, faded.a);
        }
    }

    return col;
}
"#;

pub(super) fn compositor_shader() -> String {
    format!(
        "const ATLAS_COLS: f32 = {}.0;\nconst ATLAS_ROWS: f32 = {}.0;\n{}",
        CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS, COMPOSITOR_SHADER_BODY
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
    // Canvas2D toDataURL produces straight-alpha pixels; premultiply before blending.
    return vec4<f32>(col.rgb * in.alpha, col.a * in.alpha);
}
"#;

pub(super) fn overlay_shader() -> &'static str {
    OVERLAY_SHADER_BODY
}
