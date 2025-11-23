use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, Rounding, Stroke, Vec2, FontId, Align2};

// --- Configuration ---
const ANIMATION_DURATION: f32 = 4.5;
const GREEN_CORE: Color32 = Color32::from_rgb(50, 255, 150);
const GREEN_GLOW: Color32 = Color32::from_rgb(0, 255, 100);
const DARK_BG: Color32 = Color32::from_rgb(10, 12, 16);

#[derive(Clone, Copy)]
struct Particle {
    pos: Pos2,
    vel: Vec2,
    life: f32,     // 1.0 to 0.0
    max_life: f32,
    size: f32,
}

pub enum SplashStatus {
    Ongoing,
    Finished,
}

pub struct SplashScreen {
    start_time: f64,
    particles: Vec<Particle>,
    rng_seed: u64, // Simple pseudo-rng state
}

impl SplashScreen {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            start_time: ctx.input(|i| i.time),
            particles: Vec::with_capacity(100),
            rng_seed: 12345,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) -> SplashStatus {
        let time = (ctx.input(|i| i.time) - self.start_time) as f32;
        let t_norm = (time / ANIMATION_DURATION).clamp(0.0, 1.0);

        ctx.request_repaint(); // Animation requires continuous repaint

        if t_norm >= 1.0 {
            return SplashStatus::Finished;
        }

        // --- Pseudo-Random Generator (Deterministic per frame for effects) ---
        let mut local_seed = self.rng_seed;
        let mut next_rand = || -> f32 {
            local_seed = local_seed.wrapping_add(0x5DEECE66D);
            (local_seed >> 16) as f32 / 65535.0
        };

        // We need to collect new particles to add, to avoid borrowing self.particles inside the closure
        let new_particles_cell = std::cell::RefCell::new(Vec::new());
        
        // 1. Particle Logic (Update existing)
        let mut keep_particles = Vec::new();
        for mut p in self.particles.drain(..) {
            p.pos += p.vel;
            p.vel.y += 0.1; // Gravity
            p.life -= 0.02; // Decay
            if p.life > 0.0 {
                keep_particles.push(p);
            }
        }
        self.particles = keep_particles;

        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            let rect = ui.max_rect();
            let center = rect.center();
            let logo_size = 140.0;
            
            // 1. Background: Deep Void with Moving Grid
            painter.rect_filled(rect, 0.0, DARK_BG);
            Self::draw_perspective_grid(painter, rect, time);

            // 2. Calculate Animation Phases
            let logo_rect = Rect::from_center_size(center, Vec2::splat(logo_size));

            // --- Phase 1: Wireframe Construction ---
            let wireframe_progress = remap(time, 0.0, 1.5, 0.0, 1.0).clamp(0.0, 1.0);
            if wireframe_progress > 0.0 && time < 3.5 {
                // Jitter effect
                let jitter = if next_rand() > 0.95 { Vec2::new(next_rand() * 4.0 - 2.0, 0.0) } else { Vec2::ZERO };
                Self::draw_logo_wireframe(painter, logo_rect.translate(jitter), wireframe_progress);
            }

            // --- Phase 2: The Scanner & Solidification ---
            let scan_progress = remap(time, 1.5, 3.0, 0.0, 1.0).clamp(0.0, 1.0);
            if scan_progress > 0.0 {
                let scan_y = logo_rect.top() + (logo_rect.height() * scan_progress);
                
                // A. Solidify Logo
                let mut clip_rect = rect;
                clip_rect.max.y = scan_y;
                
                if time < 3.8 {
                   let _ = painter.with_clip_rect(clip_rect);
                   Self::draw_logo_solid_clipped(painter, logo_rect, scan_y, 1.0);
                }

                // B. The Laser Beam
                if scan_progress < 1.0 {
                    let beam_rect = Rect::from_min_size(
                        Pos2::new(rect.left(), scan_y), 
                        Vec2::new(rect.width(), 2.0)
                    );
                    
                    painter.rect_filled(beam_rect, 0.0, Color32::WHITE);
                    painter.rect_filled(beam_rect.expand(2.0), 2.0, GREEN_CORE.linear_multiply(0.5));
                    painter.rect_filled(beam_rect.expand(15.0), 10.0, GREEN_GLOW.linear_multiply(0.1));

                    // C. Spawn Particles
                    if next_rand() > 0.3 {
                        let spark_x = logo_rect.left() + next_rand() * logo_rect.width();
                        new_particles_cell.borrow_mut().push(Particle {
                            pos: Pos2::new(spark_x, scan_y),
                            vel: Vec2::new(next_rand() * 4.0 - 2.0, next_rand() * 4.0 - 5.0), // Fly up
                            life: 1.0,
                            max_life: 0.5 + next_rand() * 0.5,
                            size: 1.0 + next_rand() * 2.0,
                        });
                    }
                }
            }
            
            // --- Phase 3: Text Decoding ---
            let text_start = 2.5;
            if time > text_start {
                let text_progress = remap(time, text_start, 3.5, 0.0, 1.0);
                let final_text = "SCREEN GROUNDED TRANSLATOR";
                let decoded_len = (final_text.len() as f32 * text_progress) as usize;
                
                let mut display_text = String::new();
                for (i, c) in final_text.chars().enumerate() {
                    if i < decoded_len {
                        display_text.push(c);
                    } else {
                        let chars = "AXE78901_#@!$%&";
                        let idx = (next_rand() * 100.0) as usize % chars.len();
                        display_text.push(chars.chars().nth(idx).unwrap());
                    }
                }
                
                 let opacity = remap(time, text_start, text_start + 0.5, 0.0, 1.0).clamp(0.0, 1.0);
                let text_pos = center + Vec2::new(0.0, logo_size * 0.7);
                
                painter.text(
                    text_pos,
                    Align2::CENTER_TOP,
                    &display_text,
                    FontId::proportional(20.0),
                    Color32::WHITE.linear_multiply(opacity),
                );
                
                if time < 3.8 {
                     painter.text(
                        text_pos + Vec2::new(1.0, 1.0),
                        Align2::CENTER_TOP,
                        &display_text,
                        FontId::proportional(20.0),
                        GREEN_GLOW.linear_multiply(opacity * 0.5),
                    );
                }
            }

            // Draw existing particles
            for p in &self.particles {
                 let alpha = (p.life / p.max_life).clamp(0.0, 1.0);
                 painter.circle_filled(p.pos, p.size, GREEN_CORE.linear_multiply(alpha));
            }

            // --- Final Exit Fade ---
            if t_norm > 0.9 {
                let fade = remap(t_norm, 0.9, 1.0, 0.0, 1.0);
                painter.rect_filled(rect, 0.0, Color32::from_black_alpha((fade * 255.0) as u8));
            }
        }); 
        
        // Append new particles
        self.particles.extend(new_particles_cell.into_inner());
        self.rng_seed = local_seed; // Update seed
        
        SplashStatus::Ongoing
    }

    // --- Helper: Retro Perspective Grid ---
    fn draw_perspective_grid(painter: &egui::Painter, rect: Rect, time: f32) {
        let horizon_y = rect.center().y;
        let speed = (time * 50.0) % 40.0;
        let color = Color32::from_rgb(30, 40, 50).linear_multiply(0.5);

        // Vertical lines (converging)
        let center_x = rect.center().x;
        for i in -10..=10 {
            let offset = i as f32 * 60.0;
            let p1 = Pos2::new(center_x + offset, rect.bottom());
            let p2 = Pos2::new(center_x + (offset * 0.1), horizon_y); // Converge
            painter.line_segment([p1, p2], Stroke::new(1.0, color));
        }

        // Horizontal lines (moving down)
        for i in 0..15 {
            let dist_factor = i as f32 / 15.0; // 0 (horizon) to 1 (bottom)
            // Exponential spacing for depth
            let y_base = dist_factor.powf(3.0) * (rect.bottom() - horizon_y); 
            let y = horizon_y + y_base + speed * dist_factor.powf(2.0); // Move faster near cam
            
            if y < rect.bottom() {
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], 
                    Stroke::new(1.0, color.linear_multiply(dist_factor))
                );
            }
        }
    }

    // --- Helper: Tracing Wireframe ---
    fn draw_logo_wireframe(painter: &egui::Painter, rect: Rect, progress: f32) {
        let corner_radius = 20.0;
        
        // 1. Outer Box Trace
        let mut shape_rect = rect;
        shape_rect = shape_rect.expand(2.0);
        
        let pts = [
            shape_rect.left_top(), shape_rect.right_top(), 
            shape_rect.right_bottom(), shape_rect.left_bottom(), 
            shape_rect.left_top()
        ];

        // Simpler Wireframe Effect:
        painter.rect_stroke(rect, Rounding::same(corner_radius), Stroke::new(1.0, Color32::from_rgb(40, 50, 60)));

        // "Writing" effect: A bright dot moving along the path
        if progress < 1.0 {
            let t = (progress * 4.0) % 4.0; // 4 sides
            let side = t.floor() as usize;
            let sub_t = t.fract();
            
            let p1 = pts[side];
            let p2 = pts[side+1];
            let dot_pos = p1 + (p2 - p1) * sub_t;
            
            // The "Writer" head
            painter.circle_filled(dot_pos, 3.0, GREEN_CORE);
            painter.circle_stroke(dot_pos, 6.0, Stroke::new(1.0, GREEN_GLOW));
        }
    }

    // --- Helper: Solid Logo (Clipped Manually) ---
    fn draw_logo_solid_clipped(painter: &egui::Painter, rect: Rect, clip_y: f32, opacity: f32) {
        if rect.top() > clip_y { return; } // Completely hidden

        let fill_color = Color32::WHITE.linear_multiply(opacity);
        let dark_fill = Color32::from_rgb(20, 20, 20).linear_multiply(opacity);

        // 1. Container
        painter.rect_filled(rect, Rounding::same(20.0), dark_fill);
        painter.rect_stroke(rect, Rounding::same(20.0), Stroke::new(3.0, fill_color));

        // 2. Top-Left Circle (Iconography)
        let r = rect.width() * 0.25;
        let c_pos = rect.min + Vec2::new(rect.width() * 0.35, rect.height() * 0.35);
        painter.circle_filled(c_pos, r, fill_color);

        // 3. Bottom-Right Shape
        let sub_size = Vec2::new(rect.width() * 0.45, rect.height() * 0.4);
        let sub_pos = rect.max - sub_size - Vec2::splat(rect.width() * 0.1);
        painter.rect_filled(Rect::from_min_size(sub_pos, sub_size), Rounding::same(10.0), fill_color);
    }
}

// Math helper
fn remap(val: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = (val - in_min) / (in_max - in_min);
    let t = t.clamp(0.0, 1.0);
    out_min + t * (out_max - out_min)
}
