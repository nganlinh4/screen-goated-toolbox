// --- ADDITIONAL ICON PAINTERS ---
// Contains paint functions for TextSelect, Speaker, Lightbulb, Realtime, Star, Sun/Moon,
// Device, DragHandle, History, Priority, Parakeet, Pointer, and window control icons.

use super::Icon;
use eframe::egui;
use std::f32::consts::PI;

use super::paint::{bezier_points, lerp};

pub(super) fn paint_extra_icons(
    painter: &egui::Painter,
    center: egui::Pos2,
    icon: Icon,
    color: egui::Color32,
    scale: f32,
    stroke: egui::Stroke,
) {
    match icon {
        Icon::TextSelect => {
            // Text with selection highlight/cursor - represents "select text" mode
            // Draw 3 horizontal lines (text lines) with middle one highlighted
            let line_w = 12.0 * scale;
            let line_gap = 4.0 * scale;
            let line_y1 = center.y - line_gap;
            let line_y2 = center.y;
            let line_y3 = center.y + line_gap;

            // Text lines
            painter.line_segment(
                [
                    egui::pos2(center.x - line_w / 2.0, line_y1),
                    egui::pos2(center.x + line_w / 2.0, line_y1),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - line_w / 2.0, line_y3),
                    egui::pos2(center.x + line_w / 2.0, line_y3),
                ],
                stroke,
            );

            // Highlighted middle line (thicker, representing selection)
            let highlight_stroke = egui::Stroke::new(3.0 * scale, color);
            painter.line_segment(
                [
                    egui::pos2(center.x - line_w / 2.0, line_y2),
                    egui::pos2(center.x + line_w / 2.0, line_y2),
                ],
                highlight_stroke,
            );

            // Cursor (vertical line with serifs at ends)
            let cursor_x = center.x + line_w / 2.0 + 2.0 * scale;
            let cursor_top = center.y - 5.0 * scale;
            let cursor_bot = center.y + 5.0 * scale;
            let serif_w = 1.5 * scale;
            painter.line_segment(
                [
                    egui::pos2(cursor_x, cursor_top),
                    egui::pos2(cursor_x, cursor_bot),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(cursor_x - serif_w, cursor_top),
                    egui::pos2(cursor_x + serif_w, cursor_top),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(cursor_x - serif_w, cursor_bot),
                    egui::pos2(cursor_x + serif_w, cursor_bot),
                ],
                stroke,
            );
        }

        Icon::Speaker => {
            // Speaker with sound waves - for device audio (system sound)
            let body_x = center.x - 3.0 * scale;
            let body_w = 4.0 * scale;
            let body_h = 6.0 * scale;
            let cone_w = 5.0 * scale;
            let cone_h = 10.0 * scale;

            // Rectangle (back of speaker)
            let rect = egui::Rect::from_center_size(
                egui::pos2(body_x - body_w / 2.0, center.y),
                egui::vec2(body_w, body_h),
            );
            painter.rect_stroke(rect, 0.5 * scale, stroke, egui::StrokeKind::Middle);

            // Cone (trapezoid)
            let cone_pts = vec![
                egui::pos2(body_x, center.y - body_h / 2.0),
                egui::pos2(body_x + cone_w, center.y - cone_h / 2.0),
                egui::pos2(body_x + cone_w, center.y + cone_h / 2.0),
                egui::pos2(body_x, center.y + body_h / 2.0),
            ];
            painter.add(egui::Shape::closed_line(cone_pts, stroke));

            // Sound waves (arcs)
            let wave_x = center.x + 4.0 * scale;
            let wave_r1 = 3.0 * scale;
            let wave_r2 = 5.5 * scale;

            // First wave
            let wave_segments = 8;
            let wave_angle = PI / 3.0;
            let mut wave1_pts = Vec::new();
            for i in 0..=wave_segments {
                let t = i as f32 / wave_segments as f32;
                let angle = -wave_angle + 2.0 * wave_angle * t;
                wave1_pts.push(egui::pos2(
                    wave_x + wave_r1 * angle.cos(),
                    center.y + wave_r1 * angle.sin(),
                ));
            }
            painter.add(egui::Shape::line(wave1_pts, stroke));

            // Second wave
            let mut wave2_pts = Vec::new();
            for i in 0..=wave_segments {
                let t = i as f32 / wave_segments as f32;
                let angle = -wave_angle + 2.0 * wave_angle * t;
                wave2_pts.push(egui::pos2(
                    wave_x + wave_r2 * angle.cos(),
                    center.y + wave_r2 * angle.sin(),
                ));
            }
            painter.add(egui::Shape::line(wave2_pts, stroke));
        }

        Icon::SpeakerDisabled => {
            // Speaker with NO sound waves and a cross (gray/disabled look)
            let body_x = center.x - 3.0 * scale;
            let body_w = 4.0 * scale;
            let body_h = 6.0 * scale;
            let cone_w = 5.0 * scale;
            let cone_h = 10.0 * scale;

            // Rectangle (back of speaker)
            let rect = egui::Rect::from_center_size(
                egui::pos2(body_x - body_w / 2.0, center.y),
                egui::vec2(body_w, body_h),
            );
            painter.rect_stroke(rect, 0.5 * scale, stroke, egui::StrokeKind::Middle);

            // Cone (trapezoid)
            let cone_pts = vec![
                egui::pos2(body_x, center.y - body_h / 2.0),
                egui::pos2(body_x + cone_w, center.y - cone_h / 2.0),
                egui::pos2(body_x + cone_w, center.y + cone_h / 2.0),
                egui::pos2(body_x, center.y + body_h / 2.0),
            ];
            painter.add(egui::Shape::closed_line(cone_pts, stroke));

            // Diagonal cross (indicating disabled)
            let cross_stroke = egui::Stroke::new(2.0 * scale, color);
            let cross_sz = 8.0 * scale;
            painter.line_segment(
                [
                    egui::pos2(center.x - cross_sz / 2.0, center.y - cross_sz / 2.0),
                    egui::pos2(center.x + cross_sz / 2.0, center.y + cross_sz / 2.0),
                ],
                cross_stroke,
            );
        }

        Icon::CopyDisabled => {
            // Copy icon with diagonal cross
            let w = 7.0 * scale;
            let h = 9.0 * scale;
            let offset = 2.0 * scale;

            // Back rect (Top Left)
            let back_rect = egui::Rect::from_center_size(
                center - egui::vec2(offset / 2.0, offset / 2.0),
                egui::vec2(w, h),
            );
            painter.rect_stroke(back_rect, 1.0 * scale, stroke, egui::StrokeKind::Middle);

            // Front rect (Bottom Right)
            let front_rect =
                egui::Rect::from_center_size(center + egui::vec2(offset, offset), egui::vec2(w, h));
            painter.rect_filled(
                front_rect,
                1.0 * scale,
                painter.ctx().style().visuals.panel_fill,
            );
            painter.rect_stroke(front_rect, 1.0 * scale, stroke, egui::StrokeKind::Middle);

            // Diagonal cross (indicating disabled)
            let cross_stroke = egui::Stroke::new(2.0 * scale, color);
            let cross_sz = 10.0 * scale;
            painter.line_segment(
                [
                    egui::pos2(center.x - cross_sz / 2.0, center.y - cross_sz / 2.0),
                    egui::pos2(center.x + cross_sz / 2.0, center.y + cross_sz / 2.0),
                ],
                cross_stroke,
            );
        }

        Icon::Lightbulb => {
            // Simple lightbulb icon using explicit coordinates
            let bulb_r = 4.5 * scale;
            let bulb_cy = center.y - 2.0 * scale;

            // 1. Draw bulb circle (full circle)
            painter.circle_stroke(egui::pos2(center.x, bulb_cy), bulb_r, stroke);

            // 2. Draw neck (two converging lines from bulb bottom to base)
            let neck_top_w = 3.0 * scale;
            let neck_bot_w = 2.0 * scale;
            let neck_top_y = bulb_cy + bulb_r;
            let neck_bot_y = neck_top_y + 3.0 * scale;

            // Left neck line
            painter.line_segment(
                [
                    egui::pos2(center.x - neck_top_w, neck_top_y),
                    egui::pos2(center.x - neck_bot_w, neck_bot_y),
                ],
                stroke,
            );
            // Right neck line
            painter.line_segment(
                [
                    egui::pos2(center.x + neck_top_w, neck_top_y),
                    egui::pos2(center.x + neck_bot_w, neck_bot_y),
                ],
                stroke,
            );

            // 3. Draw base (two horizontal lines)
            painter.line_segment(
                [
                    egui::pos2(center.x - neck_bot_w, neck_bot_y),
                    egui::pos2(center.x + neck_bot_w, neck_bot_y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - neck_bot_w * 0.7, neck_bot_y + 1.5 * scale),
                    egui::pos2(center.x + neck_bot_w * 0.7, neck_bot_y + 1.5 * scale),
                ],
                stroke,
            );

            // 4. Draw rays (3 lines going up from top of bulb)
            let ray_start_y = bulb_cy - bulb_r - 1.5 * scale;
            let ray_len = 2.5 * scale;

            // Center ray (straight up)
            painter.line_segment(
                [
                    egui::pos2(center.x, ray_start_y),
                    egui::pos2(center.x, ray_start_y - ray_len),
                ],
                stroke,
            );
            // Left ray (diagonal)
            painter.line_segment(
                [
                    egui::pos2(center.x - 2.5 * scale, ray_start_y + 1.0 * scale),
                    egui::pos2(center.x - 4.0 * scale, ray_start_y - ray_len + 1.5 * scale),
                ],
                stroke,
            );
            // Right ray (diagonal)
            painter.line_segment(
                [
                    egui::pos2(center.x + 2.5 * scale, ray_start_y + 1.0 * scale),
                    egui::pos2(center.x + 4.0 * scale, ray_start_y - ray_len + 1.5 * scale),
                ],
                stroke,
            );
        }

        Icon::Realtime => {
            // Realtime waveform icon - audio oscilloscope pattern
            let y_center = center.y;
            let wave_stroke = egui::Stroke::new(2.0 * scale, color);

            // Left flat segment
            let left_start = center.x - 10.0 * scale;
            let left_end = center.x - 7.0 * scale;
            painter.line_segment(
                [
                    egui::pos2(left_start, y_center),
                    egui::pos2(left_end, y_center),
                ],
                wave_stroke,
            );

            // Waveform points
            let wave_pts = vec![
                egui::pos2(left_end, y_center),
                egui::pos2(center.x - 5.5 * scale, y_center - 3.0 * scale),
                egui::pos2(center.x - 3.5 * scale, y_center + 7.0 * scale),
                egui::pos2(center.x, y_center - 7.0 * scale),
                egui::pos2(center.x + 3.5 * scale, y_center + 3.0 * scale),
                egui::pos2(center.x + 5.5 * scale, y_center),
            ];
            painter.add(egui::Shape::line(wave_pts, wave_stroke));

            // Right flat segment
            let right_start = center.x + 5.5 * scale;
            let right_end = center.x + 10.0 * scale;
            painter.line_segment(
                [
                    egui::pos2(right_start, y_center),
                    egui::pos2(right_end, y_center),
                ],
                wave_stroke,
            );
        }

        Icon::Star => {
            // 5-pointed star outline - SMALL variant to match CopySmall/Delete
            let outer_r = 5.5 * scale;
            let inner_r = 2.4 * scale;
            let mut points = Vec::new();

            for i in 0..10 {
                let angle = (i as f32 * PI / 5.0) - PI / 2.0;
                let r = if i % 2 == 0 { outer_r } else { inner_r };
                points.push(egui::pos2(
                    center.x + r * angle.cos(),
                    center.y + r * angle.sin(),
                ));
            }
            points.push(points[0]);
            painter.add(egui::Shape::line(points, stroke));
        }

        Icon::StarFilled => {
            // 5-pointed star filled with gold color, soft rounded corners
            let outer_r = 6.0 * scale;
            let inner_r = 2.6 * scale;
            let gold = egui::Color32::from_rgb(255, 193, 7);

            // 1. Calculate the 10 raw vertices
            let mut raw_points = Vec::with_capacity(10);
            for i in 0..10 {
                let angle = (i as f32 * PI / 5.0) - PI / 2.0;
                let r = if i % 2 == 0 { outer_r } else { inner_r };
                raw_points.push(egui::pos2(
                    center.x + r * angle.cos(),
                    center.y + r * angle.sin(),
                ));
            }

            // 2. Generate rounded path
            let mut path_points = Vec::new();
            let round_ratio = 0.35;

            for i in 0..10 {
                let p = raw_points[i];

                if i % 2 == 0 {
                    // Tip: Replace sharp vertex with a curve
                    let p_prev = raw_points[(i + 9) % 10];
                    let p_next = raw_points[(i + 1) % 10];

                    let p_start = lerp(p, p_prev, round_ratio);
                    let p_end = lerp(p, p_next, round_ratio);

                    let curve = bezier_points(p_start, p, p_end, 5);
                    path_points.extend(curve);
                } else {
                    // Valley: Keep sharp
                    path_points.push(p);
                }
            }

            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: path_points,
                closed: true,
                fill: gold,
                stroke: egui::Stroke::new(1.0 * scale, gold).into(),
            }));
        }

        Icon::Sun => {
            painter.circle_stroke(center, 4.0 * scale, stroke);
            for i in 0..8 {
                let angle = (i as f32 * 45.0).to_radians();
                let dir = egui::vec2(angle.cos(), angle.sin());
                let start = center + dir * 6.5 * scale;
                let end = center + dir * 9.0 * scale;
                painter.line_segment([start, end], stroke);
            }
        }

        Icon::Moon => {
            let r = 7.0 * scale;
            let offset = 3.5 * scale;
            painter.circle_filled(center, r, color);
            painter.circle_filled(
                center + egui::vec2(offset, -offset * 0.8),
                r * 0.85,
                painter.ctx().style().visuals.panel_fill,
            );
        }

        Icon::Device => {
            // Monitor / PC Icon
            let w = 12.0 * scale;
            let h = 8.0 * scale;

            // Screen rect
            let screen_rect =
                egui::Rect::from_center_size(center + egui::vec2(0.0, -scale), egui::vec2(w, h));
            painter.rect_stroke(screen_rect, 1.0 * scale, stroke, egui::StrokeKind::Middle);

            // Stand
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y + 3.0 * scale),
                    egui::pos2(center.x, center.y + 6.0 * scale),
                ],
                stroke,
            );

            // Base feet
            painter.line_segment(
                [
                    egui::pos2(center.x - 3.0 * scale, center.y + 6.0 * scale),
                    egui::pos2(center.x + 3.0 * scale, center.y + 6.0 * scale),
                ],
                stroke,
            );
        }

        Icon::DragHandle => {
            // 6 dots (2 columns of 3) for drag handles
            let w_gap = 4.0 * scale;
            let h_gap = 4.0 * scale;
            let r = 1.0 * scale;

            for col in 0..2 {
                for row in -1..=1 {
                    let cx = center.x + (col as f32 - 0.5) * w_gap;
                    let cy = center.y + row as f32 * h_gap;
                    painter.circle_filled(egui::pos2(cx, cy), r, color);
                }
            }
        }

        Icon::History => {
            // Clock icon
            let r = 7.0 * scale;
            painter.circle_stroke(center, r, stroke);

            // Hands
            painter.line_segment([center, center + egui::vec2(4.0 * scale, 0.0)], stroke);
            painter.line_segment([center, center + egui::vec2(0.0, -5.0 * scale)], stroke);
        }

        Icon::Priority => {
            // Ranked list / priority chain icon
            let row_offsets = [-5.0, 0.0, 5.0];
            let dot_radii = [1.8, 1.4, 1.2];
            let line_lengths = [9.0, 7.0, 5.5];

            for ((y_offset, dot_radius), line_len) in
                row_offsets.into_iter().zip(dot_radii).zip(line_lengths)
            {
                let y = center.y + y_offset * scale;
                let dot_center = egui::pos2(center.x - 6.5 * scale, y);
                painter.circle_filled(dot_center, dot_radius * scale, color);
                painter.line_segment(
                    [
                        egui::pos2(center.x - 3.5 * scale, y),
                        egui::pos2(center.x + line_len * scale, y),
                    ],
                    stroke,
                );
            }
        }

        Icon::Parakeet => {
            // Bird Head (Parakeet) - Minimalist profile
            let r_head = 7.0 * scale;
            painter.circle_filled(center, r_head, color);

            // Eye (Contrast color - usually background fill)
            let eye_pos = center + egui::vec2(2.0, -2.0) * scale;
            painter.circle_filled(
                eye_pos,
                2.0 * scale,
                painter.ctx().style().visuals.panel_fill,
            );

            // Beak (Triangle on right)
            let beak_pts = vec![
                center + egui::vec2(5.0 * scale, -scale),
                center + egui::vec2(11.0 * scale, 3.0 * scale),
                center + egui::vec2(4.0 * scale, 5.0 * scale),
            ];
            painter.add(egui::Shape::convex_polygon(beak_pts, color, stroke));
        }

        Icon::Pointer => {
            // Mouse pointer/cursor arrow icon
            let a = center + egui::vec2(-5.0 * scale, -9.0 * scale);
            let b = center + egui::vec2(-5.0 * scale, 4.0 * scale);
            let c = center + egui::vec2(-2.0 * scale, 1.5 * scale);
            let d = center + egui::vec2(0.5 * scale, 7.0 * scale);
            let e = center + egui::vec2(3.0 * scale, 5.0 * scale);
            let f = center + egui::vec2(-0.5 * scale, 0.0 * scale);
            let g = center + egui::vec2(2.0 * scale, -3.0 * scale);

            // Fill: concave shape decomposed into convex triangles
            for tri in [[a, b, c], [c, d, e], [c, e, f], [a, c, f], [a, f, g]] {
                painter.add(egui::Shape::convex_polygon(
                    tri.to_vec(),
                    color,
                    egui::Stroke::NONE,
                ));
            }

            // Outline
            let outline = [a, b, c, d, e, f, g, a];
            for w in outline.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
        }

        Icon::Minimize => {
            let h_line = 0.5 * scale;
            let w = 6.0 * scale;
            painter.line_segment(
                [
                    center - egui::vec2(w, -h_line),
                    center + egui::vec2(w, h_line),
                ],
                stroke,
            );
        }

        Icon::Maximize => {
            let sz = 6.0 * scale;
            let rect = egui::Rect::from_center_size(center, egui::vec2(sz * 2.0, sz * 2.0));
            painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
        }

        Icon::Restore => {
            let sz = 5.0 * scale;
            let offset = 2.0 * scale;

            // Back rect
            let rect_back = egui::Rect::from_center_size(
                center + egui::vec2(offset, -offset),
                egui::vec2(sz * 2.0, sz * 2.0),
            );
            painter.rect_stroke(rect_back, 0.0, stroke, egui::StrokeKind::Middle);

            // Front rect
            let rect_front = egui::Rect::from_center_size(
                center + egui::vec2(-offset, offset),
                egui::vec2(sz * 2.0, sz * 2.0),
            );
            painter.rect_filled(
                rect_front.expand(stroke.width / 2.0),
                0.0,
                painter.ctx().style().visuals.panel_fill,
            );
            painter.rect_stroke(rect_front, 0.0, stroke, egui::StrokeKind::Middle);
        }

        // Icons handled by paint.rs — should never reach here
        _ => {}
    }
}
