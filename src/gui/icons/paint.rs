// --- INTERNAL ICON PAINTER ENGINE ---
// Contains the paint_internal function and math helpers for icon rendering.
// Additional icons (TextSelect onwards) are in paint_extra.rs.

use super::Icon;
use eframe::egui;
use std::f32::consts::PI;

pub(super) fn paint_internal(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: Icon,
    color: egui::Color32,
) {
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

                let bevel_angle = (PI / teeth as f32) * 0.25;
                let theta_a = theta - bevel_angle;
                let theta_b = theta + bevel_angle;

                points.push(center + egui::vec2(theta_a.cos() * r, theta_a.sin() * r));
                points.push(center + egui::vec2(theta_b.cos() * r, theta_b.sin() * r));
            }
            points.push(points[0]);

            painter.add(egui::Shape::line(points, stroke));
            painter.circle_stroke(center, hole_r, stroke);
        }

        Icon::EyeOpen => {
            let w = 9.0 * scale;
            let h = 5.0 * scale;
            let p_left = center - egui::vec2(w, 0.0);
            let p_right = center + egui::vec2(w, 0.0);
            let p_top = center - egui::vec2(0.0, h * 1.5);
            let p_bot = center + egui::vec2(0.0, h * 1.5);

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

            let lash_y = center.y + 1.0 * scale;
            let l_len = 3.5 * scale;
            painter.line_segment(
                [
                    egui::pos2(center.x, lash_y),
                    egui::pos2(center.x, lash_y + l_len),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - 3.0 * scale, lash_y - 1.0 * scale),
                    egui::pos2(center.x - 5.0 * scale, lash_y + l_len * 0.8),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 3.0 * scale, lash_y - 1.0 * scale),
                    egui::pos2(center.x + 5.0 * scale, lash_y + l_len * 0.8),
                ],
                stroke,
            );
        }

        Icon::Microphone => {
            // Larger Microphone icon
            let w = 6.5 * scale;
            let h = 12.0 * scale;
            let caps_rect = egui::Rect::from_center_size(
                center - egui::vec2(0.0, 1.5 * scale),
                egui::vec2(w, h),
            );
            painter.rect_stroke(caps_rect, w / 2.0, stroke, egui::StrokeKind::Middle);

            // Horizontal lines on mic head
            let y_start = caps_rect.top() + 3.5 * scale;
            painter.line_segment(
                [
                    egui::pos2(center.x - 2.0 * scale, y_start),
                    egui::pos2(center.x + 2.0 * scale, y_start),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - 2.0 * scale, y_start + 3.0 * scale),
                    egui::pos2(center.x + 2.0 * scale, y_start + 3.0 * scale),
                ],
                stroke,
            );

            // U-shaped holder
            let u_left = egui::pos2(center.x - 5.5 * scale, center.y);
            let u_right = egui::pos2(center.x + 5.5 * scale, center.y);
            let u_bot = egui::pos2(center.x, center.y + 7.0 * scale);
            let u_path = bezier_points(u_left, u_bot, u_right, 10);
            painter.add(egui::Shape::line(u_path, stroke));

            // Stand
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y + 4.5 * scale),
                    egui::pos2(center.x, center.y + 9.0 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - 4.0 * scale, center.y + 9.0 * scale),
                    egui::pos2(center.x + 4.0 * scale, center.y + 9.0 * scale),
                ],
                stroke,
            );
        }

        Icon::Image => {
            let img_rect = rect.shrink(3.0 * scale);
            painter.rect_stroke(img_rect, 2.0 * scale, stroke, egui::StrokeKind::Middle);
            let p1 = img_rect.left_bottom() - egui::vec2(-1.0, 2.0) * scale;
            let p2 = img_rect.left_bottom() + egui::vec2(3.0, -6.0) * scale;
            let p3 = img_rect.left_bottom() + egui::vec2(6.0, -3.0) * scale;
            let p4 = img_rect.left_bottom() + egui::vec2(9.0, -7.0) * scale;
            let p5 = img_rect.right_bottom() - egui::vec2(1.0, 2.0) * scale;
            painter.add(egui::Shape::line(vec![p1, p2, p3, p4, p5], stroke));
            painter.circle_filled(
                img_rect.left_top() + egui::vec2(3.5, 3.5) * scale,
                1.5 * scale,
                color,
            );
        }

        Icon::Text => {
            // Larger Elegant Serif 'T' Icon
            let top_y = center.y - 8.0 * scale;
            let bot_y = center.y + 8.0 * scale;
            let left_x = center.x - 7.0 * scale;
            let right_x = center.x + 7.0 * scale;
            let serif_h = 2.0 * scale;
            let stem_w = 2.5 * scale;

            // Top horizontal bar (thicker)
            let bar_stroke = egui::Stroke::new(2.5 * scale, color);
            painter.line_segment(
                [egui::pos2(left_x, top_y), egui::pos2(right_x, top_y)],
                bar_stroke,
            );

            // Left serif
            painter.line_segment(
                [
                    egui::pos2(left_x, top_y),
                    egui::pos2(left_x, top_y + serif_h),
                ],
                stroke,
            );

            // Right serif
            painter.line_segment(
                [
                    egui::pos2(right_x, top_y),
                    egui::pos2(right_x, top_y + serif_h),
                ],
                stroke,
            );

            // Vertical stem (thicker)
            let stem_stroke = egui::Stroke::new(2.0 * scale, color);
            painter.line_segment(
                [egui::pos2(center.x, top_y), egui::pos2(center.x, bot_y)],
                stem_stroke,
            );

            // Bottom serif
            painter.line_segment(
                [
                    egui::pos2(center.x - stem_w, bot_y),
                    egui::pos2(center.x + stem_w, bot_y),
                ],
                stroke,
            );
        }

        Icon::Delete => {
            // Trash Can (original, for presets) - centered in hitbox
            let c = center;
            let lid_y = c.y - 3.2 * scale;
            let w_lid = 8.0 * scale;
            let w_can_top = 6.0 * scale;
            let w_can_bot = 4.5 * scale;
            let h_can = 7.0 * scale;

            painter.line_segment(
                [
                    egui::pos2(c.x - w_lid / 2.0, lid_y),
                    egui::pos2(c.x + w_lid / 2.0, lid_y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.0 * scale, lid_y),
                    egui::pos2(c.x - 1.0 * scale, lid_y - 1.0 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.0 * scale, lid_y - 1.0 * scale),
                    egui::pos2(c.x + 1.0 * scale, lid_y - 1.0 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.0 * scale, lid_y - 1.0 * scale),
                    egui::pos2(c.x + 1.0 * scale, lid_y),
                ],
                stroke,
            );

            let p1 = egui::pos2(c.x - w_can_top / 2.0, lid_y);
            let p2 = egui::pos2(c.x - w_can_bot / 2.0, lid_y + h_can);
            let p3 = egui::pos2(c.x + w_can_bot / 2.0, lid_y + h_can);
            let p4 = egui::pos2(c.x + w_can_top / 2.0, lid_y);
            painter.add(egui::Shape::line(vec![p1, p2, p3, p4], stroke));
        }

        Icon::DeleteLarge => {
            // Trash Can (centered and larger, for history items)
            let c = center;
            let lid_y = c.y - 4.0 * scale;
            let w_lid = 10.0 * scale;
            let w_can_top = 8.0 * scale;
            let w_can_bot = 6.0 * scale;
            let h_can = 9.0 * scale;

            // Lid line
            painter.line_segment(
                [
                    egui::pos2(c.x - w_lid / 2.0, lid_y),
                    egui::pos2(c.x + w_lid / 2.0, lid_y),
                ],
                stroke,
            );

            // Handle (small loop above lid)
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.0 * scale, lid_y),
                    egui::pos2(c.x - 1.0 * scale, lid_y - 1.0 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 1.0 * scale, lid_y - 1.0 * scale),
                    egui::pos2(c.x + 1.0 * scale, lid_y - 1.0 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.0 * scale, lid_y - 1.0 * scale),
                    egui::pos2(c.x + 1.0 * scale, lid_y),
                ],
                stroke,
            );

            // Can Body (Trapezoid)
            let p1 = egui::pos2(c.x - w_can_top / 2.0, lid_y);
            let p2 = egui::pos2(c.x - w_can_bot / 2.0, lid_y + h_can);
            let p3 = egui::pos2(c.x + w_can_bot / 2.0, lid_y + h_can);
            let p4 = egui::pos2(c.x + w_can_top / 2.0, lid_y);
            painter.add(egui::Shape::line(vec![p1, p2, p3, p4], stroke));
        }

        Icon::Folder => {
            // Folder Icon
            let w = 14.0 * scale;
            let h = 10.0 * scale;
            let body_rect = egui::Rect::from_center_size(
                center + egui::vec2(0.0, 1.0 * scale),
                egui::vec2(w, h),
            );

            // Tab (top left)
            let tab_w = 6.0 * scale;
            let tab_h = 2.0 * scale;

            let p1 = body_rect.left_top();
            let p2 = body_rect.left_bottom();
            let p3 = body_rect.right_bottom();
            let p4 = body_rect.right_top();
            let p5 = body_rect.left_top() + egui::vec2(tab_w, 0.0);
            let p6 = body_rect.left_top() + egui::vec2(tab_w, -tab_h);
            let p7 = body_rect.left_top() + egui::vec2(0.0, -tab_h);

            painter.add(egui::Shape::line(
                vec![p7, p1, p2, p3, p4, p5, p6, p7],
                stroke,
            ));
        }

        Icon::Copy => {
            // Two overlapping rectangles - REDUCED SIZE to match Trashcan
            let w = 7.0 * scale;
            let h = 9.0 * scale;
            let offset = 2.0 * scale;

            // Back rect (Top Left)
            let back_rect = egui::Rect::from_center_size(
                center - egui::vec2(offset / 2.0, offset / 2.0),
                egui::vec2(w, h),
            );
            painter.rect_stroke(back_rect, 1.0 * scale, stroke, egui::StrokeKind::Middle);

            // Front rect (Bottom Right) - Filled to cover back lines
            let front_rect =
                egui::Rect::from_center_size(center + egui::vec2(offset, offset), egui::vec2(w, h));
            painter.rect_filled(
                front_rect,
                1.0 * scale,
                painter.ctx().style().visuals.panel_fill,
            );
            painter.rect_stroke(front_rect, 1.0 * scale, stroke, egui::StrokeKind::Middle);
        }

        Icon::CopySmall => {
            // Two overlapping rectangles - MINI SIZE for preset buttons
            let w = 5.0 * scale;
            let h = 6.5 * scale;
            let offset = 1.2 * scale;

            // Back rect (Top Left)
            let back_rect = egui::Rect::from_center_size(
                center - egui::vec2(offset / 2.0, offset / 2.0),
                egui::vec2(w, h),
            );
            painter.rect_stroke(back_rect, 0.8 * scale, stroke, egui::StrokeKind::Middle);

            // Front rect (Bottom Right) - Filled to cover back lines
            let front_rect =
                egui::Rect::from_center_size(center + egui::vec2(offset, offset), egui::vec2(w, h));
            painter.rect_filled(
                front_rect,
                0.8 * scale,
                painter.ctx().style().visuals.panel_fill,
            );
            painter.rect_stroke(front_rect, 0.8 * scale, stroke, egui::StrokeKind::Middle);
        }

        Icon::Close => {
            // 'X' Icon
            let sz = 5.0 * scale;
            let p1 = center - egui::vec2(sz, sz);
            let p2 = center + egui::vec2(sz, sz);
            let p3 = center - egui::vec2(sz, -sz);
            let p4 = center + egui::vec2(sz, -sz);

            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p3, p4], stroke);
        }

        // All remaining icons are handled in paint_extra.rs
        _ => {
            super::paint_extra::paint_extra_icons(painter, center, icon, color, scale, stroke);
        }
    }
}

// --- MATH HELPERS ---

pub(super) fn lerp(a: egui::Pos2, b: egui::Pos2, t: f32) -> egui::Pos2 {
    egui::pos2(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn lerp_quadratic(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, t: f32) -> egui::Pos2 {
    let l1 = lerp(p0, p1, t);
    let l2 = lerp(p1, p2, t);
    lerp(l1, l2, t)
}

pub(super) fn bezier_points(
    p0: egui::Pos2,
    p1: egui::Pos2,
    p2: egui::Pos2,
    segments: usize,
) -> Vec<egui::Pos2> {
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        points.push(lerp_quadratic(p0, p1, p2, t));
    }
    points
}
