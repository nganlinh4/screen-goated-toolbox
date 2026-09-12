//! Hotkey configuration type.

use serde::{Deserialize, Serialize, Serializer, ser::SerializeStruct};

/// Represents a keyboard hotkey binding
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Hotkey {
    /// Virtual key code
    pub code: u32,
    /// Modifier flags (Ctrl, Alt, Shift, Win)
    pub modifiers: u32,
}

impl Hotkey {
    pub fn new(code: u32, modifiers: u32) -> Self {
        Self { code, modifiers }
    }

    pub fn display_name(&self) -> String {
        crate::hotkey::names::format(self.code, self.modifiers)
    }
}

// Keep the public JSON shape while deriving labels from binding identity.
// Legacy saved `name` values are ignored on deserialize.
impl Serialize for Hotkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_struct("Hotkey", 3)?;
        value.serialize_field("code", &self.code)?;
        value.serialize_field("name", &self.display_name())?;
        value.serialize_field("modifiers", &self.modifiers)?;
        value.end()
    }
}
