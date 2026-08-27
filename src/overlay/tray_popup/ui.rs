use std::sync::LazyLock;
use std::time::{Duration, Instant};

use eframe::egui;
use parking_lot::Mutex;

use crate::APP;
use crate::gui::icons::{self, Icon};
use crate::gui::theme::AppTheme;

use super::data::PopupSnapshot;
use super::layout::{
    FLYOUT_GAP, FLYOUT_WIDTH, MAIN_HEIGHT, MAIN_WIDTH, OPTION_HEIGHT, PopupPlacement,
};

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
    prepared_generation: u64,
    restore_expanded: bool,
    collapse_at: Option<Instant>,
    shaped_expanded: Option<bool>,
}

impl PopupRuntime {
    fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.prepared_generation = 0;
        self.restore_expanded = false;
        self.collapse_at = None;
        self.shaped_expanded = None;
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
    flyout_hovered: bool,
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

pub(super) fn render(ui: &mut egui::Ui, placement: PopupPlacement, generation: u64) {
    let context = ui.ctx().clone();
    if ui.input(|input| input.viewport().close_requested() || input.key_pressed(egui::Key::Escape))
    {
        super::close_from_viewport(&context, "escape-or-window-close");
        return;
    }

    let snapshot = super::data::snapshot();
    let now = Instant::now();
    let expanded = {
        let mut runtime = RUNTIME.lock();
        if runtime.generation != generation {
            runtime.reset(generation);
        }
        runtime.restore_expanded && !snapshot.restore_options.is_empty()
    };

    let painted = paint_popup(ui, placement, &snapshot, expanded);
    if let Some(window) = super::win32::popup_window() {
        let mut runtime = RUNTIME.lock();
        update_flyout_state(
            &mut runtime,
            &painted,
            snapshot.restore_options.is_empty(),
            now,
        );

        if runtime.prepared_generation != generation {
            runtime.prepared_generation = generation;
            runtime.shaped_expanded = Some(runtime.restore_expanded);
        } else if runtime.shaped_expanded != Some(runtime.restore_expanded) {
            super::win32::apply_bounds_and_region(window, placement, runtime.restore_expanded);
            runtime.shaped_expanded = Some(runtime.restore_expanded);
        }
    } else {
        context.request_repaint_after(Duration::from_millis(10));
    }
    if let Some(action) = painted.action {
        perform_action(action, &context);
        return;
    }
    context.request_repaint_after(Duration::from_millis(50));
}

pub(super) fn prepaint(ui: &mut egui::Ui, placement: PopupPlacement) {
    let snapshot = super::data::snapshot();
    let _ = paint_popup(ui, placement, &snapshot, false);
}

fn update_flyout_state(
    runtime: &mut PopupRuntime,
    painted: &PaintResult,
    restore_empty: bool,
    now: Instant,
) {
    if restore_empty {
        runtime.restore_expanded = false;
        runtime.collapse_at = None;
    } else if painted.restore_hovered || painted.flyout_hovered {
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

fn paint_popup(
    ui: &mut egui::Ui,
    placement: PopupPlacement,
    snapshot: &PopupSnapshot,
    expanded: bool,
) -> PaintResult {
    ui.set_min_size(placement.size_points);
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

    let flyout_hovered = if expanded && !restore_disabled {
        let (flyout_action, hovered) = paint_flyout(ui, origin, placement, snapshot, theme);
        action = flyout_action.or(action);
        hovered
    } else {
        false
    };

    PaintResult {
        action,
        restore_hovered: !restore_disabled && restore.hovered(),
        flyout_hovered,
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

fn paint_flyout(
    ui: &mut egui::Ui,
    origin: egui::Pos2,
    placement: PopupPlacement,
    snapshot: &PopupSnapshot,
    theme: AppTheme,
) -> (Option<Action>, bool) {
    let flyout_min = origin + egui::vec2(MAIN_WIDTH + FLYOUT_GAP, placement.flyout_top);
    let flyout = egui::Rect::from_min_size(
        flyout_min,
        egui::vec2(FLYOUT_WIDTH, placement.flyout_height),
    );
    paint_card(ui.painter(), flyout, theme, ui.visuals().dark_mode);
    let mut action = None;
    let mut any_hovered = false;

    for (index, option) in snapshot.restore_options.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            flyout_min + egui::vec2(4.0, 4.0 + index as f32 * OPTION_HEIGHT),
            egui::vec2(FLYOUT_WIDTH - 8.0, OPTION_HEIGHT),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("restore-option", option.batch_count)),
            egui::Sense::click(),
        );
        any_hovered |= response.hovered();
        if response.hovered() || response.has_focus() {
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
        paint_elided_text(
            ui.painter(),
            &option.label,
            egui::pos2(rect.left() + 10.0, rect.center().y),
            rect.width() - 20.0,
            12.0,
            theme.on_surface(),
        );
        if activated(ui, &response) {
            action = Some(Action::Restore(option.batch_count));
        }
    }
    (
        action,
        any_hovered
            || flyout.contains(
                ui.input(|input| input.pointer.hover_pos())
                    .unwrap_or_default(),
            ),
    )
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

    fn hover(restore_hovered: bool, flyout_hovered: bool) -> PaintResult {
        PaintResult {
            action: None,
            restore_hovered,
            flyout_hovered,
        }
    }

    #[test]
    fn restore_flyout_bridges_the_pointer_gap_before_collapsing() {
        let start = Instant::now();
        let mut runtime = PopupRuntime::default();
        update_flyout_state(&mut runtime, &hover(true, false), false, start);
        assert!(runtime.restore_expanded);

        update_flyout_state(
            &mut runtime,
            &hover(false, false),
            false,
            start + Duration::from_millis(10),
        );
        update_flyout_state(
            &mut runtime,
            &hover(false, true),
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
        update_flyout_state(&mut runtime, &hover(true, false), false, start);
        update_flyout_state(
            &mut runtime,
            &hover(false, false),
            false,
            start + Duration::from_millis(10),
        );
        update_flyout_state(
            &mut runtime,
            &hover(false, false),
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
        update_flyout_state(&mut runtime, &hover(true, true), true, Instant::now());
        assert!(!runtime.restore_expanded);
        assert!(runtime.collapse_at.is_none());
    }
}
