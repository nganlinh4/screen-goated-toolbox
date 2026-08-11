//! Physical-pixel geometry for cards inside the virtual-desktop compositor.

use serde::Serialize;
use std::cell::{Cell, RefCell};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardRole {
    Transcription,
    Translation,
}

impl CardRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "transcription" => Some(Self::Transcription),
            "translation" => Some(Self::Translation),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transcription => "transcription",
            Self::Translation => "translation",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CardRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CompositorLayout {
    pub transcription: CardRect,
    pub translation: CardRect,
}

thread_local! {
    static LAYOUT: RefCell<CompositorLayout> = RefCell::new(CompositorLayout::default());
    static INTERACTION_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub fn configure(
    main_position: (i32, i32),
    main_size: (i32, i32),
    translation_size: (i32, i32),
    has_translation: bool,
) {
    let trans_x = main_position.0 + main_size.0 + super::state::GAP;
    LAYOUT.with(|slot| {
        *slot.borrow_mut() = CompositorLayout {
            transcription: CardRect {
                x: main_position.0,
                y: main_position.1,
                width: main_size.0,
                height: main_size.1,
                visible: true,
            },
            translation: CardRect {
                x: trans_x,
                y: main_position.1,
                width: translation_size.0,
                height: translation_size.1,
                visible: has_translation,
            },
        };
    });
}

pub fn snapshot_for_renderer() -> CompositorLayout {
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    LAYOUT.with(|slot| {
        let mut layout = *slot.borrow();
        layout.transcription.x -= virtual_x;
        layout.transcription.y -= virtual_y;
        layout.translation.x -= virtual_x;
        layout.translation.y -= virtual_y;
        layout
    })
}

pub fn move_card(role: CardRole, dx: i32, dy: i32) {
    LAYOUT.with(|slot| {
        let mut layout = slot.borrow_mut();
        let card = card_mut(&mut layout, role);
        card.x += dx;
        card.y += dy;
    });
}

pub fn move_group(dx: i32, dy: i32) {
    LAYOUT.with(|slot| {
        let mut layout = slot.borrow_mut();
        layout.transcription.x += dx;
        layout.transcription.y += dy;
        layout.translation.x += dx;
        layout.translation.y += dy;
    });
}

pub fn resize_card(role: CardRole, dx: i32, dy: i32) {
    LAYOUT.with(|slot| {
        let mut layout = slot.borrow_mut();
        let card = card_mut(&mut layout, role);
        card.width = (card.width + dx).max(200);
        card.height = (card.height + dy).max(100);
    });
}

pub fn set_visible(role: CardRole, visible: bool) {
    LAYOUT.with(|slot| card_mut(&mut slot.borrow_mut(), role).visible = visible);
}

pub fn card_size(role: CardRole) -> (i32, i32) {
    LAYOUT.with(|slot| {
        let layout = slot.borrow();
        let card = card(&layout, role);
        (card.width, card.height)
    })
}

pub fn set_interaction_active(hwnd: HWND, active: bool) {
    INTERACTION_ACTIVE.with(|state| state.set(active));
    rebuild_native_region(hwnd);
}

fn interaction_active() -> bool {
    INTERACTION_ACTIVE.with(Cell::get)
}

pub fn apply_native_region(hwnd: HWND) {
    if !should_refresh_sparse_region(interaction_active()) {
        return;
    }
    rebuild_native_region(hwnd);
}

fn should_refresh_sparse_region(interacting: bool) -> bool {
    !interacting
}

fn rebuild_native_region(hwnd: HWND) {
    let layout = snapshot_for_renderer();
    unsafe {
        let interacting = interaction_active();
        let combined = if interacting {
            CreateRectRgn(
                0,
                0,
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            )
        } else {
            CreateRectRgn(0, 0, 0, 0)
        };
        if !interacting {
            for card in [layout.transcription, layout.translation]
                .into_iter()
                .filter(|card| card.visible)
            {
                // WebView2 anti-aliases the visible 12 CSS-pixel corner. A GDI
                // round region is integer-aliased and cannot track CSS DPI, so
                // only constrain hit testing to the card's rectangular bounds.
                let region =
                    CreateRectRgn(card.x, card.y, card.x + card.width, card.y + card.height);
                let _ = CombineRgn(Some(combined), Some(combined), Some(region), RGN_OR);
                let _ = DeleteObject(region.into());
            }
        }
        let _ = SetWindowRgn(hwnd, Some(combined), true);
    }
}

fn card(layout: &CompositorLayout, role: CardRole) -> &CardRect {
    match role {
        CardRole::Transcription => &layout.transcription,
        CardRole::Translation => &layout.translation,
    }
}

fn card_mut(layout: &mut CompositorLayout, role: CardRole) -> &mut CardRect {
    match role {
        CardRole::Transcription => &mut layout.transcription,
        CardRole::Translation => &mut layout.translation,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CardRole, card_size, configure, interaction_active, move_card, resize_card,
        should_refresh_sparse_region,
    };

    #[test]
    fn cards_move_and_resize_independently() {
        configure((100, 200), (400, 300), (500, 250), true);
        move_card(CardRole::Translation, 10, 20);
        resize_card(CardRole::Transcription, -1000, -1000);
        assert_eq!(card_size(CardRole::Transcription), (200, 100));
        assert_eq!(card_size(CardRole::Translation), (500, 250));
    }

    #[test]
    fn interaction_state_is_explicit_and_starts_inactive() {
        assert!(!interaction_active());
        assert!(!should_refresh_sparse_region(true));
        assert!(should_refresh_sparse_region(false));
    }
}
