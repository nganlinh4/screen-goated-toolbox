use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, Vec2, FontId, Align2, Stroke};

// --- CONFIGURATION ---
const ANIMATION_DURATION: f32 = 7.0;
const FADE_OUT_START: f32 = 6.0;

// --- PALETTE (Cyber-Construct Theme) ---
const C_BG: Color32 = Color32::from_rgb(5, 8, 12);              // Deep Void
const C_PRIMARY: Color32 = Color32::from_rgb(0, 240, 255);      // Cyan Neon
const C_SECONDARY: Color32 = Color32::from_rgb(180, 50, 255);   // Violet Plasma
const C_CORE: Color32 = Color32::from_rgb(220, 250, 255);       // White Hot
const C_GRID: Color32 = Color32::from_rgb(20, 40, 60);          // Background Grid

// --- 3D MATH KERNEL ---
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 { x: f32, y: f32, z: f32 }

impl Vec3 {
    const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } }

    fn add(self, v: Vec3) -> Self { Self::new(self.x + v.x, self.y + v.y, self.z + v.z) }
    fn sub(self, v: Vec3) -> Self { Self::new(self.x - v.x, self.y - v.y, self.z - v.z) }
    fn mul(self, s: f32) -> Self { Self::new(self.x * s, self.y * s, self.z * s) }
    
    fn len(self) -> f32 { (self.x*self.x + self.y*self.y + self.z*self.z).sqrt() }

    fn rotate_y(self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }
    fn rotate_x(self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
    }
    fn rotate_z(self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c, self.z)
    }

    // Returns (ScreenPos, Scale, Z-Depth)
    fn project(self, center: Pos2, fov_scale: f32, cam_z: f32) -> Option<(Pos2, f32, f32)> {
        let z_depth = cam_z - self.z;
        if z_depth <= 1.0 { return None; } 
        let scale = fov_scale / z_depth;
        let x = center.x + self.x * scale;
        let y = center.y - self.y * scale; 
        Some((Pos2::new(x, y), scale, z_depth))
    }
}

// --- PARTICLE SYSTEM ---
#[derive(PartialEq, Clone, Copy)]
enum PType {
    Voxel,      // The SGT text
    Orbit,      // The swirling rings
    DataRain,   // Background Matrix-style rain
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    target: Vec3,
    
    ptype: PType,
    color: Color32,
    size: f32,
    
    // Animation properties
    drag: f32,
    spring: f32,
    phase: f32, 
}

pub struct SplashScreen {
    start_time: f64,
    particles: Vec<Particle>,
    connections: Vec<(usize, usize)>, // Indices of particles to draw lines between
    init_done: bool,
    
    // Interactive State
    mouse_influence: Vec2,
}

pub enum SplashStatus {
    Ongoing,
    Finished,
}

impl SplashScreen {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            start_time: ctx.input(|i| i.time),
            particles: Vec::with_capacity(2000),
            connections: Vec::new(),
            init_done: false,
            mouse_influence: Vec2::ZERO,
        }
    }

    fn init_scene(&mut self) {
        let mut rng_state = 12345u64;
        let mut rng = || {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (rng_state >> 32) as f32 / 4294967295.0
        };

        // --- 1. GENERATE "SGT" VOXELS (High Res) ---
        // We construct a grid but only spawn on "pixels"
        let s_grid = [
            "  XXXXX  ", " XX   XX ", "XX     X ", "XX       ", " XXXXX   ",
            "      XX ", "       XX", " X     XX", " XX   XX ", "  XXXXX  "
        ];
        let g_grid = [
            "  XXXXX  ", " XX   XX ", "XX       ", "XX       ", "XX  XXXX ",
            "XX     XX", "XX     XX", " XX   XX ", "  XXXXX  ", "         "
        ];
        let t_grid = [
            "XXXXXXXXX", "XXXXXXXXX", "   XXX   ", "   XXX   ", "   XXX   ",
            "   XXX   ", "   XXX   ", "   XXX   ", "   XXX   ", "   XXX   "
        ];

        let mut spawn_char = |grid: &[&str], x_offset: f32, particles: &mut Vec<Particle>, conns: &mut Vec<(usize, usize)>| {
            let _base_idx = particles.len();
            let mut grid_indices = Vec::new(); // Track positions to make connections
            
            for (row, line) in grid.iter().enumerate() {
                for (col, char) in line.chars().enumerate() {
                    if char == 'X' {
                        // Add depth (thickness)
                        for depth in 0..2 {
                            let tx = x_offset + (col as f32 * 1.2);
                            let ty = (row as f32 * -1.2) + 6.0;
                            let tz = (depth as f32 * 1.5) - 0.75; // Slight 3D depth

                            // Target Position
                            let target = Vec3::new(tx * 12.0, ty * 12.0, tz * 10.0);
                            
                            // Start Position (Exploded)
                            let start = Vec3::new(
                                (rng() - 0.5) * 3000.0,
                                (rng() - 0.5) * 2000.0,
                                (rng() - 0.5) * 3000.0 - 1000.0
                            );

                            particles.push(Particle {
                                pos: start,
                                vel: Vec3::ZERO,
                                target,
                                ptype: PType::Voxel,
                                color: C_PRIMARY,
                                size: 2.0 + rng() * 2.0,
                                drag: 0.92,       // High drag for stability
                                spring: 0.05,     // Snappy spring
                                phase: rng(),
                            });
                            grid_indices.push(particles.len() - 1);
                        }
                    }
                }
            }
            
            // Pre-calculate connections (nearest neighbors in the list)
            // This is a simplified "connect sequential points" logic for the visual effect
            for i in 0..grid_indices.len().saturating_sub(1) {
                if rng() > 0.6 { // Randomly connect 40% of nodes to avoid clutter
                    conns.push((grid_indices[i], grid_indices[i+1]));
                }
            }
        };

        spawn_char(&s_grid, -18.0, &mut self.particles, &mut self.connections);
        spawn_char(&g_grid, -5.0, &mut self.particles, &mut self.connections);
        spawn_char(&t_grid, 8.0, &mut self.particles, &mut self.connections);

        // --- 2. ORBITAL RINGS (Lissajous) ---
        for i in 0..400 {
            let t = i as f32 * 0.1;
            let r = 250.0;
            // Lissajous curve target
            let tx = r * (t * 1.0).sin();
            let ty = r * (t * 2.0).cos(); 
            let tz = r * (t * 3.0).sin() * 0.5;
            
            let target = Vec3::new(tx, ty, tz);
            
            self.particles.push(Particle {
                pos: target.mul(10.0), // Start far out
                vel: Vec3::ZERO,
                target,
                ptype: PType::Orbit,
                color: C_SECONDARY,
                size: 1.5,
                drag: 0.95,
                spring: 0.02,
                phase: t,
            });
        }

        // --- 3. DATA RAIN (Background) ---
        for _ in 0..500 {
            let x = (rng() - 0.5) * 3000.0;
            let y = (rng() - 0.5) * 2000.0;
            let z = (rng() - 0.5) * 2000.0;
            
            self.particles.push(Particle {
                pos: Vec3::new(x, y, z),
                vel: Vec3::new(0.0, -50.0 - rng()*100.0, 0.0), // Falling down
                target: Vec3::ZERO, // Irrelevant for rain
                ptype: PType::DataRain,
                color: C_GRID.linear_multiply(0.3),
                size: 1.0 + rng(),
                drag: 1.0,
                spring: 0.0,
                phase: rng(),
            });
        }

        self.init_done = true;
    }

    pub fn update(&mut self, ctx: &egui::Context) -> SplashStatus {
        if !self.init_done { self.init_scene(); }

        let now = ctx.input(|i| i.time);
        let t = (now - self.start_time) as f32;
        
        if t > ANIMATION_DURATION {
            return SplashStatus::Finished;
        }
        ctx.request_repaint();

        // Interactive Parallax
        if let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) {
            let screen_rect = ctx.input(|i| i.screen_rect());
            let center = screen_rect.center();
            let target_x = (pointer.x - center.x) / center.x;
            let target_y = (pointer.y - center.y) / center.y;
            // Smoothly interpolate mouse influence
            self.mouse_influence.x += (target_x - self.mouse_influence.x) * 0.05;
            self.mouse_influence.y += (target_y - self.mouse_influence.y) * 0.05;
        }

        // Physics Update
        for p in &mut self.particles {
            match p.ptype {
                PType::Voxel => {
                    // SPRING PHYSICS: Force = (Target - Pos) * k - Vel * d
                    let diff = p.target.sub(p.pos);
                    let force = diff.mul(p.spring);
                    p.vel = p.vel.add(force).mul(p.drag);
                    p.pos = p.pos.add(p.vel);
                },
                PType::Orbit => {
                    // Dynamic spinning target
                    let spin_speed = 0.5 + (2.0 / (t + 1.0)); // Fast then slow
                    let angle = t * spin_speed + p.phase;
                    let r = 280.0;
                    
                    // Update target dynamically to create swirling orbit
                    let tx = r * angle.cos();
                    let ty = r * angle.sin();
                    let tz = (angle * 2.0).sin() * 100.0;
                    
                    let target = Vec3::new(tx, ty, tz);
                    let diff = target.sub(p.pos);
                    p.vel = p.vel.add(diff.mul(p.spring)).mul(p.drag);
                    p.pos = p.pos.add(p.vel);
                },
                PType::DataRain => {
                    // Constant falling
                    p.pos = p.pos.add(p.vel);
                    // Wrap around Y axis
                    if p.pos.y < -1000.0 { p.pos.y = 1000.0; }
                }
            }
        }
        
        // Render
        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint(ui, t);
        });

        SplashStatus::Ongoing
    }

    fn paint(&self, ui: &mut egui::Ui, t: f32) {
        let painter = ui.painter();
        let rect = ui.max_rect();
        let center = rect.center();
        
        // Calculate Camera
        // Dolly Zoom: As we get closer (cam_dist decreases), FOV increases slightly
        let zoom_progress = (t / 3.0).clamp(0.0, 1.0);
        let cam_dist = 1800.0 - (zoom_progress * 1000.0); // 1800 -> 800
        let fov = 800.0 + (zoom_progress * 200.0);
        
        // Camera Rotation (Mouse + Auto)
        let cam_rot = Vec3::new(
            self.mouse_influence.y * 0.2,
            self.mouse_influence.x * 0.2 + (t * 0.1).sin() * 0.05, // Subtle drift
            (t * 0.5).cos() * 0.02
        );

        // 1. Background & Vignette
        let alpha_mult = if t > FADE_OUT_START {
            1.0 - ((t - FADE_OUT_START) / (ANIMATION_DURATION - FADE_OUT_START)).clamp(0.0, 1.0)
        } else { 1.0 };

        painter.rect_filled(rect, 15.0, Color32::from_black_alpha((255.0 * alpha_mult) as u8));
        let bg_col = C_BG.linear_multiply(alpha_mult);
        painter.rect_filled(rect, 15.0, bg_col);

        // 2. Project Particles
        let mut draw_list: Vec<(f32, Pos2, f32, Color32, &Particle, usize)> = Vec::with_capacity(self.particles.len());

        for (idx, p) in self.particles.iter().enumerate() {
            // Rotate world
            let view_pos = p.pos.rotate_y(cam_rot.y).rotate_x(cam_rot.x).rotate_z(cam_rot.z);
            
            if let Some((screen_pos, scale, z)) = view_pos.project(center, fov, cam_dist) {
                // Calculate Brightness based on speed (hot when moving fast) and depth
                let speed = p.vel.len();
                let brightness = (0.5 + (speed * 0.05)).clamp(0.5, 1.5);
                let mut col = p.color.linear_multiply(brightness);
                
                // Depth fog
                let fog = (1.0 - (z / 3000.0)).clamp(0.0, 1.0);
                col = col.linear_multiply(fog * alpha_mult);

                draw_list.push((z, screen_pos, scale, col, p, idx));
            }
        }
        
        // Sort by Z for proper occlusion
        draw_list.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // 3. Draw Connections (The Neural Web) - Only if particles are locked in (t > 2.0)
        if t > 1.5 && alpha_mult > 0.1 {
            // Create a map for screen positions for O(1) lookup
            let mut screen_map = std::collections::HashMap::new();
            for (_, pos, _, _, _, idx) in &draw_list {
                screen_map.insert(*idx, *pos);
            }

            let line_alpha = (t - 1.5).clamp(0.0, 1.0) * 0.3 * alpha_mult;
            if line_alpha > 0.0 {
                let stroke = Stroke::new(1.0, C_PRIMARY.linear_multiply(line_alpha));
                for (a_idx, b_idx) in &self.connections {
                    if let (Some(pos_a), Some(pos_b)) = (screen_map.get(a_idx), screen_map.get(b_idx)) {
                        painter.line_segment([*pos_a, *pos_b], stroke);
                    }
                }
            }
        }

        // 4. Draw Particles
        for (_, pos, scale, col, p, _) in draw_list {
            let size = p.size * scale;
            if size < 0.5 { continue; }

            match p.ptype {
                PType::Voxel => {
                    // BLOOM EFFECT: Draw faint large circle, then sharp small square
                    painter.circle_filled(pos, size * 2.5, col.linear_multiply(0.2));
                    painter.rect_filled(Rect::from_center_size(pos, Vec2::splat(size)), 1.0, C_CORE.linear_multiply(col.a() as f32 / 255.0));
                },
                PType::Orbit => {
                    painter.circle_filled(pos, size, col);
                },
                PType::DataRain => {
                    painter.line_segment([pos, pos + Vec2::new(0.0, size * 5.0)], Stroke::new(scale, col));
                }
            }
        }

        // 5. Post-Process: Scanlines
        if alpha_mult > 0.5 {
            let scan_alpha = 10u8;
            for i in (rect.top() as i32..rect.bottom() as i32).step_by(4) {
                painter.line_segment(
                    [Pos2::new(rect.left(), i as f32), Pos2::new(rect.right(), i as f32)],
                    Stroke::new(1.0, Color32::from_black_alpha(scan_alpha))
                );
            }
        }

        // 6. Text
        if t > 2.5 {
            let opacity = ((t - 2.5) * 2.0).clamp(0.0, 1.0) * alpha_mult;
            if opacity > 0.05 {
                // Glitch offset
                let glitch = if rng_bool(0.05) { 5.0 } else { 0.0 };
                
                painter.text(
                    center + Vec2::new(glitch, 160.0),
                    Align2::CENTER_TOP,
                    "Screen Grounded Translator",
                    FontId::proportional(18.0),
                    Color32::from_rgb(180, 200, 220).linear_multiply(opacity)
                );
                
                painter.text(
                    center + Vec2::new(-glitch, 185.0),
                    Align2::CENTER_TOP,
                    "nganlinh4",
                    FontId::monospace(10.0),
                    C_PRIMARY.linear_multiply(opacity * 0.7)
                );
            }
        }
    }
}

// Helper for quick randomness in paint loop
fn rng_bool(chance: f32) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    (nanos % 100) < (chance * 100.0) as u32
}