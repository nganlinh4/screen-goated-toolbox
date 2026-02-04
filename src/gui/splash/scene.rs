// --- SPLASH SCENE ENTITIES ---
// Data structures and initialization for splash screen scene elements.

use super::math::Vec3;
use super::{C_CYAN, C_DAY_REP, C_DAY_SEC, C_MAGENTA, C_SHADOW, C_WHITE};
use eframe::egui::Vec2;
use eframe::egui::Color32;
use std::f32::consts::PI;

// --- ATMOSPHERE ENTITIES ---

pub struct Cloud {
    pub pos: Vec2,
    pub velocity: f32,
    pub scale: f32,
    pub opacity: f32,
    pub puffs: Vec<(Vec2, f32)>, // (Offset from center, Radius multiplier)
}

pub struct Star {
    pub pos: Vec2, // 0.0-1.0 normalized screen coords
    pub phase: f32,
    pub brightness: f32,
    pub size: f32,
}

// --- MOON ENTITIES ---
pub struct MoonFeature {
    pub pos: Vec2, // Normalized on moon disk (-1.0 to 1.0)
    pub radius: f32,
    pub is_crater: bool, // if true, draws a depth ring; if false, draws a filled patch (Mare)
}

// --- VOXEL ENTITIES ---
pub struct Voxel {
    pub helix_radius: f32,
    pub helix_angle_offset: f32,
    pub helix_y: f32,
    pub target_pos: Vec3,
    pub pos: Vec3,
    pub rot: Vec3,
    pub scale: f32,
    pub velocity: Vec3,
    pub color: Color32,
    pub noise_factor: f32,
    pub is_debris: bool,
}

/// Initialize all scene entities (voxels, stars, clouds, moon features)
pub fn init_scene(
    voxels: &mut Vec<Voxel>,
    stars: &mut Vec<Star>,
    clouds: &mut Vec<Cloud>,
    moon_features: &mut Vec<MoonFeature>,
    is_dark: bool,
) {
    let mut rng_state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(987654321u64);

    let mut rng = || {
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (rng_state >> 32) as f32 / 4294967295.0
    };

    // --- 1. Init Text Voxels ---
    let s_map = [" ####", "##   ", " ### ", "   ##", "#### "];
    let g_map = [" ####", "##   ", "## ##", "##  #", " ####"];
    let t_map = ["#####", "  #  ", "  #  ", "  #  ", "  #  "];

    let spacing = 14.0;
    let mut total_voxels = 0;

    let mut spawn_letter = |map: &[&str], offset_x: f32, color_theme: Color32| {
        for (y, row) in map.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '#' {
                    total_voxels += 1;
                    let tx = offset_x + (x as f32 * spacing);
                    let ty = (2.0 - y as f32) * spacing;
                    let tz = 0.0;
                    let target = Vec3::new(tx, ty, tz);

                    let strand_idx = total_voxels % 2;
                    let h_y = ((total_voxels as f32 * 3.0) % 240.0) - 120.0;
                    let h_radius = 60.0;
                    let h_angle = (if strand_idx == 0 { 0.0 } else { PI }) + (h_y * 0.05);

                    voxels.push(Voxel {
                        helix_radius: h_radius,
                        helix_angle_offset: h_angle,
                        helix_y: h_y,
                        target_pos: target,
                        pos: Vec3::ZERO,
                        rot: Vec3::new(rng() * 6.0, rng() * 6.0, rng() * 6.0),
                        scale: 0.1,
                        velocity: Vec3::ZERO,
                        color: if rng() > 0.85 { C_WHITE } else { color_theme },
                        noise_factor: rng(),
                        is_debris: false,
                    });
                }
            }
        }
    };

    let c_primary = if is_dark { C_MAGENTA } else { C_DAY_REP };
    let c_secondary = if is_dark { C_CYAN } else { C_DAY_SEC };

    spawn_letter(&s_map, -120.0, c_secondary);
    spawn_letter(&g_map, -35.0, c_primary);
    spawn_letter(&t_map, 50.0, c_secondary);

    // Debris
    for _ in 0..100 {
        let h_y = (rng() * 300.0) - 150.0;
        let h_radius = 80.0 + rng() * 60.0;
        let h_angle = rng() * PI * 2.0;

        let n = rng();
        // Spread targets in a thick 3D torus/nebula
        let t_y = rng() * 700.0 - 50.0;
        // Diverse depth: Some very close, some very far
        let t_dist = 400.0 + n.powi(2) * 1400.0;
        let target = Vec3::new(h_angle.cos() * t_dist, t_y, h_angle.sin() * t_dist);

        voxels.push(Voxel {
            helix_radius: h_radius,
            helix_angle_offset: h_angle,
            helix_y: h_y,
            target_pos: target,
            pos: Vec3::ZERO,
            rot: Vec3::new(rng(), rng(), rng()),
            // Diverse sizes: small dust to larger puffs
            scale: 0.1 + n * 0.5,
            velocity: Vec3::ZERO,
            color: C_SHADOW,
            noise_factor: n,
            is_debris: true,
        });
    }

    // --- 2. Init Stars ---
    for _ in 0..150 {
        stars.push(Star {
            pos: Vec2::new(rng(), rng() * 0.85), // Keep mostly top/middle
            phase: rng() * PI * 2.0,
            brightness: 0.3 + rng() * 0.7,
            size: if rng() > 0.95 {
                1.5 + rng()
            } else {
                0.8 + rng() * 0.5
            },
        });
    }

    // --- 3. Init Dark Clouds (Volumetric Puffs) ---
    for _ in 0..15 {
        // Fewer total clouds, but more complex
        let mut puffs = Vec::new();
        // Core main puff
        puffs.push((Vec2::ZERO, 1.0));
        // Satellites
        let num_puffs = 5 + (rng() * 4.0) as usize;
        for _ in 0..num_puffs {
            let angle = rng() * PI * 2.0;
            let dist = 15.0 + rng() * 25.0;
            let r_mult = 0.4 + rng() * 0.5;
            puffs.push((
                Vec2::new(angle.cos() * dist, angle.sin() * dist * 0.6), // Squashed vertically
                r_mult,
            ));
        }

        clouds.push(Cloud {
            pos: Vec2::new(rng() * 1200.0 - 600.0, rng() * 400.0 - 200.0),
            velocity: 5.0 + rng() * 15.0, // Drifting right
            scale: 1.2 + rng() * 1.5,
            opacity: 0.4 + rng() * 0.4,
            puffs,
        });
    }

    // --- 4. Init Moon Features ---
    // Maria (Dark Patches - large, irregular)
    for _ in 0..20 {
        let angle = rng() * PI * 2.0;
        let dist = rng().sqrt() * 0.7; // Bias towards center/middle
        let pos = Vec2::new(angle.cos() * dist, angle.sin() * dist);

        moon_features.push(MoonFeature {
            pos,
            radius: 0.15 + rng() * 0.25,
            is_crater: false,
        });
    }

    // Craters (Small, sharp)
    for _ in 0..50 {
        let angle = rng() * PI * 2.0;
        let dist = rng().powf(0.8);
        let pos = Vec2::new(angle.cos() * dist, angle.sin() * dist);

        moon_features.push(MoonFeature {
            pos,
            radius: 0.02 + rng() * 0.06,
            is_crater: true,
        });
    }
}
