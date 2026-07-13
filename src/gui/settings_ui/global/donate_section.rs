//! Donation section shown in global settings.
//!
//! Donations currently support Vietnamese donors only. For the Vietnamese locale the
//! section shows the VietQR bank-transfer details + actions; English/Korean locales show
//! a short "Vietnamese donors only" note instead. The QR is referenced by its VietQR
//! image URL (opened in the browser) rather than bundled as an asset.
//!
//! The section is collapsed by default — a `potted_plant` glyph + title on the left, an
//! expand chevron pinned to the right; the whole header row toggles it.

use crate::gui::icons::{self, Icon};
use crate::gui::locale::LocaleText;
use eframe::egui;

const DONATE_ACCOUNT: &str = "8850273958";
const DONATE_BANK_LINE: &str = "BIDV · NGUYEN BAO LINH · STK: 8850273958";
const VIETQR_URL: &str = "https://img.vietqr.io/image/970418-8850273958-compact2.png?accountName=NGUYEN%20BAO%20LINH&addInfo=Ung%20ho%20SGT";

pub fn render_donate_section_content(ui: &mut egui::Ui, text: &LocaleText) {
    let id = ui.make_persistent_id("sgt_donate_collapsing");
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let openness = state.openness(ui.ctx());

    let header_resp = ui
        .horizontal(|ui| {
            icons::draw_icon_static(ui, Icon::PottedPlant, Some(icons::ICON_MD));
            ui.label(
                egui::RichText::new(text.global_settings.donate_header)
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = ui.spacing().icon_width;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                let chevron = if openness < 0.5 {
                    Icon::ArrowDown
                } else {
                    Icon::ArrowUp
                };
                icons::paint_icon(
                    ui.painter(),
                    rect,
                    chevron,
                    ui.visuals().widgets.inactive.fg_stroke.color,
                );
            });
        })
        .response
        .interact(egui::Sense::click());

    if header_resp.clicked() {
        state.toggle(ui);
    }
    if header_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    state.store(ui.ctx());

    state.show_body_unindented(ui, |ui| {
        ui.add_space(6.0);
        ui.label(text.global_settings.donate_body);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(DONATE_BANK_LINE).strong());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Sao chép STK").clicked() {
                ui.ctx().copy_text(DONATE_ACCOUNT.to_string());
            }
            if ui.button("Mở mã VietQR").clicked() {
                let _ = open::that(VIETQR_URL);
            }
        });
        // EN/KO get an extra line clarifying the bank transfer is for Vietnamese donors.
        if !text.global_settings.donate_vietnamese {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(text.global_settings.donate_note)
                    .size(11.5)
                    .color(crate::gui::theme::AppTheme::from_ui(ui).on_surface_variant()),
            );
        }
    });
}
