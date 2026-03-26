use eframe::egui;

pub(super) fn render_transcript(
    ui: &mut egui::Ui,
    full: &str,
    split_pos: usize,
    font: &egui::FontId,
) {
    let split_idx = split_pos.min(full.len());
    let split_idx = if full.is_char_boundary(split_idx) {
        split_idx
    } else {
        full.char_indices()
            .take_while(|(i, _)| *i < split_idx)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0)
    };

    let committed = full[..split_idx].trim_end();
    let uncommitted = full[split_idx..].trim_start();
    let dark_mode = ui.visuals().dark_mode;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if !committed.is_empty() {
            ui.label(
                egui::RichText::new(committed)
                    .font(font.clone())
                    .color(get_text_color(true, dark_mode)),
            );
        }
        if !uncommitted.is_empty() {
            if !committed.is_empty() {
                ui.label(" ");
            }
            let color = if dark_mode {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            };
            ui.label(
                egui::RichText::new(uncommitted)
                    .font(font.clone())
                    .color(color)
                    .italics(),
            );
        }
    });
}

pub(super) fn render_translation(
    ui: &mut egui::Ui,
    segments: &[String],
    uncommitted: &str,
    font: &egui::FontId,
) {
    let uncommitted = uncommitted.trim_start();
    let dark_mode = ui.visuals().dark_mode;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        for (i, segment) in segments.iter().enumerate() {
            let color = get_segment_color(i, dark_mode);
            ui.label(egui::RichText::new(segment).font(font.clone()).color(color));
        }

        if !uncommitted.is_empty() {
            // Uncommitted text color
            let color = if dark_mode {
                egui::Color32::YELLOW
            } else {
                egui::Color32::from_rgb(200, 100, 0)
            }; // Dark Orange for light mode
            ui.label(
                egui::RichText::new(uncommitted)
                    .font(font.clone())
                    .color(color)
                    .italics(),
            );
        }
    });
}

fn get_segment_color(index: usize, dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        if index.is_multiple_of(2) {
            egui::Color32::from_gray(230)
        } else {
            egui::Color32::from_rgb(180, 210, 255) // Light Blue
        }
    } else if index.is_multiple_of(2) {
        egui::Color32::from_gray(30) // Dark Gray (almost black) for readability
    } else {
        egui::Color32::from_rgb(0, 80, 200) // Deep Blue
    }
}

fn get_text_color(is_committed: bool, dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        if is_committed {
            egui::Color32::from_gray(200)
        } else {
            egui::Color32::WHITE
        }
    } else if is_committed {
        egui::Color32::from_gray(60)
    } else {
        egui::Color32::BLACK
    }
}
