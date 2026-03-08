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

fn diagonal_glow_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let diag = clamp((uv.x * u.bg_params1.x) + ((1.0 - uv.y) * u.bg_params1.y), 0.0, 1.0);
    var base: vec4<f32>;
    if (diag < u.bg_params1.z) {
        base = mix(u.gradient_color1, u.gradient_color2, diag / max(u.bg_params1.z, 0.0001));
    } else {
        base = mix(
            u.gradient_color2,
            u.gradient_color3,
            (diag - u.bg_params1.z) / max(1.0 - u.bg_params1.z, 0.0001)
        );
    }

    let glow_a = smoothstep(
        u.bg_params2.z,
        u.bg_params2.w,
        distance(uv, vec2<f32>(u.bg_params2.x, u.bg_params2.y))
    ) * u.bg_params1.w;
    let glow_b = smoothstep(
        u.bg_params3.z,
        u.bg_params3.w,
        distance(uv, vec2<f32>(u.bg_params3.x, u.bg_params3.y))
    ) * u.bg_params4.x;

    let lit = base.rgb + (u.gradient_color4.rgb * glow_a) + (u.gradient_color5.rgb * glow_b);
    let shaded = mix(
        lit,
        lit * 0.82,
        smoothstep(u.bg_params4.y, u.bg_params4.z, distance(uv, vec2<f32>(0.5, 0.5))) * u.bg_params4.w
    );
    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params5.x / 255.0);
    return vec4<f32>(clamp(shaded + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn hash12(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn gradient8_ribbon(
    point: vec2<f32>,
    start: vec2<f32>,
    end: vec2<f32>,
    width: f32,
    curve_amp: f32,
    curve_freq: f32,
    intensity: f32
) -> vec2<f32> {
    let seg = end - start;
    let seg_len_sq = max(dot(seg, seg), 1e-6);
    let t = clamp(dot(point - start, seg) / seg_len_sq, 0.0, 1.0);
    let seg_len = sqrt(seg_len_sq);
    let normal = vec2<f32>(-seg.y, seg.x) / seg_len;
    let curve = sin(t * 3.14159265 * curve_freq) * curve_amp;
    let curve_point = start + (seg * t) + (normal * curve);
    let distance_to_curve = distance(point, curve_point);
    let edge_fade = smoothstep(0.01, 0.14, t) * (1.0 - smoothstep(0.84, 0.99, t));
    let band = (1.0 - smoothstep(width * 0.55, width * 2.25, distance_to_curve)) * edge_fade * intensity;
    let core = (1.0 - smoothstep(width * 0.10, width * 0.72, distance_to_curve)) * edge_fade * intensity;
    return vec2<f32>(band, core);
}

fn edge_ribbons_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let ribbon_a = gradient8_ribbon(
        point,
        vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y),
        vec2<f32>(u.bg_params1.z * aspect, u.bg_params1.w),
        u.bg_params2.x,
        u.bg_params2.y,
        u.bg_params2.z,
        u.bg_params2.w
    );
    let ribbon_b = gradient8_ribbon(
        point,
        vec2<f32>(u.bg_params3.x * aspect, u.bg_params3.y),
        vec2<f32>(u.bg_params3.z * aspect, u.bg_params3.w),
        u.bg_params4.x,
        u.bg_params4.y,
        u.bg_params4.z,
        u.bg_params4.w
    );

    let depth_mix = clamp((uv.y * 0.86) + ((1.0 - uv.x) * 0.14), 0.0, 1.0);
    var lit = mix(u.gradient_color1.rgb, u.gradient_color2.rgb, depth_mix);
    lit += (u.gradient_color3.rgb * ribbon_a.x) + (u.gradient_color4.rgb * ribbon_b.x);

    let core_glow = (ribbon_a.y * 0.42) + (ribbon_b.y * 0.28);
    lit += u.gradient_color5.rgb * core_glow;

    let glow_center = vec2<f32>(u.bg_params5.x * aspect, u.bg_params5.y);
    let glow_distance = distance(point, glow_center);
    let glow_strength = (1.0 - smoothstep(0.0, u.bg_params5.z, glow_distance)) * u.bg_params5.w;
    lit += u.gradient_color5.rgb * glow_strength;

    let vignette = smoothstep(u.bg_params6.x, u.bg_params6.y, distance(vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5), vec2<f32>(0.0, 0.0))) * u.bg_params6.z;
    lit = mix(lit, lit * 0.82, vignette);

    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params6.w / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn prism_fold_signed_distance(point: vec2<f32>, line: vec4<f32>) -> f32 {
    let p0 = vec2<f32>(line.x, line.y);
    let p1 = vec2<f32>(line.z, line.w);
    let dir = p1 - p0;
    let inv_len = 1.0 / max(length(dir), 0.0001);
    return dot(point - p0, vec2<f32>(-dir.y, dir.x)) * inv_len;
}

fn prism_fold_mask(
    point: vec2<f32>,
    line: vec4<f32>,
    reference: vec2<f32>,
    softness: f32
) -> vec2<f32> {
    let signed_distance = prism_fold_signed_distance(point, line);
    let reference_side = select(-1.0, 1.0, prism_fold_signed_distance(reference, line) >= 0.0);
    let inside = signed_distance * reference_side;
    let mask = smoothstep(-softness * 1.2, softness * 3.2, inside);
    let body = smoothstep(softness * 1.4, softness * 7.5, inside);
    let glow = body * (1.0 - smoothstep(softness * 7.5, softness * 15.0, inside));
    return vec2<f32>(mask, glow);
}

fn prism_fold_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let softness = max(u.bg_params5.w, 0.0001);

    let pane_a = prism_fold_mask(
        point,
        vec4<f32>(u.bg_params1.x * aspect, u.bg_params1.y, u.bg_params1.z * aspect, u.bg_params1.w),
        vec2<f32>(0.02 * aspect, 0.02),
        softness
    );
    let pane_b = prism_fold_mask(
        point,
        vec4<f32>(u.bg_params2.x * aspect, u.bg_params2.y, u.bg_params2.z * aspect, u.bg_params2.w),
        vec2<f32>(0.98 * aspect, 0.02),
        softness
    );
    let pane_c = prism_fold_mask(
        point,
        vec4<f32>(u.bg_params3.x * aspect, u.bg_params3.y, u.bg_params3.z * aspect, u.bg_params3.w),
        vec2<f32>(0.98 * aspect, 0.48),
        softness
    );
    let pane_d = prism_fold_mask(
        point,
        vec4<f32>(u.bg_params4.x * aspect, u.bg_params4.y, u.bg_params4.z * aspect, u.bg_params4.w),
        vec2<f32>(0.38 * aspect, 0.98),
        softness
    );

    let pane_a_mask = pane_a.x * 1.0;
    let pane_b_mask = pane_b.x * 0.92;
    let pane_c_mask = pane_c.x * 0.84;
    let pane_d_mask = pane_d.x * 0.96;
    let ambient = clamp(((1.0 - uv.x) * 0.52) + ((1.0 - uv.y) * 0.48), 0.0, 1.0);
    var lit = u.gradient_color1.rgb * mix(0.84, 1.12, ambient);

    lit += u.gradient_color2.rgb * ((pane_a_mask * u.bg_params5.x) + (pane_a.y * u.bg_params5.y));
    lit += u.gradient_color3.rgb * ((pane_b_mask * u.bg_params5.x) + (pane_b.y * u.bg_params5.y * 0.92));
    lit += u.gradient_color4.rgb * ((pane_c_mask * u.bg_params5.x) + (pane_c.y * u.bg_params5.y * 0.84));
    lit += u.gradient_color5.rgb * ((pane_d_mask * u.bg_params5.x) + (pane_d.y * u.bg_params5.y * 0.96));

    let pane_accum =
        (u.gradient_color2.rgb * pane_a_mask) +
        (u.gradient_color3.rgb * pane_b_mask) +
        (u.gradient_color4.rgb * pane_c_mask) +
        (u.gradient_color5.rgb * pane_d_mask);
    let pane_mask_sum = pane_a_mask + pane_b_mask + pane_c_mask + pane_d_mask;
    let overlap = max(pane_mask_sum - 1.0, 0.0) * u.bg_params5.z;
    if (overlap > 0.0001) {
        let avg = pane_accum / max(pane_mask_sum, 0.0001);
        lit += mix(avg, vec3<f32>(1.0, 1.0, 1.0), 0.35) * overlap;
    }

    let vignette =
        smoothstep(u.bg_params6.x, u.bg_params6.y, distance(vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5), vec2<f32>(0.0, 0.0))) *
        u.bg_params6.z;
    lit = mix(lit, lit * 0.82, vignette);

    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params6.w / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn topographic_flow_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let centered = vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let source_a = vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y);
    let source_b = vec2<f32>(u.bg_params1.z * aspect, u.bg_params1.w);
    let dist_a = distance(point, source_a);
    let dist_b = distance(point, source_b);
    let warp =
        (sin(((point.x * 0.82) + (point.y * 1.14)) * 6.2831853 * u.bg_params2.y) * u.bg_params2.z) +
        (sin(((point.x * -0.58) + (point.y * 0.92)) * 6.2831853 * u.bg_params2.y * 0.72) * u.bg_params2.z * 0.6);
    let field = ((dist_a * 0.92) + (dist_b * 0.78) + warp) * u.bg_params2.x;
    let line = 1.0 - smoothstep(u.bg_params2.w, u.bg_params2.w + 0.22, abs(sin(field * 3.14159265)));
    let glow = 1.0 - smoothstep(
        u.bg_params2.w * 2.6,
        (u.bg_params2.w * 2.6) + 0.24,
        abs(sin((field + 0.32) * 3.14159265))
    );
    let edge_bias = mix(
        u.bg_params3.z,
        1.0,
        smoothstep(0.18, 0.84, length(centered))
    );
    let phase_mix = clamp((sin((dist_a - dist_b) * 4.6) * 0.5) + 0.5, 0.0, 1.0);
    let line_color = mix(u.gradient_color2.rgb, u.gradient_color3.rgb, phase_mix);
    var lit = u.gradient_color1.rgb;
    lit += line_color * line * u.bg_params3.x * edge_bias;
    lit += u.gradient_color4.rgb * glow * u.bg_params3.y * edge_bias;
    let vignette = smoothstep(u.bg_params4.x, u.bg_params4.y, length(centered)) * u.bg_params4.z;
    lit = mix(lit, u.gradient_color5.rgb, vignette);
    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params4.w / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn stacked_radial_color(uv_raw: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let axis_t = select(uv.y, uv.x, u.bg_params1.x > 0.5);
    var base: vec3<f32>;
    if (axis_t < 0.5) {
        base = mix(u.gradient_color1.rgb, u.gradient_color2.rgb, axis_t / 0.5);
    } else {
        base = mix(u.gradient_color2.rgb, u.gradient_color3.rgb, (axis_t - 0.5) / 0.5);
    }

    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let center = vec2<f32>(u.bg_params2.x, u.bg_params2.y);
    let radial_distance = length(vec2<f32>(uv.x - center.x, (uv.y - center.y) / aspect));
    let overlay_strength = (1.0 - smoothstep(0.0, u.bg_params2.z, radial_distance)) * u.bg_params2.w;
    let shaded = mix(base, u.gradient_color4.rgb, overlay_strength);
    return vec4<f32>(clamp(shaded, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

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
    // Canvas2D toDataURL produces straight-alpha. Premultiply RGB by alpha so
    // drop-shadows and text AA composite correctly with (One, OneMinusSrcAlpha).
    return vec4<f32>(col.rgb * col.a * in.alpha, col.a * in.alpha);
}
"#;

pub(super) fn overlay_shader() -> &'static str {
    OVERLAY_SHADER_BODY
}
