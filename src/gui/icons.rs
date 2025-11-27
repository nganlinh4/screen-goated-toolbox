// --- ENHANCED ICON PAINTER MODULE V2 ---
// High-fidelity programmatic vector icons for egui.
// No assets, no fonts, pure math.

use eframe::egui;
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Settings,
    Moon,
    Sun,
    EyeOpen,
    EyeClosed,
    Microphone,
    Image,
    Video,
    Delete, // Now renders as a Trash Can
    Info,
    Statistics,
    Refresh,
}

/// Main entry point: Draw a clickable icon button
pub fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    let size = egui::vec2(24.0, 24.0); // Comfortable touch target
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    // 1. Background Hover Effect
    if response.hovered() {
        ui.painter().rect_filled(
            rect.shrink(2.0),
            4.0,
            ui.visuals().widgets.hovered.bg_fill,
        );
    }

    // 2. Determine Style
    let color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };

    // 3. Paint
    paint_internal(ui.painter(), rect, icon, color);

    response
}

/// Draw a static icon (for labels/headers)
pub fn draw_icon_static(ui: &mut egui::Ui, icon: Icon, size_override: Option<f32>) {
    let side = size_override.unwrap_or(16.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let color = ui.visuals().text_color();
    paint_internal(ui.painter(), rect, icon, color);
}

// --- INTERNAL PAINTER ENGINE ---

fn paint_internal(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let center = rect.center();
    // Base scale on a 20x20 reference grid, scaled to actual rect
    let scale = rect.width().min(rect.height()) / 22.0;
    let stroke = egui::Stroke::new(1.5 * scale, color); // Consistent line weight

    match icon {
        Icon::Settings => {
            // Modern Cogwheel
            let teeth = 8;
            let outer_r = 9.0 * scale;
            let inner_r = 6.5 * scale;
            let hole_r = 2.5 * scale;

            let mut points = Vec::new();
            for i in 0..(teeth * 2) {
                let theta = (i as f32 * PI) / teeth as f32;
                let r = if i % 2 == 0 { outer_r } else { inner_r };

                // Add slight bevel to teeth for softness
                let bevel_angle = (PI / teeth as f32) * 0.25;
                let theta_a = theta - bevel_angle;
                let theta_b = theta + bevel_angle;

                points.push(center + egui::vec2(theta_a.cos() * r, theta_a.sin() * r));
                points.push(center + egui::vec2(theta_b.cos() * r, theta_b.sin() * r));
            }
            points.push(points[0]); // Close loop

            painter.add(egui::Shape::line(points, stroke));
            painter.circle_stroke(center, hole_r, stroke);
        }

        Icon::Moon => {
            // Sharp Crescent
            let r = 7.0 * scale;
            let offset = 3.5 * scale;

            // We draw a full circle, then clip it? No, painters algorithm:
            // Draw filled circle, then cut with background?
            // Better: Draw the actual crescent shape if possible, but boolean diff is hard.
            // Fallback to "Masking" visual trick which is cheapest in immediate mode.

            painter.circle_filled(center, r, color);
            painter.circle_filled(
                center + egui::vec2(offset, -offset * 0.8),
                r * 0.85,
                painter.ctx().style().visuals.panel_fill, // Theme background color
            );
            // Redraw faint border on the inner curve to define it against bg?
            // usually not needed if bg matches.
        }

        Icon::Sun => {
            // Core
            painter.circle_stroke(center, 4.0 * scale, stroke);

            // Rays (detached)
            for i in 0..8 {
                let angle = (i as f32 * 45.0).to_radians();
                let dir = egui::vec2(angle.cos(), angle.sin());
                let start = center + dir * 6.5 * scale;
                let end = center + dir * 9.0 * scale;
                painter.line_segment([start, end], stroke);
            }
        }

        Icon::EyeOpen => {
            let w = 9.0 * scale;
            let h = 5.0 * scale;

            // Upper lid (Arc)
            let p_left = center - egui::vec2(w, 0.0);
            let p_right = center + egui::vec2(w, 0.0);
            let p_top = center - egui::vec2(0.0, h * 1.5);
            let p_bot = center + egui::vec2(0.0, h * 1.5);

            // Use simplified quadratic bezier logic
            let pts_top = bezier_points(p_left, p_top, p_right, 10);
            let pts_bot = bezier_points(p_right, p_bot, p_left, 10);

            let mut full_eye = pts_top;
            full_eye.extend(pts_bot);

            painter.add(egui::Shape::line(full_eye, stroke));
            painter.circle_filled(center, 2.5 * scale, color);
        }

        Icon::EyeClosed => {
            let w = 9.0 * scale;
            let h = 5.0 * scale;

            let p_left = center - egui::vec2(w, 0.0);
            let p_right = center + egui::vec2(w, 0.0);
            let p_top = center - egui::vec2(0.0, h * 1.5);

            let pts = bezier_points(p_left, p_top, p_right, 12);
            painter.add(egui::Shape::line(pts, stroke));

            // Lashes
            let lash_y = center.y + 1.0 * scale;
            let l_len = 3.5 * scale;

            // Center lash
            painter.line_segment([egui::pos2(center.x, lash_y), egui::pos2(center.x, lash_y + l_len)], stroke);
            // Side lashes (rotated)
            painter.line_segment([egui::pos2(center.x - 3.0*scale, lash_y - 1.0*scale), egui::pos2(center.x - 5.0*scale, lash_y + l_len*0.8)], stroke);
            painter.line_segment([egui::pos2(center.x + 3.0*scale, lash_y - 1.0*scale), egui::pos2(center.x + 5.0*scale, lash_y + l_len*0.8)], stroke);
        }

        Icon::Microphone => {
            let w = 5.0 * scale;
            let h = 10.0 * scale;
            let caps_rect = egui::Rect::from_center_size(center - egui::vec2(0.0, 2.0*scale), egui::vec2(w, h));

            // 1. Capsule Body
            painter.rect_stroke(caps_rect, w/2.0, stroke);

            // 2. Grille details (small horizontal lines inside)
            let y_start = caps_rect.top() + 3.0 * scale;
            painter.line_segment([egui::pos2(center.x - 1.5*scale, y_start), egui::pos2(center.x + 1.5*scale, y_start)], stroke);
            painter.line_segment([egui::pos2(center.x - 1.5*scale, y_start + 2.5*scale), egui::pos2(center.x + 1.5*scale, y_start + 2.5*scale)], stroke);

            // 3. Holder/Stand
            // Simpler U-bracket:
            let u_left = egui::pos2(center.x - 4.5*scale, center.y - 1.0*scale);
            let u_right = egui::pos2(center.x + 4.5*scale, center.y - 1.0*scale);
            let u_bot = egui::pos2(center.x, center.y + 6.0*scale);
            let u_path = bezier_points(u_left, u_bot, u_right, 10);
            painter.add(egui::Shape::line(u_path, stroke));

            // Stem and Foot
            painter.line_segment([egui::pos2(center.x, center.y + 3.5*scale), egui::pos2(center.x, center.y + 8.0*scale)], stroke);
            painter.line_segment([egui::pos2(center.x - 3.0*scale, center.y + 8.0*scale), egui::pos2(center.x + 3.0*scale, center.y + 8.0*scale)], stroke);
        }

        Icon::Image => {
            let img_rect = rect.shrink(3.0 * scale);
            painter.rect_stroke(img_rect, 2.0 * scale, stroke);

            // Mountains
            let p1 = img_rect.left_bottom() - egui::vec2(-1.0, 2.0)*scale;
            let p2 = img_rect.left_bottom() + egui::vec2(3.0, -6.0)*scale; // Peak 1
            let p3 = img_rect.left_bottom() + egui::vec2(6.0, -3.0)*scale; // Valley
            let p4 = img_rect.left_bottom() + egui::vec2(9.0, -7.0)*scale; // Peak 2
            let p5 = img_rect.right_bottom() - egui::vec2(1.0, 2.0)*scale;

            painter.add(egui::Shape::line(vec![p1, p2, p3, p4, p5], stroke));

            // Sun
            painter.circle_filled(img_rect.left_top() + egui::vec2(3.5, 3.5)*scale, 1.5*scale, color);
        }

        Icon::Video => {
            // Modern Rounded Camera
            let body_w = 12.0 * scale;
            let body_h = 8.0 * scale;
            let body_rect = egui::Rect::from_center_size(center - egui::vec2(1.0*scale, 0.0), egui::vec2(body_w, body_h));

            painter.rect_stroke(body_rect, 2.0 * scale, stroke);

            // Lens Triangle
            let l_x = body_rect.right();
            let l_y = center.y;
            let lens_pts = vec![
                egui::pos2(l_x, l_y - 2.0*scale),
                egui::pos2(l_x + 3.5*scale, l_y - 3.5*scale),
                egui::pos2(l_x + 3.5*scale, l_y + 3.5*scale),
                egui::pos2(l_x, l_y + 2.0*scale),
            ];
            painter.add(egui::Shape::closed_line(lens_pts, stroke));

            // Reels circles (small detail on top)
            painter.circle_stroke(body_rect.left_top() + egui::vec2(3.0, 0.0)*scale, 1.5*scale, stroke);
            painter.circle_stroke(body_rect.right_top() + egui::vec2(-3.0, 0.0)*scale, 1.5*scale, stroke);
        }

        Icon::Delete => {
            // Trash Can (Simplified & Re-aligned)
            // Shift up significantly (2.0 scale units) for better optical alignment in the button
            let c = center - egui::vec2(0.0, 2.0 * scale);
            
            let lid_y = c.y - 3.2 * scale;
            let w_lid = 8.0 * scale; 
            let w_can_top = 6.0 * scale;
            let w_can_bot = 4.5 * scale;
            let h_can = 7.0 * scale;

            // Lid line
            painter.line_segment(
                [egui::pos2(c.x - w_lid/2.0, lid_y), egui::pos2(c.x + w_lid/2.0, lid_y)],
                stroke
            );
            
            // Handle
            painter.line_segment(
                [egui::pos2(c.x - 1.0*scale, lid_y), egui::pos2(c.x - 1.0*scale, lid_y - 1.0*scale)],
                stroke
            );
            painter.line_segment(
                [egui::pos2(c.x - 1.0*scale, lid_y - 1.0*scale), egui::pos2(c.x + 1.0*scale, lid_y - 1.0*scale)],
                stroke
            );
            painter.line_segment(
                [egui::pos2(c.x + 1.0*scale, lid_y - 1.0*scale), egui::pos2(c.x + 1.0*scale, lid_y)],
                stroke
            );

            // Can Body (Clean - Removed Ribs for less boldness)
            let p1 = egui::pos2(c.x - w_can_top/2.0, lid_y);
            let p2 = egui::pos2(c.x - w_can_bot/2.0, lid_y + h_can);
            let p3 = egui::pos2(c.x + w_can_bot/2.0, lid_y + h_can);
            let p4 = egui::pos2(c.x + w_can_top/2.0, lid_y);

            painter.add(egui::Shape::line(vec![p1, p2, p3, p4], stroke));
        }

        Icon::Info => {
            // Shift up slightly for alignment
            let c = center - egui::vec2(0.0, 1.0 * scale);
            
            // Circle
            painter.circle_stroke(c, 5.0 * scale, stroke);
            // Dot (Floating)
            painter.circle_filled(c - egui::vec2(0.0, 1.8 * scale), 0.6 * scale, color);
            // Line
            painter.rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(0.0, 1.0 * scale), egui::vec2(1.0 * scale, 2.5 * scale)),
                0.4 * scale,
                color,
            );
        }

        Icon::Statistics => {
            let base_y = center.y + 6.0 * scale;
            let bar_w = 2.5 * scale;
            let gap = 1.5 * scale;

            // Bars
            let h1 = 4.0 * scale;
            let h2 = 7.0 * scale;
            let h3 = 10.0 * scale;

            let x1 = center.x - bar_w - gap;
            let x2 = center.x;
            let x3 = center.x + bar_w + gap;

            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x1 - bar_w/2.0, base_y - h1), egui::pos2(x1 + bar_w/2.0, base_y)), 1.0, color);
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x2 - bar_w/2.0, base_y - h2), egui::pos2(x2 + bar_w/2.0, base_y)), 1.0, color);
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x3 - bar_w/2.0, base_y - h3), egui::pos2(x3 + bar_w/2.0, base_y)), 1.0, color);

            // Trend Line
            let t_offset = 3.0 * scale; // Floating above bars
            let points = vec![
                egui::pos2(x1 - bar_w, base_y - h1 - t_offset + 2.0*scale), // Start low
                egui::pos2(x1, base_y - h1 - t_offset),
                egui::pos2(x2, base_y - h2 - t_offset),
                egui::pos2(x3, base_y - h3 - t_offset),
                egui::pos2(x3 + bar_w, base_y - h3 - t_offset - 2.0*scale), // Shoot up
            ];
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.2 * scale, color)));
        }

        Icon::Refresh => {
            // Smaller radius for a cleaner look (was 8.0)
            let r = 6.0 * scale;
            
            // Slightly thinner stroke for elegance
            let refresh_stroke = egui::Stroke::new(1.2 * scale, color);

            let segments = 30;
            
            // Gap at the top (~12 o'clock).
            // Screen coordinates: 0=Right, PI/2=Bottom, PI=Left, -PI/2=Top.
            // Clockwise movement = increasing angle.
            
            // Start at 12:30 (Top-Right)
            let start_angle = -PI / 2.0 + 0.6;
            // Sweep clockwise to 11:30 (Top-Left), leaving a gap at the top
            let sweep = 2.0 * PI - 1.2;
            
            let mut points = Vec::new();
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start_angle + sweep * t;
                points.push(center + egui::vec2(angle.cos() * r, angle.sin() * r));
            }
            
            // Draw the circle arc
            painter.add(egui::Shape::line(points.clone(), refresh_stroke));
            
            // Draw Arrowhead at the end of the arc
            if let Some(tip) = points.last() {
                let end_angle = start_angle + sweep;
                let arrow_len = 3.5 * scale;
                
                // The direction the line is moving at the end (tangent)
                // For a clockwise circle, tangent is +90 degrees from the radius vector
                let tangent = end_angle + PI / 2.0;
                
                // Calculate wings pointing backwards from the tip
                let wing_offset = 0.6; // ~35 degrees
                let back_angle1 = tangent - PI + wing_offset;
                let back_angle2 = tangent - PI - wing_offset;

                let p1 = *tip + egui::vec2(back_angle1.cos() * arrow_len, back_angle1.sin() * arrow_len);
                let p2 = *tip + egui::vec2(back_angle2.cos() * arrow_len, back_angle2.sin() * arrow_len);

                painter.add(egui::Shape::line(vec![p1, *tip, p2], refresh_stroke));
            }
        }
    }
}

// --- MATH HELPERS ---

fn lerp(a: egui::Pos2, b: egui::Pos2, t: f32) -> egui::Pos2 {
    egui::pos2(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
    )
}

fn lerp_quadratic(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, t: f32) -> egui::Pos2 {
    let l1 = lerp(p0, p1, t);
    let l2 = lerp(p1, p2, t);
    lerp(l1, l2, t)
}

/// Generates points for a smooth curve
fn bezier_points(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, segments: usize) -> Vec<egui::Pos2> {
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        points.push(lerp_quadratic(p0, p1, p2, t));
    }
    points
}
