use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use serde_json::{Value, json};

use super::generation_mode::GenerationMode;

const MAX_INSTRUCTION_CHARACTERS: usize = 1_000;

#[derive(Clone, Copy, Default)]
struct ProductCapabilities {
    fast_instruction: bool,
    quality_instruction: bool,
}

static CAPABILITIES: LazyLock<Mutex<Option<ProductCapabilities>>> =
    LazyLock::new(|| Mutex::new(None));
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn refresh() {
    if REFRESHING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| {
        let next =
            ProductCapabilities {
                fast_instruction:
                    crate::overlay::creation_runtime::supports_optional_3d_instruction("fast"),
                quality_instruction:
                    crate::overlay::creation_runtime::supports_optional_3d_instruction("quality"),
            };
        *CAPABILITIES
            .lock()
            .unwrap_or_else(|value| value.into_inner()) = Some(next);
        REFRESHING.store(false, Ordering::SeqCst);
    });
}

pub(super) fn product_capabilities() -> Value {
    refresh();
    let current = *CAPABILITIES
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let value = current.unwrap_or_default();
    json!({
        "ready": current.is_some(),
        "optionalInstruction": {
            "fast": value.fast_instruction,
            "quality": value.quality_instruction,
        }
    })
}

pub(super) fn normalize_instruction(
    mode: GenerationMode,
    instruction: &mut Option<String>,
) -> Result<(), String> {
    *instruction = instruction
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(value) = instruction.as_ref() else {
        return Ok(());
    };
    if value.chars().count() > MAX_INSTRUCTION_CHARACTERS {
        return Err("The optional model instruction is too long.".to_string());
    }
    if !crate::overlay::creation_runtime::supports_optional_3d_instruction(mode.as_str()) {
        return Err("Optional instructions are unavailable for this mode.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_instruction_is_always_omitted() {
        let mut instruction = Some("   ".to_string());
        normalize_instruction(GenerationMode::Quality, &mut instruction).unwrap();
        assert!(instruction.is_none());
    }
}
