use std::sync::LazyLock;
use std::time::{Duration, Instant};

use eframe::egui;
use parking_lot::Mutex;

use crate::APP;
use crate::gui::icons::{self, Icon};
use crate::gui::theme::AppTheme;

use super::data::PopupSnapshot;
use super::layout::{MAIN_HEIGHT, MAIN_WIDTH, PopupPlacement};

mod flyout;

const ROW_LEFT: f32 = 4.0;
const ROW_WIDTH: f32 = 232.0;
const ROW_HEIGHT: f32 = 32.0;
const ICON_SIZE: f32 = 16.0;
const RESTORE_HIDE_DELAY: Duration = Duration::from_millis(90);

static RUNTIME: LazyLock<Mutex<PopupRuntime>> =
    LazyLock::new(|| Mutex::new(PopupRuntime::default()));

pub(super) fn begin_generation(generation: u64) -> bool {
    let mut runtime = RUNTIME.lock();
    if runtime.generation == generation {
        return false;
    }
    runtime.reset(generation);
    true
}

#[derive(Default)]
struct PopupRuntime {
    generation: u64,
    restore_expanded: bool,
    flyout_hovered: bool,
    collapse_at: Option<Instant>,
}

impl PopupRuntime {
    fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.restore_expanded = false;
        self.flyout_hovered = false;
        self.collapse_at = None;
    }
}

#[derive(Clone, Copy)]
enum Action {
    Settings,
    ToggleBubble,
    StopTts,
    Restore(usize),
    Quit,
}

struct PaintResult {
    action: Option<Action>,
    restore_hovered: bool,
}

struct RowSpec<'a> {
    id: &'static str,
    y: f32,
    label: &'a str,
    icon: Icon,
    active: bool,
    disabled: bool,
    chevron: bool,
}

pub(super) fn render_main(ui: &mut egui::Ui, generation: u64) {
    let context = ui.ctx().clone();
    if ui.input(|input| input.viewport().close_requested() || input.key_pressed(egui::Key::Escape))
    {
        super::close_from_viewport(&context, "escape-or-window-close");
        return;
    }

    let snapshot = super::data::snapshot();
    let now = Instant::now();
    let painted = paint_popup(ui, &snapshot);
    let flyout_changed = {
        let mut runtime = RUNTIME.lock();
        if runtime.generation != generation {
            runtime.reset(generation);
        }
        let was_expanded = runtime.restore_expanded;
        let flyout_hovered = runtime.flyout_hovered;
        update_flyout_state(
            &mut runtime,
            painted.restore_hovered,
            flyout_hovered,
            snapshot.restore_options.is_empty(),
            now,
        );
        was_expanded != runtime.restore_expanded
    };
    if flyout_changed {
        super::request_flyout_repaint();
    }
    if let Some(action) = painted.action {
        perform_action(action, &context);
        return;
    }
    context.request_repaint_after(Duration::from_millis(50));
}

pub(super) fn prepaint_main(ui: &mut egui::Ui) {
    let snapshot = super::data::snapshot();
    let _ = paint_popup(ui, &snapshot);
}

pub(super) fn flyout_expanded(generation: u64) -> bool {
    let runtime = RUNTIME.lock();
    runtime.generation == generation && runtime.restore_expanded
}

pub(super) fn render_flyout(ui: &mut egui::Ui, placement: PopupPlacement, generation: u64) {
    let context = ui.ctx().clone();
    if ui.input(|input| input.viewport().close_requested() || input.key_pressed(egui::Key::Escape))
    {
        super::close_from_viewport(&context, "flyout-escape-or-window-close");
        return;
    }

    let snapshot = super::data::snapshot();
    let painted = flyout::paint(ui, placement, &snapshot, AppTheme::from_ui(ui));
    let hover_changed = {
        let mut runtime = RUNTIME.lock();
        if runtime.generation != generation {
            runtime.reset(generation);
        }
        let changed = runtime.flyout_hovered != painted.hovered;
        runtime.flyout_hovered = painted.hovered;
        changed
    };
    if hover_changed {
        super::request_main_repaint();
    }
    if let Some(action) = painted.action {
        perform_action(action, &context);
        return;
    }
    context.request_repaint_after(Duration::from_millis(50));
}

pub(super) fn prepaint_flyout(ui: &mut egui::Ui, placement: PopupPlacement) {
    let snapshot = super::data::snapshot();
    let _ = flyout::paint(ui, placement, &snapshot, AppTheme::from_ui(ui));
}

fn update_flyout_state(
    runtime: &mut PopupRuntime,
    restore_hovered: bool,
    flyout_hovered: bool,
    restore_empty: bool,
    now: Instant,
) {
    if restore_empty {
        runtime.restore_expanded = false;
        runtime.collapse_at = None;
    } else if restore_hovered || flyout_hovered {
        runtime.restore_expanded = true;
        runtime.collapse_at = None;
    } else if runtime.restore_expanded {
        let deadline = runtime
            .collapse_at
            .get_or_insert_with(|| now + RESTORE_HIDE_DELAY);
        if now >= *deadline {
            runtime.restore_expanded = false;
            runtime.collapse_at = None;
        }
    }
}

fn paint_popup(ui: &mut egui::Ui, snapshot: &PopupSnapshot) -> PaintResult {
    ui.set_min_size(egui::vec2(MAIN_WIDTH, MAIN_HEIGHT));
    let origin = ui.max_rect().min;
    let theme = AppTheme::from_ui(ui);
    let main = egui::Rect::from_min_size(origin, egui::vec2(MAIN_WIDTH, MAIN_HEIGHT));
    paint_card(ui.painter(), main, theme, ui.visuals().dark_mode);

    let rows = [4.0, 38.0, 72.0, 106.0, 148.0];
    let mut action = None;
    let settings = paint_row(
        ui,
        origin,
        RowSpec {
            id: "settings",
            y: rows[0],
            label: snapshot.labels.settings,
            icon: Icon::Settings,
            active: false,
            disabled: false,
            chevron: false,
        },
        theme,
    );
    if activated(ui, &settings) {
        action = Some(Action::Settings);
    }

    let bubble = paint_row(
        ui,
        origin,
        RowSpec {
            id: "bubble",
            y: rows[1],
            label: snapshot.labels.bubble,
            icon: Icon::Star,
            active: snapshot.bubble_active,
            disabled: false,
            chevron: false,
        },
        theme,
    );
    if activated(ui, &bubble) {
        action = Some(Action::ToggleBubble);
    }

    let stop_tts = paint_row(
        ui,
        origin,
        RowSpec {
            id: "stop-tts",
            y: rows[2],
            label: snapshot.labels.stop_tts,
            icon: Icon::SpeakerDisabled,
            active: false,
            disabled: snapshot.tts_disabled,
            chevron: false,
        },
        theme,
    );
    if !snapshot.tts_disabled && activated(ui, &stop_tts) {
        action = Some(Action::StopTts);
    }

    let restore_disabled = snapshot.restore_options.is_empty();
    let restore = paint_row(
        ui,
        origin,
        RowSpec {
            id: "restore",
            y: rows[3],
            label: snapshot.labels.restore,
            icon: Icon::History,
            active: false,
            disabled: restore_disabled,
            chevron: true,
        },
        theme,
    );

    ui.painter().line_segment(
        [
            origin + egui::vec2(14.0, 143.5),
            origin + egui::vec2(MAIN_WIDTH - 14.0, 143.5),
        ],
        egui::Stroke::new(1.0, theme.on_surface_variant().gamma_multiply(0.22)),
    );

    let quit = paint_row(
        ui,
        origin,
        RowSpec {
            id: "quit",
            y: rows[4],
            label: snapshot.labels.quit,
            icon: Icon::Logout,
            active: false,
            disabled: false,
            chevron: false,
        },
        theme,
    );
    if activated(ui, &quit) {
        action = Some(Action::Quit);
    }

    PaintResult {
        action,
        restore_hovered: !restore_disabled && restore.hovered(),
    }
}

fn paint_row(
    ui: &mut egui::Ui,
    origin: egui::Pos2,
    spec: RowSpec<'_>,
    theme: AppTheme,
) -> egui::Response {
    let rect = egui::Rect::from_min_size(
        origin + egui::vec2(ROW_LEFT, spec.y),
        egui::vec2(ROW_WIDTH, ROW_HEIGHT),
    );
    let sense = if spec.disabled {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let response = ui.interact(rect, ui.id().with(spec.id), sense);
    if !spec.disabled && (response.hovered() || response.has_focus()) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), theme.neutral_fill());
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, theme.accent_fill()),
            egui::StrokeKind::Inside,
        );
    }

    let opacity = if spec.disabled { 0.38 } else { 0.82 };
    let content_color = theme.on_surface().gamma_multiply(opacity);
    let icon_rect = egui::Rect::from_min_size(
        origin + egui::vec2(14.0, spec.y + (ROW_HEIGHT - ICON_SIZE) * 0.5),
        egui::vec2(ICON_SIZE, ICON_SIZE),
    );
    icons::paint_icon(ui.painter(), icon_rect, spec.icon, content_color);

    let trailing_width = if spec.active || spec.chevron {
        30.0
    } else {
        8.0
    };
    let text_left = origin.x + 42.0;
    let text_right = origin.x + MAIN_WIDTH - trailing_width;
    paint_elided_text(
        ui.painter(),
        spec.label,
        egui::pos2(text_left, origin.y + spec.y + ROW_HEIGHT * 0.5),
        text_right - text_left,
        13.0,
        content_color,
    );

    if spec.active {
        let check_rect = egui::Rect::from_center_size(
            origin + egui::vec2(218.0, spec.y + ROW_HEIGHT * 0.5),
            egui::vec2(16.0, 16.0),
        );
        icons::paint_icon(ui.painter(), check_rect, Icon::Check, theme.on_surface());
    } else if spec.chevron {
        let arrow_rect = egui::Rect::from_center_size(
            origin + egui::vec2(218.0, spec.y + ROW_HEIGHT * 0.5),
            egui::vec2(14.0, 14.0),
        );
        icons::paint_icon(
            ui.painter(),
            arrow_rect,
            Icon::ArrowRight,
            content_color.gamma_multiply(0.78),
        );
    }
    response
}

fn paint_card(painter: &egui::Painter, rect: egui::Rect, theme: AppTheme, dark_mode: bool) {
    let outline = if dark_mode {
        egui::Color32::from_gray(69)
    } else {
        egui::Color32::from_gray(220)
    };
    painter.rect(
        rect,
        egui::CornerRadius::same(8),
        theme.dialog_surface(),
        egui::Stroke::new(1.0, outline),
        egui::StrokeKind::Inside,
    );
}

fn paint_elided_text(
    painter: &egui::Painter,
    text: &str,
    left_center: egui::Pos2,
    max_width: f32,
    size: f32,
    color: egui::Color32,
) {
    let font = egui::FontId::proportional(size);
    let fitted = elide(painter, text, &font, color, max_width);
    painter.text(left_center, egui::Align2::LEFT_CENTER, fitted, font, color);
}

fn elide(
    painter: &egui::Painter,
    text: &str,
    font: &egui::FontId,
    color: egui::Color32,
    max_width: f32,
) -> String {
    if painter
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x
        <= max_width
    {
        return text.to_owned();
    }
    let mut characters = text.chars().collect::<Vec<_>>();
    while !characters.is_empty() {
        characters.pop();
        let candidate = format!("{}…", characters.iter().collect::<String>());
        if painter
            .layout_no_wrap(candidate.clone(), font.clone(), color)
            .size()
            .x
            <= max_width
        {
            return candidate;
        }
    }
    "…".to_owned()
}

fn activated(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.clicked()
        || (response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            }))
}

fn perform_action(action: Action, context: &egui::Context) {
    match action {
        Action::Settings => {
            super::close_from_viewport(context, "open-settings");
            crate::gui::signal_restore_window();
        }
        Action::ToggleBubble => toggle_bubble(context),
        Action::StopTts => {
            crate::api::tts::TTS_MANAGER.stop();
            super::close_from_viewport(context, "stop-tts");
        }
        Action::Restore(batch_count) => {
            super::close_from_viewport(context, "restore-result");
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                let _ = crate::overlay::result::restore_recent(batch_count);
            });
        }
        Action::Quit => {
            super::close_from_viewport(context, "quit");
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(50));
                crate::gui::app::exit_app();
            });
        }
    }
}

fn toggle_bubble(context: &egui::Context) {
    let enabled = APP.lock().ok().map(|mut app| {
        app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
        let enabled = app.config.show_favorite_bubble;
        crate::config::save_config(&app.config);
        enabled
    });
    match enabled {
        Some(true) => {
            crate::overlay::favorite_bubble::show_favorite_bubble();
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(150));
                crate::overlay::favorite_bubble::trigger_blink_animation();
            });
        }
        Some(false) => crate::overlay::favorite_bubble::hide_favorite_bubble(),
        None => {}
    }
    context.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_flyout_bridges_the_pointer_gap_before_collapsing() {
        let start = Instant::now();
        let mut runtime = PopupRuntime::default();
        update_flyout_state(&mut runtime, true, false, false, start);
        assert!(runtime.restore_expanded);

        update_flyout_state(
            &mut runtime,
            false,
            false,
            false,
            start + Duration::from_millis(10),
        );
        update_flyout_state(
            &mut runtime,
            false,
            true,
            false,
            start + Duration::from_millis(80),
        );
        assert!(runtime.restore_expanded);
        assert!(runtime.collapse_at.is_none());
    }

    #[test]
    fn restore_flyout_collapses_after_the_hover_delay() {
        let start = Instant::now();
        let mut runtime = PopupRuntime::default();
        update_flyout_state(&mut runtime, true, false, false, start);
        update_flyout_state(
            &mut runtime,
            false,
            false,
            false,
            start + Duration::from_millis(10),
        );
        update_flyout_state(
            &mut runtime,
            false,
            false,
            false,
            start + Duration::from_millis(101),
        );
        assert!(!runtime.restore_expanded);
        assert!(runtime.collapse_at.is_none());
    }

    #[test]
    fn empty_restore_history_forces_the_flyout_closed() {
        let mut runtime = PopupRuntime {
            restore_expanded: true,
            collapse_at: Some(Instant::now()),
            ..Default::default()
        };
        update_flyout_state(&mut runtime, true, true, true, Instant::now());
        assert!(!runtime.restore_expanded);
        assert!(runtime.collapse_at.is_none());
    }
}
