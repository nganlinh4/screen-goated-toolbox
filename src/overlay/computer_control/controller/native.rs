//! The native (desktop UIA) `Surface`: perception, execution, and read-back over
//! the foreground window, through `uia::enumerate` and the humanized executor.
//!
//! Perception pairs each unlabeled field with its nearest visible Text label and
//! lists ONLY interactive elements — so the model targets a field by @id and can
//! never click the stray Text label (the native "Name" bug). Fills are read back
//! through UIA ValuePattern, so a fill that didn't land is caught in code.
//!
//! Native exposes no language-neutral form/submit/destructive structure the way
//! HTML does, so those gates stay browser-strong; here the wins are paired-label
//! @id targeting + fill verification. Destructive caution on the desktop stays in
//! the prompt JUDGMENT/SAFETY block (it is inherently language-bound).

use anyhow::Result;
use serde_json::json;

use super::super::executor;
use super::super::uia::{self, UiElement};
use super::verify::ReadBack;
use super::world::{ElHandle, IndexedElement, WorldState};
use super::{ActCtx, Surface, Verb};

pub struct NativeSurface;

impl NativeSurface {
    pub fn new() -> Self {
        NativeSurface
    }
}

impl Surface for NativeSurface {
    fn observe(&mut self) -> Result<WorldState> {
        Ok(build_world(&uia::enumerate(None)?))
    }

    fn execute(&mut self, el: &IndexedElement, verb: Verb, value: Option<&str>, act: &ActCtx) -> Result<()> {
        let ElHandle::Native { cx, cy } = el.handle else {
            anyhow::bail!("not a native element");
        };
        match verb {
            Verb::Click | Verb::Submit | Verb::Toggle => {
                click_native(cx, cy, act);
                Ok(())
            }
            Verb::Fill => {
                click_native(cx, cy, act); // focus the field
                if !act.dry {
                    // Select existing content so the paste replaces it, then type.
                    let _ = executor::execute_ex(
                        "key_combination",
                        &json!({"keys": "Control+A"}),
                        act.profile,
                        act.cancel,
                    );
                    let _ = executor::execute_ex(
                        "type_text",
                        &json!({"text": value.unwrap_or("")}),
                        act.profile,
                        act.cancel,
                    );
                }
                Ok(())
            }
            Verb::Select => {
                click_native(cx, cy, act); // open the dropdown
                anyhow::bail!(
                    "on the desktop 'select' only opens the dropdown — observe() the now-open list and click the option you want"
                )
            }
        }
    }

    fn read_back(&mut self, el: &IndexedElement) -> ReadBack {
        let ElHandle::Native { cx, cy } = el.handle else {
            return ReadBack::default();
        };
        ReadBack { value: uia::read_value_at(cx, cy), validity: None }
    }
}

/// Humanized left-click at a screen pixel (the executor maps screen→0-1000 and
/// drives `SendInput`). `uncertain:false` — UIA gives the exact center, no hesitation.
fn click_native(cx: i32, cy: i32, act: &ActCtx) {
    if act.dry {
        return;
    }
    let (vx, vy, vw, vh) = uia::virtual_desktop();
    let nx = (cx - vx) as f64 / vw.max(1) as f64 * 1000.0;
    let ny = (cy - vy) as f64 / vh.max(1) as f64 * 1000.0;
    let _ = executor::execute_ex(
        "click",
        &json!({"x": nx, "y": ny, "button": "left", "uncertain": false}),
        act.profile,
        act.cancel,
    );
}

/// Map a UIA control type to the model-facing role, or `None` for non-actionable
/// containers (Text/Image/Pane/Group/Window — Text is used only for label pairing).
fn interactive_role(ct: &str) -> Option<&'static str> {
    Some(match ct {
        "Edit" | "Document" => "textbox",
        "Button" | "SplitButton" => "button",
        "Hyperlink" => "link",
        "CheckBox" => "checkbox",
        "RadioButton" => "radio",
        "ComboBox" => "combobox",
        "Slider" => "slider",
        "TabItem" => "tab",
        "MenuItem" => "menuitem",
        "ListItem" => "listitem",
        "TreeItem" => "treeitem",
        _ => return None,
    })
}

/// The label text nearest to (and outside) `e` — preferring a Text just to its
/// LEFT on the same row, else one just ABOVE in roughly the same column. This is
/// the native label-pairing that lets the model target the FIELD, not the label.
fn nearest_label(e: &UiElement, labels: &[&UiElement]) -> Option<String> {
    let ey = (e.top + e.bottom) / 2;
    let row_tol = (e.bottom - e.top).max(10);
    let mut best: Option<(i64, String)> = None;
    for l in labels {
        let ly = (l.top + l.bottom) / 2;
        let left_of = l.right <= e.left + 5 && (e.left - l.right) < 400 && (ly - ey).abs() <= row_tol;
        let above = l.bottom <= e.top + 5 && (e.top - l.bottom) < 80 && (l.left - e.left).abs() < 200;
        let d = if left_of {
            (e.left - l.right) as i64 // closest-left wins
        } else if above {
            1000 + (e.top - l.bottom) as i64 // left-of preferred over above
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, l.name.trim().to_string()));
        }
    }
    best.map(|(_, n)| n)
}

/// Build the indexed world from a UIA enumeration: keep interactive elements,
/// pair unlabeled ones with a nearby Text label, drop anonymous noise.
fn build_world(els: &[UiElement]) -> WorldState {
    let labels: Vec<&UiElement> =
        els.iter().filter(|e| e.control_type == "Text" && !e.name.trim().is_empty()).collect();
    let mut out = Vec::new();
    let mut id = 0u32;
    for e in els {
        let Some(role) = interactive_role(e.control_type) else {
            continue;
        };
        let mut name = e.name.trim().to_string();
        if name.is_empty() {
            name = nearest_label(e, &labels).unwrap_or_default();
        }
        // Drop anonymous noise: no name, no value, no state, and not a text field.
        if name.is_empty() && e.value.is_none() && e.state.is_none() && role != "textbox" {
            continue;
        }
        id += 1;
        let (cx, cy) = e.center();
        out.push(IndexedElement {
            id,
            role: role.to_string(),
            name,
            value: e.value.clone(),
            state: e.state.clone(),
            enabled: e.enabled,
            required: e.required,
            submit: false, // no language-neutral submit signal on the desktop
            form: None,    // native has no form grouping
            risk: None,    // no clean structural signal on the desktop — prompt handles it
            handle: ElHandle::Native { cx, cy },
        });
        if out.len() >= 200 {
            break;
        }
    }
    WorldState { elements: out, url: None, title: None }
}
