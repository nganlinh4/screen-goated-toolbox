//! Pointer-tracking reorder for the app's sortable lists.
//!
//! The sortable-list pattern the web settled on: the grabbed row is lifted out
//! of the list and follows the cursor, the rows left behind close ranks and open
//! a gap where it would land, and the list itself is only rewritten when the
//! pointer is released. Nothing swaps underneath the cursor mid-drag, so there is
//! no flicker, and where a row lands depends on where it is being held rather
//! than on which row the pointer happens to be over.

use eframe::egui;

/// One entry of the list as drawn this frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Slot {
    /// Draw the list entry at this index.
    Step(usize),
    /// Leave a row-sized hole: where the lifted row would land on release.
    Gap,
}

/// The row riding the cursor.
#[derive(Clone, Copy)]
struct LiftedRow {
    from: usize,
    insert: usize,
    /// Where inside the row the pointer grabbed it, so the row keeps its grip
    /// point instead of snapping its top or centre to the cursor.
    grab_dy: f32,
    /// Footprint of the row as it was picked up. The gap reserves exactly this,
    /// so a list whose width is set by its longest row cannot narrow mid-drag.
    size: egui::Vec2,
}

/// The list's geometry. Removing a row and inserting a same-sized gap leaves the
/// layout unchanged, so these stay valid for the whole drag.
#[derive(Clone, Copy)]
struct ListMetrics {
    first_slot: egui::Rect,
    pitch: f32,
}

impl Default for ListMetrics {
    fn default() -> Self {
        Self {
            first_slot: egui::Rect::NOTHING,
            pitch: 0.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    lifted: Option<LiftedRow>,
    metrics: ListMetrics,
    /// Pass this list was last drawn on, so a lift cannot outlive it.
    last_pass: u64,
}

/// The floating copy to draw for a lifted row.
pub(crate) struct FloatingRow {
    pub origin: egui::Pos2,
    pub size: egui::Vec2,
    /// Index of the row being carried.
    pub from: usize,
    /// Position it currently projects to, for anything the copy numbers.
    pub insert: usize,
}

pub(crate) struct ListReorder {
    id: egui::Id,
    state: State,
    first_slot: Option<egui::Rect>,
    pitch: Option<f32>,
}

impl ListReorder {
    pub(crate) fn load(ui: &egui::Ui, section: &'static str) -> Self {
        let id = egui::Id::new((section, "list-reorder"));
        let pass = ui.ctx().cumulative_pass_nr();
        let mut state = ui
            .data(|data| data.get_temp::<State>(id))
            .unwrap_or_default();
        // A lift only lives as long as the list keeps being drawn. Closing the
        // modal (or switching tabs) mid-drag means the release happened where
        // this code could not see it, so resume nothing: a carried-over lift
        // would come back with a row missing and a gap in its place.
        if state.last_pass + 1 < pass {
            state.lifted = None;
        }
        state.last_pass = pass;
        Self {
            id,
            state,
            first_slot: None,
            pitch: None,
        }
    }

    /// The list's geometry, preferring what this frame measured over what the
    /// last one stored. Inserting a gap where the lifted row was leaves the
    /// layout unchanged, so this stays accurate mid-drag.
    fn metrics(&self) -> ListMetrics {
        ListMetrics {
            first_slot: self.first_slot.unwrap_or(self.state.metrics.first_slot),
            pitch: self.pitch.unwrap_or(self.state.metrics.pitch),
        }
    }

    pub(crate) fn is_lifting(&self) -> bool {
        self.state.lifted.is_some()
    }

    /// The order to draw the list in: untouched, or the remaining rows with a
    /// gap at the projected landing position.
    pub(crate) fn plan(&self, len: usize) -> Vec<Slot> {
        let Some(lifted) = self.state.lifted else {
            return (0..len).map(Slot::Step).collect();
        };
        let mut slots: Vec<Slot> = (0..len)
            .filter(|idx| *idx != lifted.from)
            .map(Slot::Step)
            .collect();
        slots.insert(lifted.insert.min(slots.len()), Slot::Gap);
        slots
    }

    /// Size a gap should occupy: the row that vacated it, so the list keeps the
    /// footprint it had before the lift. Falls back to a measured row, then to
    /// the caller's guess.
    pub(crate) fn slot_size(&self, fallback: egui::Vec2) -> egui::Vec2 {
        [self.state.lifted.map(|lifted| lifted.size)]
            .into_iter()
            .flatten()
            .chain(std::iter::once(self.metrics().first_slot.size()))
            .find(|size| size.is_finite() && size.x > 0.0 && size.y > 0.0)
            .unwrap_or(fallback)
    }

    /// Record where a drawn slot landed. The first two feed the projection: the
    /// list's origin and its row pitch.
    pub(crate) fn note_slot(&mut self, rect: egui::Rect) {
        if self.first_slot.is_none() {
            self.first_slot = Some(rect);
        } else if self.pitch.is_none() {
            self.pitch = Some(rect.top() - self.first_slot.unwrap_or(rect).top());
        }
    }

    /// Take a row out of the list and onto the cursor.
    pub(crate) fn lift(&mut self, ui: &egui::Ui, from: usize, row: egui::Rect) {
        let grab_dy = ui
            .ctx()
            .pointer_interact_pos()
            .map_or(row.height() / 2.0, |pointer| pointer.y - row.top());
        self.state.lifted = Some(LiftedRow {
            from,
            insert: from,
            grab_dy,
            size: row.size(),
        });
    }

    /// Where to draw the lifted row's floating copy. Yields nothing until the
    /// list has been measured: a copy sized from an empty rect would lay out
    /// at infinity and render as a bare frame with no content in it.
    pub(crate) fn floating(&self, ui: &egui::Ui) -> Option<FloatingRow> {
        let lifted = self.state.lifted?;
        let pointer = ui.ctx().pointer_interact_pos()?;
        let slot = self.metrics().first_slot;
        // The copy is the row that was grabbed, so it keeps that row's width
        // rather than borrowing the first row's.
        let size = self.slot_size(egui::Vec2::ZERO);
        if !slot.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        Some(FloatingRow {
            origin: egui::pos2(slot.left(), pointer.y - lifted.grab_dy),
            size,
            from: lifted.from,
            insert: lifted.insert,
        })
    }

    /// Track the cursor, and on release report the move to apply as `(from, to)`.
    /// Escape abandons the lift, leaving the list as it was.
    pub(crate) fn settle(&mut self, ui: &egui::Ui, len: usize) -> Option<(usize, usize)> {
        let metrics = self.metrics();
        self.state.metrics = metrics;
        let pointer = ui.ctx().pointer_interact_pos();
        let released = !ui.input(|input| input.pointer.any_down());
        let cancelled = ui.input(|input| input.key_pressed(egui::Key::Escape));

        let lifted = self.state.lifted.as_mut()?;
        if cancelled {
            self.state.lifted = None;
            return None;
        }
        if let Some(pointer) = pointer
            && metrics.pitch > 0.0
        {
            // Project from where the row is being held, not from the row under
            // the pointer: the carried row decides its own landing slot.
            let travelled = (pointer.y - lifted.grab_dy - metrics.first_slot.top()) / metrics.pitch;
            lifted.insert = travelled.round().clamp(0.0, len.saturating_sub(1) as f32) as usize;
        }
        if released {
            let moved = (lifted.from, lifted.insert);
            self.state.lifted = None;
            return (moved.0 != moved.1).then_some(moved);
        }
        None
    }

    pub(crate) fn store(self, ui: &egui::Ui) {
        ui.data_mut(|data| data.insert_temp(self.id, self.state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifted(from: usize, insert: usize) -> ListReorder {
        ListReorder {
            id: egui::Id::new("test"),
            state: State {
                lifted: Some(LiftedRow {
                    from,
                    insert,
                    grab_dy: 0.0,
                    size: egui::vec2(180.0, 22.0),
                }),
                metrics: ListMetrics::default(),
                last_pass: 0,
            },
            first_slot: None,
            pitch: None,
        }
    }

    #[test]
    fn an_untouched_list_draws_in_chain_order() {
        let idle = ListReorder {
            id: egui::Id::new("test"),
            state: State::default(),
            first_slot: None,
            pitch: None,
        };
        assert_eq!(
            idle.plan(3),
            vec![Slot::Step(0), Slot::Step(1), Slot::Step(2)]
        );
    }

    #[test]
    fn a_lifted_step_leaves_the_list_and_opens_a_gap_where_it_would_land() {
        // Carrying the first step down to the last slot.
        assert_eq!(
            lifted(0, 2).plan(3),
            vec![Slot::Step(1), Slot::Step(2), Slot::Gap]
        );
        // ...and back up to the top.
        assert_eq!(
            lifted(0, 0).plan(3),
            vec![Slot::Gap, Slot::Step(1), Slot::Step(2)]
        );
        // A step from the middle: the gap follows the projection, never the
        // step's original home.
        assert_eq!(
            lifted(1, 0).plan(3),
            vec![Slot::Gap, Slot::Step(0), Slot::Step(2)]
        );
    }

    #[test]
    fn the_gap_stays_inside_the_list_however_far_the_cursor_travels() {
        let plan = lifted(1, 99).plan(3);
        assert_eq!(plan.len(), 3, "one slot per step, gap included");
        assert_eq!(plan.last(), Some(&Slot::Gap));
    }

    /// Runs one egui pass, handing the body a `Ui` to work in.
    fn pass(ctx: &egui::Context, body: impl FnOnce(&mut egui::Ui)) {
        let mut body = Some(body);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(body) = body.take() {
                body(ui);
            }
        });
    }

    #[test]
    fn the_gap_holds_the_footprint_of_the_row_that_left_it() {
        // A column is as wide as its widest row. If the gap that row vacates
        // reserves anything narrower, the column shrinks for the length of the
        // drag and snaps back on release.
        assert_eq!(
            lifted(0, 2).slot_size(egui::vec2(10.0, 10.0)),
            egui::vec2(180.0, 22.0)
        );

        let idle = ListReorder {
            id: egui::Id::new("test"),
            state: State::default(),
            first_slot: None,
            pitch: None,
        };
        assert_eq!(
            idle.slot_size(egui::vec2(10.0, 10.0)),
            egui::vec2(10.0, 10.0),
            "an unmeasured list falls back to the caller's guess"
        );
    }

    #[test]
    fn a_lift_does_not_outlive_the_list_being_hidden() {
        let ctx = egui::Context::default();
        let row = egui::Rect::from_min_size(egui::pos2(0.0, 20.0), egui::vec2(120.0, 24.0));

        pass(&ctx, |ui| {
            let mut reorder = ListReorder::load(ui, "test");
            reorder.lift(ui, 1, row);
            reorder.store(ui);
        });
        // Still on screen the next pass: the lift is mid-gesture and must stand.
        pass(&ctx, |ui| {
            let reorder = ListReorder::load(ui, "test");
            assert!(reorder.is_lifting(), "a live drag was dropped");
            reorder.store(ui);
        });

        // The modal closes mid-drag: the list stops being drawn, so the
        // release lands somewhere this code never sees.
        pass(&ctx, |_| {});
        pass(&ctx, |_| {});

        pass(&ctx, |ui| {
            let reorder = ListReorder::load(ui, "test");
            assert!(
                !reorder.is_lifting(),
                "reopening resumed a phantom drag: the list would show a gap                  and a missing step"
            );
            assert_eq!(
                reorder.plan(3),
                vec![Slot::Step(0), Slot::Step(1), Slot::Step(2)]
            );
            reorder.store(ui);
        });
    }
}
