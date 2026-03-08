pub(super) const COMPOSITOR_SHADER_BACKGROUND_FUNCTIONS: &str = r#"
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

fn rotate2(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>((p.x * c) - (p.y * s), (p.x * s) + (p.y * c));
}

fn windowlight_beam(
    point: vec2<f32>,
    center: vec2<f32>,
    angle: f32,
    width: f32,
    length: f32,
    intensity: f32,
    caustic_freq: f32,
    caustic_warp: f32,
    caustic_strength: f32
) -> f32 {
    let rel = rotate2(point - center, -angle);
    let half_size = vec2<f32>(length * 0.5, width * 0.5);
    let radius = half_size.y * 0.82;
    let dist = sd_box(rel, half_size, radius);
    let mask = 1.0 - smoothstep(0.0, max(width * 0.9, 0.08), dist);
    let caustic_a =
        sin((rel.x * 3.14159265 * caustic_freq) + (sin(rel.y * 3.14159265 * caustic_freq * 0.78) * caustic_warp));
    let caustic_b = sin((rel.y * 3.14159265 * caustic_freq * 0.52) - (rel.x * 3.14159265 * 0.35) + 1.3);
    let striation = clamp(((caustic_a * 0.5 + 0.5) * 0.72) + ((caustic_b * 0.5 + 0.5) * 0.28), 0.0, 1.0);
    let edge_lift = 1.0 - smoothstep(half_size.y * 0.18, half_size.y * 1.35, abs(rel.y));
    let end_fade = 1.0 - smoothstep(half_size.x * 0.22, half_size.x * 1.04, abs(rel.x));
    let textured = mix(
        1.0 - caustic_strength,
        1.0 + (caustic_strength * 0.42),
        clamp((striation * 0.82) + (edge_lift * 0.18), 0.0, 1.0)
    );
    return mask * end_fade * textured * intensity;
}

fn windowlight_caustics_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let centered = vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);
    let edge_bias = mix(
        u.bg_params5.w,
        1.0,
        smoothstep(0.16, 0.84, length(centered))
    );
    let ambient = clamp(((1.0 - uv.y) * 0.62) + (uv.x * 0.18) + 0.12, 0.0, 1.0);
    var lit = u.gradient_color1.rgb * mix(0.92, 1.08, ambient);
    let beam_a = windowlight_beam(
        point,
        vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y),
        u.bg_params1.z,
        u.bg_params1.w,
        u.bg_params2.x,
        u.bg_params2.y,
        u.bg_params5.x,
        u.bg_params5.y,
        u.bg_params5.z
    ) * edge_bias;
    let beam_b = windowlight_beam(
        point,
        vec2<f32>(u.bg_params2.z * aspect, u.bg_params2.w),
        u.bg_params3.x,
        u.bg_params3.y,
        u.bg_params3.z,
        u.bg_params3.w,
        u.bg_params5.x * 1.12,
        u.bg_params5.y * 0.92,
        u.bg_params5.z * 0.85
    ) * edge_bias;
    let highlight =
        (1.0 - smoothstep(0.0, u.bg_params4.z, distance(point, vec2<f32>(u.bg_params4.x * aspect, u.bg_params4.y)))) *
        u.bg_params4.w *
        edge_bias;
    let beam_core_lift = (beam_a * 0.18) + (beam_b * 0.15);
    lit += u.gradient_color2.rgb * beam_a;
    lit += u.gradient_color3.rgb * beam_b;
    lit += u.gradient_color4.rgb * (highlight + beam_core_lift);
    let vignette = smoothstep(u.bg_params6.x, u.bg_params6.y, length(centered)) * u.bg_params6.z;
    lit = mix(lit, u.gradient_color5.rgb, vignette);
    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params6.w / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn matte_collage_rotate(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>((p.x * c) - (p.y * s), (p.x * s) + (p.y * c));
}

fn matte_collage_corner_radius(size: vec2<f32>) -> f32 {
    return max(min(size.y * 0.42, size.x * 0.18), 0.02);
}

fn matte_collage_layer(point: vec2<f32>, center: vec2<f32>, radii: vec2<f32>, angle: f32) -> vec2<f32> {
    let rel = matte_collage_rotate(point - center, -angle);
    let safe_radii = max(radii, vec2<f32>(0.0001, 0.0001));
    let corner_radius = matte_collage_corner_radius(safe_radii);
    let distance = sd_box(rel, safe_radii, corner_radius);
    let edge_softness = max(min(safe_radii.y * 0.2, 0.08), 0.028);
    let mask = 1.0 - smoothstep(0.0, edge_softness, distance);
    let light = clamp(0.6 + ((-rel.x / safe_radii.x) * 0.08) + ((-rel.y / safe_radii.y) * 0.24), 0.0, 1.0);
    let shade = mix(0.92, 1.05, light);
    return vec2<f32>(mask, shade);
}

fn matte_collage_shadow(point: vec2<f32>, center: vec2<f32>, radii: vec2<f32>, angle: f32, blur: f32) -> f32 {
    let rel = matte_collage_rotate(point - center, -angle);
    let safe_radii = max(radii, vec2<f32>(0.0001, 0.0001));
    let corner_radius = matte_collage_corner_radius(safe_radii);
    let distance = sd_box(rel, safe_radii, corner_radius);
    return 1.0 - smoothstep(0.0, max(blur, 0.0001), distance);
}

fn matte_collage_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let collage_aspect = min(mix(1.0, aspect, 0.24), 1.28);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let centered = vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);
    let ambient = clamp(((1.0 - uv.y) * 0.46) + ((1.0 - uv.x) * 0.18) + 0.26, 0.0, 1.0);
    var lit = u.gradient_color1.rgb * mix(0.92, 1.06, ambient);
    let layer_a_center = vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y);
    let layer_a_radii = vec2<f32>(u.bg_params1.z * collage_aspect, u.bg_params1.w);
    let layer_b_center = vec2<f32>(u.bg_params2.y * aspect, u.bg_params2.z);
    let layer_b_radii = vec2<f32>(u.bg_params2.w * collage_aspect, u.bg_params3.x);
    let layer_c_center = vec2<f32>(u.bg_params3.z * aspect, u.bg_params3.w);
    let layer_c_radii = vec2<f32>(u.bg_params4.x * collage_aspect, u.bg_params4.y);
    let shadow_offset = vec2<f32>(u.bg_params4.w * collage_aspect, u.bg_params5.x);
    let shadow_a = matte_collage_shadow(point, layer_a_center + shadow_offset, layer_a_radii, u.bg_params2.x, u.bg_params5.y);
    let shadow_b = matte_collage_shadow(point, layer_b_center + shadow_offset, layer_b_radii, u.bg_params3.y, u.bg_params5.y);
    let shadow_c = matte_collage_shadow(point, layer_c_center + shadow_offset, layer_c_radii, u.bg_params4.z, u.bg_params5.y);
    let shadow_mix = clamp((shadow_a + shadow_b + shadow_c) * u.bg_params5.z, 0.0, 1.0);
    lit = mix(lit, u.gradient_color5.rgb, shadow_mix);

    let layer_a = matte_collage_layer(point, layer_a_center, layer_a_radii, u.bg_params2.x);
    let layer_b = matte_collage_layer(point, layer_b_center, layer_b_radii, u.bg_params3.y);
    let layer_c = matte_collage_layer(point, layer_c_center, layer_c_radii, u.bg_params4.z);

    lit = mix(lit, u.gradient_color2.rgb * layer_a.y, layer_a.x);
    lit = mix(lit, u.gradient_color3.rgb * layer_b.y, layer_b.x);
    lit = mix(lit, u.gradient_color4.rgb * layer_c.y, layer_c.x);

    let vignette = smoothstep(u.bg_params6.x, u.bg_params6.y, length(centered)) * u.bg_params6.z;
    lit = mix(lit, lit * 0.86, vignette);
    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params5.w / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

fn orbital_arc_sample(
    point: vec2<f32>,
    center: vec2<f32>,
    radius: f32,
    thickness: f32,
    intensity: f32
) -> vec3<f32> {
    let ring_distance = abs(distance(point, center) - radius);
    let edge_softness = max(thickness * 0.62, 0.025);
    let field = (1.0 - smoothstep(thickness * 1.4, thickness * 6.8, ring_distance)) * intensity;
    let band = (1.0 - smoothstep(thickness, thickness + edge_softness, ring_distance)) * intensity;
    let core = (1.0 - smoothstep(thickness * 0.18, thickness * 0.72, ring_distance)) * intensity;
    return vec3<f32>(field, band, core);
}

fn orbital_arc_sweep_weight(point: vec2<f32>, center: vec2<f32>, canvas_center: vec2<f32>) -> f32 {
    let point_dir = point - center;
    let focus_dir = canvas_center - center;
    let point_len = max(length(point_dir), 0.00001);
    let focus_len = max(length(focus_dir), 0.00001);
    let facing = dot(point_dir / point_len, focus_dir / focus_len);
    return mix(0.3, 1.0, smoothstep(0.08, 0.96, (facing * 0.5) + 0.5));
}

fn orbital_arcs_color(uv_raw: vec2<f32>, pixel_pos: vec2<f32>) -> vec4<f32> {
    let uv = clamp(uv_raw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let aspect = max(u.output_size.x / max(u.output_size.y, 1.0), 0.0001);
    let point = vec2<f32>(uv.x * aspect, uv.y);
    let centered = vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);
    let canvas_center = vec2<f32>(aspect * 0.5, 0.52);
    let base_mix = clamp((uv.x * 0.68) + ((1.0 - uv.y) * 0.32), 0.0, 1.0);
    let ambient = clamp(((1.0 - uv.y) * 0.28) + ((1.0 - uv.x) * 0.08) + 0.2, 0.0, 1.0);
    let edge_bias = mix(
        u.bg_params5.x,
        1.0,
        smoothstep(0.16, 0.88, length(centered))
    );
    var lit = mix(u.gradient_color1.rgb, u.gradient_color2.rgb, base_mix) * mix(0.82, 0.96, ambient);

    let arc_a = orbital_arc_sample(
        point,
        vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y),
        u.bg_params1.z,
        u.bg_params1.w,
        u.bg_params2.x
    );
    let arc_b = orbital_arc_sample(
        point,
        vec2<f32>(u.bg_params2.y * aspect, u.bg_params2.z),
        u.bg_params2.w,
        u.bg_params3.x,
        u.bg_params3.y
    );
    let arc_c = orbital_arc_sample(
        point,
        vec2<f32>(u.bg_params3.z * aspect, u.bg_params3.w),
        u.bg_params4.x,
        u.bg_params4.y,
        u.bg_params4.z
    );
    let sweep_a = orbital_arc_sweep_weight(point, vec2<f32>(u.bg_params1.x * aspect, u.bg_params1.y), canvas_center);
    let sweep_b = orbital_arc_sweep_weight(point, vec2<f32>(u.bg_params2.y * aspect, u.bg_params2.z), canvas_center);
    let sweep_c = orbital_arc_sweep_weight(point, vec2<f32>(u.bg_params3.z * aspect, u.bg_params3.w), canvas_center);

    let arc_field_a = ((arc_a.x * 0.025) + (arc_a.y * 1.18)) * sweep_a;
    let arc_field_b = ((arc_b.x * 0.03) + (arc_b.y * 1.2)) * sweep_b;
    let arc_field_c = ((arc_c.x * 0.025) + (arc_c.y * 1.16)) * sweep_c;
    lit += (
        (u.gradient_color3.rgb * arc_field_a) +
        (u.gradient_color4.rgb * arc_field_b) +
        (u.gradient_color5.rgb * arc_field_c)
    ) * edge_bias;

    let glow_lift =
        ((arc_a.z * 0.22 * sweep_a) + (arc_b.z * 0.24 * sweep_b) + (arc_c.z * 0.2 * sweep_c) + ((arc_a.x + arc_b.x + arc_c.x) * 0.004)) *
        edge_bias;
    let overlap =
        max(((arc_a.y * sweep_a) + (arc_b.y * sweep_b) + (arc_c.y * sweep_c)) - 0.86, 0.0) *
        u.bg_params4.w *
        edge_bias;
    let glow_color = mix(
        (u.gradient_color3.rgb * 0.26) + (u.gradient_color4.rgb * 0.38) + (u.gradient_color5.rgb * 0.36),
        vec3<f32>(1.0, 1.0, 1.0),
        0.02
    );
    lit += glow_color * (glow_lift + overlap);

    let vignette = smoothstep(u.bg_params5.y, u.bg_params5.z, length(centered)) * u.bg_params5.w;
    lit = mix(lit, lit * 0.84, vignette);
    let noise = (hash12(pixel_pos) - 0.5) * (u.bg_params6.x / 255.0);
    return vec4<f32>(clamp(lit + vec3<f32>(noise), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;
