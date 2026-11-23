//! The indexed world-state the model reasons over — surface-agnostic.
//!
//! Perception (a `Surface`) produces a `WorldState`: a flat, numbered list of the
//! interactive elements actually present, each carrying its PAIRED accessible
//! label (computed onto the element itself, so the model can never target a stray
//! label — the "Name" bug becomes structurally impossible). The model picks an
//! element by its `id`; the controller resolves that id back to the surface's
//! opaque `handle` and acts on it. Same shape for every surface (browser, native).

/// One interactive element. The model targets it by `id`; we act via `handle`.
#[derive(Debug, Clone)]
pub struct IndexedElement {
    pub id: u32,
    /// Normalized role: "textbox" / "button" / "link" / "checkbox" / "radio" /
    /// "combobox" / "slider" — what the model treats it as.
    pub role: String,
    /// The PAIRED accessible label (aria-label / labelledby / <label> / placeholder
    /// / own text), computed and attached to THIS element during perception.
    pub name: String,
    /// Current text/selection value, when the element holds one (passwords masked).
    pub value: Option<String>,
    /// Bounded text observation, separate from a full value used by form gates.
    pub value_preview: Option<String>,
    /// True when the control structurally accepts free-form text input. Roles such
    /// as `combobox` may be either editable or selection-only, so role alone is
    /// not sufficient to authorize `fill`.
    pub editable: bool,
    /// The observed control was explicitly identified as a protected text field.
    pub protected: bool,
    /// Ground-truth state when exposed: "checked"/"unchecked"/"expanded"/… .
    pub state: Option<String>,
    pub enabled: bool,
    /// The field must be filled before its form is valid (`required` / aria-required).
    pub required: bool,
    /// A submit control (structural: `type=submit`, or a default button in a form).
    pub submit: bool,
    /// Index of the owning form, so the required-field gate scopes to one form.
    pub form: Option<i32>,
    /// Standards-derived high-stakes signal carrying a short reason, currently
    /// form reset or submission of a form with password/payment fields. URL text
    /// and localized labels are never treated as structural semantics.
    /// Acting on a risky element is GATED: it needs the user's explicit ask or a
    /// `confirm` flag.
    pub risk: Option<String>,
    /// How to re-find + act on the element on its surface.
    pub handle: ElHandle,
}

/// A surface-specific handle to re-find and act on an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElHandle {
    /// Browser: the stamped selector `[data-sgt-id="N"]`, plus the CDP session of
    /// the cross-origin frame it lives in (`None` = the top frame).
    Browser {
        selector: String,
        session: Option<String>,
        tab_id: i64,
        document_id: String,
        element_id: String,
    },
    /// Native (desktop UIA): the screen-pixel center to click / read-back at.
    Native {
        cx: i32,
        cy: i32,
        /// Provider-owned identity fields captured from the exact UIA node.
        /// The input edge re-resolves the point and requires these to match, so
        /// an occluded or inactive accessibility subtree cannot receive a click.
        provider_name: String,
        automation_id: String,
        runtime_id: Vec<i32>,
    },
}

/// Stable identity of the surface from which an indexed world was observed.
/// IDs and coordinates are valid only while this identity still matches.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceIdentity {
    Native {
        hwnd: u64,
        pid: u64,
        generation: u64,
    },
    Browser {
        tab_id: i64,
        document_id: String,
        window: BrowserWindowIdentity,
    },
}

/// Exact native window owned by the extension's browser window. The browser
/// window id distinguishes sibling windows in one browser process; HWND/PID and
/// generation bind the model-visible page to the actual foreground OS surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct BrowserWindowIdentity {
    pub browser_window_id: i64,
    pub hwnd: u64,
    pub pid: u64,
    pub generation: u64,
}

/// A snapshot of the interactive world of the active surface.
#[derive(Debug, Clone)]
pub struct WorldState {
    pub elements: Vec<IndexedElement>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub identity: SurfaceIdentity,
}

impl WorldState {
    pub fn get(&self, id: u32) -> Option<&IndexedElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Other required, still-empty fields in the same form as `el` — the
    /// premature-submit signal (fully structural, language-neutral).
    pub fn empty_required_in_form(&self, el: &IndexedElement) -> Vec<&str> {
        self.elements
            .iter()
            .filter(|e| {
                e.required && e.form == el.form && e.value.as_deref().is_some_and(str::is_empty)
            })
            .map(|e| e.name.as_str())
            .collect()
    }

    pub fn unobserved_required_in_form(&self, el: &IndexedElement) -> Vec<&str> {
        self.elements
            .iter()
            .filter(|element| {
                element.required && element.form == el.form && element.value.is_none()
            })
            .map(|element| element.name.as_str())
            .collect()
    }

    /// The compact text the model reads: one line per element,
    /// `@id role "name" [= value] [flags]`.
    pub fn to_model_text(&self) -> String {
        use super::super::model_text_budget::{Lines, MAX_FIELD_BYTES, MAX_WORLD_BYTES, field};
        let mut lines = Lines::new(MAX_WORLD_BYTES);
        if let Some(t) = &self.title {
            let mut title = format!("PAGE: {}", field(t, MAX_FIELD_BYTES));
            if let Some(u) = &self.url {
                title.push_str(&format!("  <{}>", field(u, 360)));
            }
            title.push('\n');
            lines.push(&title);
        }
        if self.elements.is_empty() {
            lines.push("(no interactive elements found — the page may still be loading, or its content is in a canvas / cross-origin frame)");
            return lines.finish();
        }
        for e in &self.elements {
            let mut line = format!(
                "@{} {} {:?}",
                e.id,
                field(&e.role, 96),
                field(&e.name, MAX_FIELD_BYTES)
            );
            if let Some(v) = e.value.as_deref().filter(|v| !v.is_empty()) {
                line.push_str(&format!(" = {:?}", field(v, 240)));
            } else if let Some(preview) = e.value_preview.as_deref() {
                line.push_str(&format!(" ~ {:?} [text preview]", field(preview, 240)));
            }
            let mut flags: Vec<String> = Vec::new();
            if e.required {
                flags.push(
                    match e.value.as_deref() {
                        Some("") => "required·empty",
                        Some(_) => "required",
                        None => "required·unobserved",
                    }
                    .to_string(),
                );
            }
            if let Some(st) = &e.state {
                flags.push(field(st, 96));
            }
            if !e.enabled {
                flags.push("disabled".to_string());
            }
            if e.editable && !matches!(e.role.as_str(), "textbox" | "searchbox" | "spinbutton") {
                flags.push("editable".to_string());
            }
            if !e.editable && matches!(e.role.as_str(), "textbox" | "searchbox" | "spinbutton") {
                flags.push("fill·unavailable".to_string());
            }
            if !flags.is_empty() {
                line.push_str(&format!(" [{}]", flags.join(", ")));
            }
            if let Some(r) = &e.risk {
                line.push_str(&format!(" ⚠ {}", field(r, MAX_FIELD_BYTES)));
            }
            line.push('\n');
            if !lines.push(&line) {
                break;
            }
        }
        lines.finish()
    }
}

#[cfg(test)]
#[path = "world_text_tests.rs"]
mod text_tests;
