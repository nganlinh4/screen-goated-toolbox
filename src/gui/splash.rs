use eframe::egui;
use eframe::egui::{Color32, Pos2, Rect, Rounding, Stroke, Vec2};

pub enum SplashStatus {
    Ongoing,
    Finished,
}

pub struct SplashScreen {
    start_time: f64,
    duration: f32,
}

impl SplashScreen {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            start_time: ctx.input(|i| i.time),
            duration: 3.5, // Total animation time in seconds
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) -> SplashStatus {
        let time = ctx.input(|i| i.time) - self.start_time;
        let t = (time as f32 / self.duration).clamp(0.0, 1.0);

        // Request repaint to keep animation smooth
        ctx.request_repaint();

        if t >= 1.0 {
            return SplashStatus::Finished;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();
            let rect = ui.max_rect();
            let center = rect.center();

            // --- 1. Background (Fill entire screen with dark theme) ---
            painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 10, 10));

            // Dimensions for the logo
            let logo_size = 120.0;
            let logo_rect = Rect::from_center_size(center, Vec2::splat(logo_size));
            
            // --- 2. Animation Stages ---
            // 0.0 - 0.2: Fade in & Scale up
            // 0.2 - 0.8: The Scan
            // 0.8 - 1.0: Text Reveal & Fade out

            let opacity = if t < 0.2 {
                remap(t, 0.0, 0.2, 0.0, 1.0)
            } else if t > 0.85 {
                remap(t, 0.85, 1.0, 1.0, 0.0)
            } else {
                1.0
            };

            // Calculate Scan Line Position (moves top to bottom)
            // Starts above logo, ends below logo
            let scan_progress = remap(t, 0.2, 0.8, 0.0, 1.0).clamp(0.0, 1.0);
            let scan_y = logo_rect.top() + (logo_rect.height() * scan_progress);

            // --- 3. Draw The Logo Base (Dim/Unscanned state) ---
            // We draw the full logo in a "dim" state first
            self.draw_logo_geometry(painter, logo_rect, Color32::from_rgb(40, 40, 40), opacity * 0.5);

            // --- 4. Draw The Logo Lit (Scanned state) ---
            // We use a clip rect to only reveal the part of the logo the scanner has passed
            // The scanner moves DOWN, so we reveal everything ABOVE scan_y
            
            // Only draw if we have scanned at least a bit
            if scan_progress > 0.0 {
                let clip_rect = rect.intersect(Rect::from_min_max(rect.min, Pos2::new(rect.max.x, scan_y)));
                painter.with_clip_rect(clip_rect).rect_filled(
                    Rect::ZERO, // Dummy, ignored by closure
                    0.0,
                    Color32::TRANSPARENT, 
                );
                
                // Hack: Egui immediate mode clipping is tricky in one pass. 
                // Instead, we just draw the lit geometry, but we check Y inside the drawing function
                // or we rely on the visual trick of the bright scanner line covering the transition.
                
                // Better approach for simple shapes: Draw "Lit" geometry with a custom function 
                // that clamps geometry to scan_y.
                // For simplicity in this demo, we will just draw the lit logo ON TOP, 
                // but use a scissor clip in the painter if possible. 
                // Since `painter.clip_rect` is for widgets, we will simulate the reveal 
                // by manually calculating the geometry height for the lit part.
                
                self.draw_lit_geometry(painter, logo_rect, scan_y, opacity);
            }

            // --- 5. The Scanner Beam (The "Green Laser") ---
            if t > 0.15 && t < 0.85 {
                let beam_width = logo_rect.width() * 1.4;
                let beam_height = 4.0;
                let beam_rect = Rect::from_center_size(
                    Pos2::new(center.x, scan_y),
                    Vec2::new(beam_width, beam_height)
                );

                // Glow effect (stacked lines)
                let neon_green = Color32::from_rgb(0, 255, 128); // SGT Green
                
                // Outer glow (faint)
                painter.rect_filled(
                    beam_rect.expand(6.0),
                    Rounding::same(4.0),
                    Color32::from_rgba_premultiplied(0, 255, 128, (20.0 * opacity) as u8)
                );
                // Mid glow
                painter.rect_filled(
                    beam_rect.expand(2.0),
                    Rounding::same(2.0),
                    Color32::from_rgba_premultiplied(0, 255, 128, (100.0 * opacity) as u8)
                );
                // Core (White hot)
                painter.rect_filled(
                    beam_rect,
                    Rounding::same(1.0),
                    Color32::WHITE.linear_multiply(opacity),
                );

                // "Particles" / Digital noise trailing the beam
                // We use a pseudo-random generator based on position to make it deterministic but "noisy"
                let seed = (scan_y as i32 / 10) as f32;
                for i in 0..5 {
                    let offset_x = ((seed * i as f32 * 123.45).sin()) * (logo_size / 1.5);
                    let particle_y = scan_y - 5.0 - (i as f32 * 3.0);
                    let p_size = 2.0;
                    if particle_y > logo_rect.top() {
                        painter.rect_filled(
                            Rect::from_center_size(Pos2::new(center.x + offset_x, particle_y), Vec2::splat(p_size)),
                            0.0,
                            neon_green.linear_multiply(opacity * (1.0 - i as f32/5.0))
                        );
                    }
                }
            }

            // --- 6. Text (Appears after scan) ---
            if scan_progress > 0.6 {
                let text_opacity = remap(scan_progress, 0.6, 1.0, 0.0, 1.0);
                let font_id = egui::FontId::proportional(24.0);
                
                painter.text(
                    center + Vec2::new(0.0, logo_size * 0.8),
                    egui::Align2::CENTER_CENTER,
                    "Screen Grounded Translator",
                    font_id,
                    Color32::WHITE.linear_multiply(text_opacity * opacity),
                );
            }
        });

        SplashStatus::Ongoing
    }

    // Helper to draw the logo shapes (based on your icon: Circle Top-Left, Rounded Rect Bottom-Right)
    fn draw_logo_geometry(&self, painter: &egui::Painter, rect: Rect, color: Color32, opacity: f32) {
        let draw_color = color.linear_multiply(opacity);
        
        // 1. The Container (Rounded Square)
        painter.rect_stroke(rect, Rounding::same(20.0), Stroke::new(3.0, draw_color));

        // 2. The Circle (Top Left)
        let circle_radius = rect.width() * 0.25;
        let circle_center = rect.min + Vec2::new(rect.width() * 0.35, rect.height() * 0.35);
        painter.circle_filled(circle_center, circle_radius, draw_color);

        // 3. The Rounded Rect (Bottom Right)
        let sub_rect_size = Vec2::new(rect.width() * 0.45, rect.height() * 0.4);
        let sub_rect_pos = rect.max - sub_rect_size - Vec2::splat(rect.width() * 0.1);
        let sub_rect = Rect::from_min_size(sub_rect_pos, sub_rect_size);
        painter.rect_filled(sub_rect, Rounding::same(10.0), draw_color);
    }

    // Helper to draw only the top part of the logo (The "Lit" part)
    fn draw_lit_geometry(&self, painter: &egui::Painter, rect: Rect, scan_y: f32, opacity: f32) {
        // We use the painter's clip_rect functionality to mask the drawing
        let clip = Rect::from_min_max(rect.min - Vec2::splat(20.0), Pos2::new(rect.max.x + 20.0, scan_y));
        
        // Push clip rect
        let _ = painter.with_clip_rect(clip);
        
        // Draw the logo again, but Bright White
        self.draw_logo_geometry(painter, rect, Color32::WHITE, opacity);
    }
}

// Math helper
fn remap(val: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = (val - in_min) / (in_max - in_min);
    let t = t.clamp(0.0, 1.0);
    out_min + t * (out_max - out_min)
}
