//! The browser `Surface`: perception, execution, and read-back over the page,
//! all through the existing CDP bridge (`super::super::browser`).
//!
//! Perception runs the JS pass in the TOP frame AND in each cross-origin child
//! frame (via its CDP session), so login / payment / embed widgets the top
//! document can't reach are perceived + acted on too. Per element it computes the
//! accessible label and STAMPS a `data-sgt-id` (ids continue across frames so they
//! stay globally unique); execution re-finds it by `[data-sgt-id="N"]` in its own
//! frame. The label travels ON the input, so clicking a stray <label> (the old
//! "Name" bug) is structurally impossible.

use anyhow::Result;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use super::super::browser as web;
use super::verify::ReadBack;
use super::world::{ElHandle, IndexedElement, SurfaceIdentity, WorldState};
use super::{ActCtx, Surface, Verb};

mod errors;
use errors::stale_action_failure;
pub(super) use errors::{action_failure, step_failure};

pub struct BrowserSurface {
    exact_tab_id: Option<i64>,
}

impl BrowserSurface {
    pub fn new() -> Self {
        Self { exact_tab_id: None }
    }

    pub fn on_tab(tab_id: i64) -> Self {
        Self {
            exact_tab_id: Some(tab_id),
        }
    }

    fn eval(&self, expression: &str, tab_id: i64, session: Option<&str>) -> Result<Value> {
        match self.exact_tab_id {
            Some(_) => web::eval_value_in_exact_tab(expression, tab_id, session),
            None => web::eval_value_in_active_tab(expression, tab_id, session),
        }
    }

    fn child_frames(&self, tab_id: i64) -> Vec<String> {
        if self.exact_tab_id.is_some() {
            web::child_frames_on_tab(tab_id)
        } else {
            web::child_frames()
        }
    }

    fn click(&self, selector: &str, tab_id: i64, document_id: &str, element_id: &str) -> Value {
        if self.exact_tab_id.is_some() {
            web::click_selector_on_tab(selector, tab_id, document_id, element_id)
        } else {
            web::click_selector_on_active_tab(selector, tab_id, document_id, element_id)
        }
    }

    fn fill(
        &self,
        selector: &str,
        text: &str,
        session: Option<&str>,
        tab_id: i64,
        document_id: &str,
        element_id: &str,
    ) -> Value {
        if self.exact_tab_id.is_some() {
            web::fill_in_on_tab(selector, text, session, tab_id, document_id, element_id)
        } else {
            web::fill_in_on_active_tab(selector, text, session, tab_id, document_id, element_id)
        }
    }

    /// Set a <select> (in its frame) to the option matching `value`, firing
    /// input/change so frameworks notice.
    fn select(
        &self,
        selector: &str,
        session: Option<&str>,
        tab_id: i64,
        value: &str,
        document_id: &str,
        element_id: &str,
    ) -> Result<()> {
        let selector_json = serde_json::to_string(selector)?;
        let document_json = serde_json::to_string(document_id)?;
        let element_json = serde_json::to_string(element_id)?;
        let inspect = format!(
            r#"(() => {{ const documentId=({document_identity});
                const e=document.querySelector({selector_json});
                const elementId=e && (e[Symbol.for('sgt.controller.element-id')] || null);
                if(documentId!=={document_json}) return {{status:'stale',reason:'document_changed',documentId,elementId}};
                if(!e) return {{status:'stale',reason:'target_missing',documentId,elementId}};
                if(elementId!=={element_json}) return {{status:'stale',reason:'element_changed',documentId,elementId}};
                if(!(e instanceof HTMLSelectElement)) return {{status:'not a select'}};
                return {{status:'ok', options:[...e.options].map((o,index)=>({{
                    index, text:o.text, value:o.value
                }}))}}; }})()"#,
            document_identity = web::DOCUMENT_ID_JS,
        );
        let inspected = self.eval(&inspect, tab_id, session)?;
        if inspected.get("status").and_then(Value::as_str) == Some("stale") {
            return Err(stale_action_failure(
                tab_id,
                selector,
                "select_inspection",
                inspected
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("identity_changed"),
                document_id,
                element_id,
                inspected.clone(),
            ));
        }
        if inspected.get("status").and_then(Value::as_str) != Some("ok") {
            anyhow::bail!(
                "{}",
                inspected
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("select inspection failed")
            );
        }
        let options = inspected
            .get("options")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("select options unavailable"))?;
        let index = unique_option_index(options, value)?;
        let option = &options[index];
        let original_index = option
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("select option index unavailable"))?;
        let expected_text =
            serde_json::to_string(option.get("text").and_then(Value::as_str).unwrap_or(""))?;
        let expected_value =
            serde_json::to_string(option.get("value").and_then(Value::as_str).unwrap_or(""))?;
        let apply = format!(
            r#"(() => {{ const documentId=({document_identity});
                const e=document.querySelector({selector_json});
                const elementId=e && (e[Symbol.for('sgt.controller.element-id')] || null);
                if(documentId!=={document_json}) return {{status:'stale',reason:'document_changed',documentId,elementId}};
                if(!e) return {{status:'stale',reason:'target_missing',documentId,elementId}};
                if(elementId!=={element_json}) return {{status:'stale',reason:'element_changed',documentId,elementId}};
                if(!(e instanceof HTMLSelectElement)) return {{status:'changed',reason:'not_a_select'}};
                const o=e.options[{original_index}];
                if(!o || o.text!=={expected_text} || o.value!=={expected_value}) return {{status:'changed',reason:'options_changed'}};
                e.selectedIndex={original_index};
                e.dispatchEvent(new Event('input',{{bubbles:true}}));
                e.dispatchEvent(new Event('change',{{bubbles:true}})); return {{status:'ok'}}; }})()"#,
            document_identity = web::DOCUMENT_ID_JS,
        );
        let applied = self.eval(&apply, tab_id, session)?;
        match applied.get("status").and_then(Value::as_str) {
            Some("ok") => Ok(()),
            Some("stale") => Err(stale_action_failure(
                tab_id,
                selector,
                "before_select_mutation",
                applied
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("identity_changed"),
                document_id,
                element_id,
                applied.clone(),
            )),
            Some(_) => anyhow::bail!(
                "{}",
                applied
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("select changed")
            ),
            None => anyhow::bail!("select failed"),
        }
    }

    /// Click an element inside a cross-origin iframe via a synthetic `el.click()`.
    /// (A trusted coordinate click would need the OOPIF's screen offset, which it
    /// doesn't expose; a JS click drives the inputs/buttons cross-origin login /
    /// payment widgets are built from.)
    fn click_in_frame(
        &self,
        selector: &str,
        session: &str,
        tab_id: i64,
        document_id: &str,
        element_id: &str,
    ) -> Result<()> {
        let selector_json = serde_json::to_string(selector)?;
        let document_json = serde_json::to_string(document_id)?;
        let element_json = serde_json::to_string(element_id)?;
        let expr = format!(
            r#"(() => {{ const documentId=({document_identity});
                const e=document.querySelector({selector_json});
                const elementId=e && (e[Symbol.for('sgt.controller.element-id')] || null);
                if(documentId!=={document_json}) return {{status:'stale',reason:'document_changed',documentId,elementId}};
                if(!e) return {{status:'stale',reason:'target_missing',documentId,elementId}};
                if(elementId!=={element_json}) return {{status:'stale',reason:'element_changed',documentId,elementId}};
                e.scrollIntoView({{block:'center'}}); e.click(); return {{status:'ok'}}; }})()"#,
            document_identity = web::DOCUMENT_ID_JS,
        );
        let clicked = self.eval(&expr, tab_id, Some(session))?;
        match clicked.get("status").and_then(Value::as_str) {
            Some("ok") => Ok(()),
            Some("stale") => Err(stale_action_failure(
                tab_id,
                selector,
                "before_frame_click",
                clicked
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("identity_changed"),
                document_id,
                element_id,
                clicked.clone(),
            )),
            _ => anyhow::bail!("element click failed in frame"),
        }
    }
}

fn normalize_option(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn unique_option_index(options: &[Value], requested: &str) -> Result<usize> {
    let requested = normalize_option(requested);
    let matches = options
        .iter()
        .enumerate()
        .filter(|(_, option)| {
            ["text", "value"].into_iter().any(|field| {
                option
                    .get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| normalize_option(candidate) == requested)
            })
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => anyhow::bail!("no option matching exactly"),
        _ => anyhow::bail!("ambiguous option: {} exact matches", matches.len()),
    }
}

impl Surface for BrowserSurface {
    fn identity(&mut self) -> Result<SurfaceIdentity> {
        let (tab_id, document_id, window) = web::active_document_identity()?;
        if self.exact_tab_id.is_some_and(|expected| expected != tab_id) {
            anyhow::bail!("controlled browser tab is not the foreground browser surface");
        }
        Ok(SurfaceIdentity::Browser {
            tab_id,
            document_id,
            window,
        })
    }

    fn observe(&mut self) -> Result<WorldState> {
        let identity = self.identity()?;
        let SurfaceIdentity::Browser { tab_id, .. } = &identity else {
            unreachable!();
        };
        // Top frame first; then each cross-origin child frame, its @ids continuing
        // from the running total so every id is globally unique + resolvable.
        let mut world = parse_world(
            &self.eval(&perception_js(0), *tab_id, None)?,
            None,
            *tab_id,
            identity.clone(),
        );
        for sid in self.child_frames(*tab_id) {
            let base = world.elements.len() as u32;
            if let Ok(v) = self.eval(&perception_js(base), *tab_id, Some(&sid)) {
                world
                    .elements
                    .extend(parse_world(&v, Some(sid.clone()), *tab_id, identity.clone()).elements);
            }
        }
        if self.identity()? != identity {
            anyhow::bail!("browser target changed while observing controls");
        }
        Ok(world)
    }

    fn execute(
        &mut self,
        el: &IndexedElement,
        verb: Verb,
        value: Option<&str>,
        _act: &ActCtx,
        expected: &SurfaceIdentity,
    ) -> Result<Value> {
        let ElHandle::Browser {
            selector,
            session,
            tab_id,
            document_id,
            element_id,
        } = &el.handle
        else {
            anyhow::bail!("not a browser element");
        };
        let current_identity = self.identity()?;
        if current_identity != *expected {
            return Err(stale_action_failure(
                *tab_id,
                selector,
                "before_action_resolution",
                "surface_changed",
                document_id,
                element_id,
                serde_json::json!({"surface_identity": format!("{current_identity:?}")}),
            ));
        }
        let result = match (verb, session.as_deref()) {
            (Verb::Fill, sess) => errors::result(self.fill(
                selector,
                value.unwrap_or(""),
                sess,
                *tab_id,
                document_id,
                element_id,
            )),
            (Verb::Select, sess) => {
                self.select(
                    selector,
                    sess,
                    *tab_id,
                    value.unwrap_or(""),
                    document_id,
                    element_id,
                )?;
                Ok(serde_json::json!({"ok": true, "selected": value.unwrap_or("")}))
            }
            // Top frame: trusted coordinate click. Cross-origin frame: JS click.
            (Verb::Click | Verb::Activate | Verb::Submit | Verb::Toggle, None) => {
                errors::result(self.click(selector, *tab_id, document_id, element_id))
            }
            (Verb::Click | Verb::Activate | Verb::Submit | Verb::Toggle, Some(sess)) => {
                self.click_in_frame(selector, sess, *tab_id, document_id, element_id)?;
                Ok(serde_json::json!({"ok": true, "frame_session": sess}))
            }
        }?;
        if self.exact_tab_id.is_none() && web::active_tab_id()? != *tab_id {
            anyhow::bail!("active browser tab changed during action dispatch");
        }
        Ok(result)
    }

    fn read_back(&mut self, el: &IndexedElement) -> ReadBack {
        let ElHandle::Browser {
            selector,
            session,
            tab_id,
            document_id,
            element_id,
        } = &el.handle
        else {
            return ReadBack::default();
        };
        // Passwords read back as null (masked — can't compare) → an honest "unknown".
        let selector_json = serde_json::to_string(selector).unwrap_or_else(|_| "null".to_string());
        let document_json =
            serde_json::to_string(document_id).unwrap_or_else(|_| "null".to_string());
        let element_json = serde_json::to_string(element_id).unwrap_or_else(|_| "null".to_string());
        let expr = format!(
            r#"(() => {{ const documentId=({document_identity});
                const e=document.querySelector({selector_json});
                const elementId=e && (e[Symbol.for('sgt.controller.element-id')] || null);
                if(documentId!=={document_json} || elementId!=={element_json}) return null;
                const t=(e.type||'').toLowerCase();
                return {{ value: (t==='password' ? null
                        : ('value' in e ? e.value : (e.isContentEditable ? e.innerText : null))),
                    validity: e.validationMessage || null }}; }})()"#,
            document_identity = web::DOCUMENT_ID_JS,
        );
        match self.eval(&expr, *tab_id, session.as_deref()) {
            Ok(Value::Null) | Err(_) => ReadBack::default(),
            Ok(v) => ReadBack {
                value: v.get("value").and_then(Value::as_str).map(str::to_string),
                validity: v
                    .get("validity")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            },
        }
    }
}

fn parse_world(
    v: &Value,
    session: Option<String>,
    tab_id: i64,
    identity: SurfaceIdentity,
) -> WorldState {
    let document_id = v.get("documentId").and_then(Value::as_str).unwrap_or("");
    let elements = v
        .get("elements")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| parse_el(e, &session, tab_id, document_id))
                .collect()
        })
        .unwrap_or_default();
    WorldState {
        elements,
        url: v.get("url").and_then(Value::as_str).map(str::to_string),
        title: v.get("title").and_then(Value::as_str).map(str::to_string),
        identity,
    }
}

fn parse_el(
    e: &Value,
    session: &Option<String>,
    tab_id: i64,
    document_id: &str,
) -> Option<IndexedElement> {
    let id = e.get("id").and_then(Value::as_u64)? as u32;
    let element_id = e.get("elementId").and_then(Value::as_str)?;
    if document_id.is_empty() || element_id.is_empty() {
        return None;
    }
    Some(IndexedElement {
        id,
        role: e
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("button")
            .to_string(),
        name: e
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value: e.get("value").and_then(Value::as_str).map(str::to_string),
        state: e.get("state").and_then(Value::as_str).map(str::to_string),
        enabled: !e.get("disabled").and_then(Value::as_bool).unwrap_or(false),
        required: e.get("required").and_then(Value::as_bool).unwrap_or(false),
        submit: e.get("submit").and_then(Value::as_bool).unwrap_or(false),
        form: e
            .get("form")
            .and_then(Value::as_i64)
            .map(|f| f as i32)
            .filter(|f| *f >= 0),
        risk: e.get("risk").and_then(Value::as_str).map(str::to_string),
        handle: ElHandle::Browser {
            selector: format!("[data-sgt-id=\"{id}\"]"),
            session: session.clone(),
            tab_id,
            document_id: document_id.to_string(),
            element_id: element_id.to_string(),
        },
    })
}

/// The perception JS with its `data-sgt-id` counter started at `base`, so elements
/// from different (cross-origin) frames get globally-unique, resolvable @ids.
fn perception_js(base: u32) -> String {
    PERCEPTION_JS.replace("let id = 0;", &format!("let id = {base};"))
}

/// The single perception pass. Walks interactive elements, computes each one's
/// accessible label, stamps `data-sgt-id`, and returns the indexed list + page
/// title/url. Skips hidden / zero-area / `type=hidden` nodes. `risk` uses only
/// standardized element/form metadata. Capped at 200 elements. Run it via
/// `perception_js(base)`, not raw.
const PERCEPTION_JS: &str = r#"(() => {
  const out = []; let id = 0;
  const nonce = () => (globalThis.crypto && crypto.randomUUID)
      ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`;
  const documentKey = Symbol.for('sgt.controller.document-id');
  const elementKey = Symbol.for('sgt.controller.element-id');
  if (!globalThis[documentKey]) globalThis[documentKey] = nonce();
  const documentId = globalThis[documentKey];
  const forms = [...document.forms];
  const formIndex = (el) => { const f = el.form || el.closest('form'); return f ? forms.indexOf(f) : -1; };
  const labelFor = (el) => { try {
      const lb = el.getAttribute('aria-labelledby');
      if (lb) { const t = lb.split(/\s+/).map(i => { const e = document.getElementById(i); return e ? e.innerText : ''; }).join(' ').trim(); if (t) return t; }
      const al = el.getAttribute('aria-label'); if (al && al.trim()) return al.trim();
      if (el.id && window.CSS && CSS.escape) { const l = document.querySelector('label[for="' + CSS.escape(el.id) + '"]'); if (l && l.innerText.trim()) return l.innerText.trim(); }
      const w = el.closest('label'); if (w && w.innerText.trim()) return w.innerText.trim();
      for (const a of ['placeholder','title','alt','name']) { const v = el.getAttribute(a); if (v && v.trim()) return v.trim(); }
      const own = (el.innerText || el.value || '').trim(); if (own) return own.slice(0, 80);
    } catch (e) {} return ''; };
  const roleOf = (el) => {
    const r = el.getAttribute('role'); if (r) return r.toLowerCase();
    const tag = el.tagName.toLowerCase();
    if (tag === 'a') return 'link'; if (tag === 'button') return 'button';
    if (tag === 'select') return 'combobox'; if (tag === 'textarea') return 'textbox';
    if (tag === 'input') { const t = (el.type||'text').toLowerCase();
      if (t === 'checkbox') return 'checkbox'; if (t === 'radio') return 'radio';
      if (['button','submit','reset','image'].includes(t)) return 'button';
      if (t === 'range') return 'slider'; return 'textbox'; }
    if (el.isContentEditable) return 'textbox'; return 'button'; };
  const INTERACTIVE = ['link','button','textbox','checkbox','radio','combobox','slider','menuitem','menuitemcheckbox','tab','switch','option','searchbox','spinbutton'];
  const SEL = 'a[href], button, input, select, textarea, [role], [contenteditable=""], [contenteditable="true"], [onclick], summary';
  const seen = new Set();
  for (const el of document.querySelectorAll(SEL)) { try {
    if (seen.has(el)) continue; seen.add(el);
    const tag = el.tagName.toLowerCase(); const type = (el.type || '').toLowerCase();
    if (tag === 'input' && type === 'hidden') continue;
    const r = el.getBoundingClientRect(); if (r.width < 1 || r.height < 1) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0) continue;
    if (el.offsetParent === null && cs.position !== 'fixed') continue;
    const role = roleOf(el);
    const hasHandler = el.hasAttribute('onclick') || ['a','button','input','select','textarea'].includes(tag) || el.isContentEditable;
    if (!INTERACTIVE.includes(role) && !hasHandler) continue;
    let value = null, state = null;
    if (type === 'password') { value = el.value ? '••••' : ''; }
    else if (tag === 'input' && (type === 'checkbox' || type === 'radio')) { state = el.checked ? 'checked' : 'unchecked'; }
    else if (tag === 'select') { const o = el.options[el.selectedIndex]; value = o ? o.text : ''; }
    else if (el.isContentEditable) { value = (el.innerText || '').slice(0, 200); }
    else if ('value' in el && typeof el.value === 'string') { value = el.value.slice(0, 200); }
    const ariaExp = el.getAttribute('aria-expanded'); if (ariaExp != null) state = ariaExp === 'true' ? 'expanded' : 'collapsed';
    const required = el.required === true || el.getAttribute('aria-required') === 'true';
    const submit = type === 'submit' || (tag === 'button' && (el.type === 'submit' || (!el.type && !!el.closest('form'))));
    // High-stakes signals come only from standardized element/form metadata.
    // URL words are not semantics and are never used as a permission classifier.
    let risk = null;
    if (type === 'reset') risk = 'clears the form';
    if (!risk && submit && el.form) { try {
      // STANDARDS only (language-neutral): a password field, or a payment field via
      // the autocomplete token spec. No localizable field-name guessing.
      if (el.form.querySelector('input[type=password], input[autocomplete*="cc-"]'))
        risk = 'submits a password or payment details';
    } catch (e) {} }
    id += 1; el.setAttribute('data-sgt-id', String(id));
    if (!el[elementKey]) el[elementKey] = nonce();
    const elementId = el[elementKey];
    out.push({ id, elementId, role, name: labelFor(el), value, state, required, submit, form: formIndex(el), risk,
      disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true' });
  } catch (e) {} }
  return { documentId, title: document.title, url: location.href, elements: out.slice(0, 200) };
})()"#;

#[cfg(test)]
mod tests {
    use super::unique_option_index;
    use serde_json::json;

    #[test]
    fn select_does_not_confuse_one_with_ten() {
        let options = vec![
            json!({"index": 0, "text": "10", "value": "10"}),
            json!({"index": 1, "text": "1", "value": "1"}),
        ];
        assert_eq!(unique_option_index(&options, "1").unwrap(), 1);
    }

    #[test]
    fn select_requires_a_unique_exact_normalized_option() {
        let options = vec![
            json!({"index": 0, "text": " Standard ", "value": "a"}),
            json!({"index": 1, "text": "standard", "value": "b"}),
        ];
        let error = unique_option_index(&options, "STANDARD").unwrap_err();
        assert!(error.to_string().contains("ambiguous option"));
    }
}
