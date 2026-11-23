//! Which keys are held, tracked lock-free by the keyboard hook so modifiers can
//! be attached to every captured event and auto-repeat presses are ignored.

use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const MOD_CTRL_BIT: u8 = 1 << 0;
pub(super) const MOD_ALT_BIT: u8 = 1 << 1;
pub(super) const MOD_SHIFT_BIT: u8 = 1 << 2;
pub(super) const MOD_WIN_BIT: u8 = 1 << 3;
const TRACKED_VK_WORDS: usize = 8; // 512 virtual keys

static KEY_DOWN_BITS: [AtomicU64; TRACKED_VK_WORDS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

pub(super) fn snapshot_modifiers_bits() -> u8 {
    let mut bits = 0u8;
    // VK_CONTROL (17), VK_LCONTROL (162), VK_RCONTROL (163)
    if key_is_down(17) || key_is_down(162) || key_is_down(163) {
        bits |= MOD_CTRL_BIT;
    }
    // VK_MENU (18), VK_LMENU (164), VK_RMENU (165)
    if key_is_down(18) || key_is_down(164) || key_is_down(165) {
        bits |= MOD_ALT_BIT;
    }
    // VK_SHIFT (16), VK_LSHIFT (160), VK_RSHIFT (161)
    if key_is_down(16) || key_is_down(160) || key_is_down(161) {
        bits |= MOD_SHIFT_BIT;
    }
    // VK_LWIN (91), VK_RWIN (92)
    if key_is_down(91) || key_is_down(92) {
        bits |= MOD_WIN_BIT;
    }
    bits
}

fn key_is_down(vk: u32) -> bool {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return false;
    }
    let mask = 1u64 << (vk % 64);
    (KEY_DOWN_BITS[idx].load(Ordering::Relaxed) & mask) != 0
}

/// Marks `vk` held; true when it already was (an auto-repeat press).
pub(super) fn mark_key_down(vk: u32) -> bool {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return false;
    }
    let mask = 1u64 << (vk % 64);
    (KEY_DOWN_BITS[idx].fetch_or(mask, Ordering::Relaxed) & mask) != 0
}

pub(super) fn clear_key_down(vk: u32) {
    let idx = (vk / 64) as usize;
    if idx >= TRACKED_VK_WORDS {
        return;
    }
    let mask = !(1u64 << (vk % 64));
    KEY_DOWN_BITS[idx].fetch_and(mask, Ordering::Relaxed);
}

pub(super) fn clear_key_state_bits() {
    for word in &KEY_DOWN_BITS {
        word.store(0, Ordering::Relaxed);
    }
}
