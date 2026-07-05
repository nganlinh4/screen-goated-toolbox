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
    /// Ground-truth state when exposed: "checked"/"unchecked"/"expanded"/… .
    pub state: Option<String>,
    pub enabled: bool,
    /// The field must be filled before its form is valid (`required` / aria-required).
    pub required: bool,
    /// A submit control (structural: `type=submit`, or a default button in a form).
    pub submit: bool,
    /// Index of the owning form, so the required-field gate scopes to one form.
    pub form: Option<i32>,
    /// Best-effort, LANGUAGE-NEUTRAL high-stakes signal carrying a short REASON
    /// ("signs you out", "submits a password or payment details", "starts a
    /// payment") when acting on this element is consequential/irreversible — from
    /// structure only (form-reset, logout/pay/delete href or path, a submit of a
    /// form with password/payment fields). `None` = ordinary. Semantic
    /// send/post/publish detection stays in the prompt (it is language-bound).
    /// Acting on a risky element is GATED: it needs the user's explicit ask or a
    /// `confirm` flag.
    pub risk: Option<String>,
    /// How to re-find + act on the element on its surface.
    pub handle: ElHandle,
}

/// A surface-specific handle to re-find and act on an element.
#[derive(Debug, Clone)]
pub enum ElHandle {
    /// Browser: the stamped selector `[data-sgt-id="N"]`, plus the CDP session of
    /// the cross-origin frame it lives in (`None` = the top frame).
    Browser {
        selector: String,
        session: Option<String>,
    },
    /// Native (desktop UIA): the screen-pixel center to click / read-back at.
    Native { cx: i32, cy: i32 },
}

/// A snapshot of the interactive world of the active surface.
pub struct WorldState {
    pub elements: Vec<IndexedElement>,
    pub url: Option<String>,
    pub title: Option<String>,
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
                e.required && e.form == el.form && e.value.as_deref().unwrap_or("").is_empty()
            })
            .map(|e| e.name.as_str())
            .collect()
    }

    /// The compact text the model reads: one line per element,
    /// `@id role "name" [= value] [flags]`.
    pub fn to_model_text(&self) -> String {
        let mut s = String::new();
        if let Some(t) = &self.title {
            s.push_str(&format!("PAGE: {t}"));
            if let Some(u) = &self.url {
                s.push_str(&format!("  <{}>", u.chars().take(90).collect::<String>()));
            }
            s.push('\n');
        }
        if self.elements.is_empty() {
            s.push_str("(no interactive elements found — the page may still be loading, or its content is in a canvas / cross-origin frame)");
            return s;
        }
        for e in &self.elements {
            s.push_str(&format!("@{} {} {:?}", e.id, e.role, e.name));
            if let Some(v) = e.value.as_deref().filter(|v| !v.is_empty()) {
                s.push_str(&format!(" = {:?}", v.chars().take(60).collect::<String>()));
            }
            let mut flags: Vec<&str> = Vec::new();
            let req = e.value.as_deref().unwrap_or("").is_empty();
            if e.required {
                flags.push(if req { "required·empty" } else { "required" });
            }
            if let Some(st) = &e.state {
                flags.push(st);
            }
            if !e.enabled {
                flags.push("disabled");
            }
            if !flags.is_empty() {
                s.push_str(&format!(" [{}]", flags.join(", ")));
            }
            if let Some(r) = &e.risk {
                s.push_str(&format!(" ⚠ {r}"));
            }
            s.push('\n');
        }
        s
    }
}
