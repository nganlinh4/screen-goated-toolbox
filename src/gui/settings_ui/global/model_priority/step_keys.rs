const STEP_SETTLE_SECONDS: f32 = 0.14;

/// Position-independent identity that travels with a row through reorders.
#[derive(Clone, Default)]
pub(super) struct StepKeys {
    next: u64,
    pub(super) keys: Vec<u64>,
}

impl StepKeys {
    pub(super) fn sync(&mut self, len: usize) {
        while self.keys.len() < len {
            self.keys.push(self.next);
            self.next += 1;
        }
        self.keys.truncate(len);
    }

    pub(super) fn at(&self, idx: usize) -> u64 {
        self.keys.get(idx).copied().unwrap_or_default()
    }

    pub(super) fn move_step(&mut self, from: usize, to: usize) {
        let key = self.keys.remove(from);
        self.keys.insert(to, key);
    }

    pub(super) fn forget(&mut self, idx: usize) {
        self.keys.remove(idx);
    }
}

/// Returns the remaining glide from the row's old slot to its current one.
pub(super) fn animated_step_offset(
    ui: &eframe::egui::Ui,
    section: &'static str,
    key: u64,
    dragging: bool,
) -> eframe::egui::Vec2 {
    let target_y = ui.next_widget_position().y;
    let animation_id = eframe::egui::Id::new((section, "step-pos", key));
    let seconds = if dragging { STEP_SETTLE_SECONDS } else { 0.0 };
    let animated_y = ui
        .ctx()
        .animate_value_with_time(animation_id, target_y, seconds);
    eframe::egui::vec2(0.0, animated_y - target_y)
}
