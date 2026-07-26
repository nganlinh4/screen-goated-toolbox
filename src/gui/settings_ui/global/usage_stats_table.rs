use crate::gui::model_performance::PREFIX_WIDTH;
use crate::gui::theme::{AppTheme, blend};
use eframe::egui;

pub(super) const CELL_GAP: f32 = 6.0;
pub(super) const WIDE_STATUS_COLUMN_WIDTH: f32 = 190.0;
pub(super) const WIDE_NAME_COLUMN_WIDTH: f32 = 118.0;
pub(super) const PROVIDER_NAME_COLUMN_WIDTH: f32 = 110.0;

const MIN_ID_COLUMN_WIDTH: f32 = 40.0;
const HEADER_ICON_COLUMN_WIDTH: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EndpointColumns {
    pub prefix: f32,
    pub name: f32,
    pub id: f32,
    pub status: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EndpointColumnRects {
    pub prefix: egui::Rect,
    pub name: egui::Rect,
    pub id: egui::Rect,
    pub status: egui::Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ProviderHeaderRects {
    pub icon: egui::Rect,
    pub name: egui::Rect,
    pub link: egui::Rect,
}

pub(super) fn endpoint_columns(row_width: f32) -> EndpointColumns {
    let status = if row_width >= 420.0 {
        WIDE_STATUS_COLUMN_WIDTH
    } else {
        (row_width * 0.36).clamp(100.0, WIDE_STATUS_COLUMN_WIDTH)
    };
    let identity_width = (row_width - PREFIX_WIDTH - status - CELL_GAP * 3.0).max(0.0);
    let maximum_name = (identity_width - MIN_ID_COLUMN_WIDTH).max(0.0);
    let preferred_name = if row_width >= 420.0 {
        WIDE_NAME_COLUMN_WIDTH
    } else {
        (identity_width * 0.52).clamp(0.0, WIDE_NAME_COLUMN_WIDTH)
    };
    let name = preferred_name.min(maximum_name);
    EndpointColumns {
        prefix: PREFIX_WIDTH,
        name,
        id: (identity_width - name).max(0.0),
        status,
    }
}

impl EndpointColumns {
    pub(super) fn rects(self, row: egui::Rect) -> EndpointColumnRects {
        let mut x = row.left();
        let prefix = column_rect(row, x, self.prefix);
        x = prefix.right() + CELL_GAP;
        let name = column_rect(row, x, self.name);
        x = name.right() + CELL_GAP;
        let id = column_rect(row, x, self.id);
        x = id.right() + CELL_GAP;
        let status = column_rect(row, x, self.status);
        EndpointColumnRects {
            prefix,
            name,
            id,
            status,
        }
    }
}

pub(super) fn provider_header_rects(row: egui::Rect) -> ProviderHeaderRects {
    let mut x = row.left();
    let icon = column_rect(row, x, HEADER_ICON_COLUMN_WIDTH);
    x = icon.right() + CELL_GAP;
    let name = column_rect(row, x, PROVIDER_NAME_COLUMN_WIDTH);
    x = name.right() + CELL_GAP;
    let link = column_rect(row, x, (row.right() - x).max(0.0));
    ProviderHeaderRects { icon, name, link }
}

pub(super) fn cell_ui(parent: &mut egui::Ui, rect: egui::Rect, layout: egui::Layout) -> egui::Ui {
    let mut child = parent.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    child.set_clip_rect(parent.clip_rect().intersect(rect));
    child
}

pub(super) fn render_status_strip(
    ui: &mut egui::Ui,
    label: &str,
    status: String,
    accent: egui::Color32,
    theme: &AppTheme,
    row_height: f32,
    status_font_size: f32,
) {
    egui::Frame::new()
        .fill(blend(theme.dialog_surface(), accent, 0.08))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            let row_width = ui.available_width();
            let columns = endpoint_columns(row_width);
            let (row_rect, _) =
                ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::hover());
            let rects = columns.rects(row_rect);
            let label_rect = egui::Rect::from_min_max(
                rects.name.min,
                egui::pos2(rects.id.right(), rects.id.bottom()),
            );
            let mut label_ui = cell_ui(
                ui,
                label_rect,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            label_ui
                .add(
                    egui::Label::new(egui::RichText::new(label).size(9.5).strong().color(accent))
                        .truncate(),
                )
                .on_hover_text(label);

            let mut status_ui = cell_ui(
                ui,
                rects.status,
                egui::Layout::left_to_right(egui::Align::Center),
            );
            status_ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(&status)
                            .monospace()
                            .size(status_font_size)
                            .color(accent),
                    )
                    .truncate(),
                )
                .on_hover_text(status);
        });
}

fn column_rect(row: egui::Rect, x: f32, width: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(x, row.top()),
        egui::vec2(width.max(0.0), row.height()),
    )
}
