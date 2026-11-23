//! Cursor sprite sampling and the Canvas-matched cursor drop shadow.

pub(super) const COMPOSITOR_SHADER_CURSOR_FUNCTIONS: &str = r#"
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

fn cursor_atlas_uv(sample_pos: vec2<f32>, type_id: f32, tile_idx: f32, cursor_scale: f32) -> vec2<f32> {
    let uv_in_tile = cursor_uv_in_tile(sample_pos, type_id, cursor_scale);
    let atlas_col = tile_idx - floor(tile_idx / ATLAS_COLS) * ATLAS_COLS;
    let atlas_row = floor(tile_idx / ATLAS_COLS);
    return vec2<f32>(
        (uv_in_tile.x + atlas_col) / ATLAS_COLS,
        (uv_in_tile.y + atlas_row) / ATLAS_ROWS
    );
}

fn sample_cursor_color(sample_pos: vec2<f32>, type_id: f32, tile_idx: f32, cursor_scale: f32) -> vec4<f32> {
    let atlas_uv = cursor_atlas_uv(sample_pos, type_id, tile_idx, cursor_scale);
    // Cursor tiles are uploaded from mixed sources (tiny-skia + browser PNG decode).
    // Normalize sampled color to straight alpha to avoid dark fringe/contour artifacts
    // at anti-aliased edges when linearly filtering against transparent pixels.
    var c = textureSampleLevel(cursor_tex, cursor_samp, atlas_uv, 0.0);
    if (c.a > 0.0001) {
        c = vec4<f32>(clamp(c.rgb / c.a, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else {
        c = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return c;
}

// Maps a screen pixel into the unrotated cursor sprite (0..size).
fn cursor_sample_pos(screen_pos: vec2<f32>, cursor_px: vec2<f32>, hotspot: vec2<f32>, pivot: vec2<f32>) -> vec2<f32> {
    let rel_pivot = screen_pos - cursor_px - pivot;
    let c = cos(-u.cursor_rotation);
    let s = sin(-u.cursor_rotation);
    return vec2<f32>(
        rel_pivot.x * c - rel_pivot.y * s,
        rel_pivot.x * s + rel_pivot.y * c
    ) + pivot + hotspot;
}

// Canvas shadowBlur B blurs the cursor alpha with a Gaussian of sigma B / 2
// (Edge measures 2.06, 4.10, 6.72 and 13.9 for B = 4, 8, 13.1 and 27.1).
// Taps sit half a sigma apart out to +/-3 sigma. Each tap reads the mip level
// whose texel spans one step, so the sum integrates the mask instead of
// aliasing its edges into bands; past the coarsest level the step shrinks
// and the tap count grows instead.
fn cursor_shadow_coverage(center: vec2<f32>, size: vec2<f32>, sigma: f32, type_id: f32, tile_idx: f32, cursor_scale: f32) -> f32 {
    let reach = vec2<f32>(3.0 * sigma);
    if any(center < -reach) || any(center > size + reach) {
        return 0.0;
    }
    let canvas_size = get_cursor_canvas_size(type_id);
    let texels_per_px = CURSOR_TILE_PX / (max(canvas_size.x, canvas_size.y) * max(cursor_scale, 0.0001));
    let step = min(0.5 * sigma, exp2(CURSOR_MAX_LOD) / texels_per_px);
    let taps = i32(min(ceil(3.0 * sigma / step), 12.0));
    let lod = clamp(log2(max(step * texels_per_px, 1.0)), 0.0, CURSOR_MAX_LOD);
    let falloff = -0.5 * (step * step) / (sigma * sigma);
    var covered = 0.0;
    var total = 0.0;
    for (var oy: i32 = -taps; oy <= taps; oy = oy + 1) {
        for (var ox: i32 = -taps; ox <= taps; ox = ox + 1) {
            let w = exp(falloff * f32(ox * ox + oy * oy));
            total = total + w;
            let p = center + vec2<f32>(f32(ox), f32(oy)) * step;
            if all(p >= vec2<f32>(0.0)) && all(p < size) {
                let uv = cursor_atlas_uv(p, type_id, tile_idx, cursor_scale);
                covered = covered + textureSampleLevel(cursor_tex, cursor_samp, uv, lod).a * w;
            }
        }
    }
    return covered / total;
}
"#;
