#[path = "../../../src/hotkey/modifiers.rs"]
mod modifiers;
#[path = "../../../src/hotkey/names.rs"]
pub(crate) mod names;
#[path = "../../../src/hotkey/web_binding.rs"]
pub(crate) mod web_binding;

pub use modifiers::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
