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

use super::super::browser as web;
use super::verify::ReadBack;
use super::world::{ElHandle, IndexedElement, WorldState};
use super::{ActCtx, Surface, Verb};

pub struct BrowserSurface;

impl BrowserSurface {
    pub fn new() -> Self {
        BrowserSurface
    }

    /// Set a <select> (in its frame) to the option matching `value`, firing
    /// input/change so frameworks notice.
    fn select(&self, selector: &str, session: Option<&str>, value: &str) -> Result<()> {
        let v = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
        let expr = format!(
            r#"(() => {{ const e = document.querySelector('{selector}'); if(!e) return 'not found';
                const opts=[...(e.options||[])];
                let o = opts.find(o=>o.text.trim()==={v}||o.value==={v})
                    || opts.find(o=>o.text.toLowerCase().includes(({v}).toLowerCase()));
                if(!o) return 'no option matching';
                e.value=o.value; e.dispatchEvent(new Event('input',{{bubbles:true}}));
                e.dispatchEvent(new Event('change',{{bubbles:true}})); return 'ok'; }})()"#
        );
        match web::eval_value_in(&expr, session)?.as_str() {
            Some("ok") => Ok(()),
            Some(other) => anyhow::bail!("{other}"),
            None => anyhow::bail!("select failed"),
        }
    }

    /// Click an element inside a cross-origin iframe via a synthetic `el.click()`.
    /// (A trusted coordinate click would need the OOPIF's screen offset, which it
    /// doesn't expose; a JS click drives the inputs/buttons cross-origin login /
    /// payment widgets are built from.)
    fn click_in_frame(&self, selector: &str, session: &str) -> Result<()> {
        let expr = format!(
            r#"(() => {{ const e = document.querySelector('{selector}'); if(!e) return false;
                e.scrollIntoView({{block:'center'}}); e.click(); return true; }})()"#
        );
        match web::eval_value_in(&expr, Some(session))? {
            Value::Bool(true) => Ok(()),
            _ => anyhow::bail!("element not found in frame"),
        }
    }
}

impl Surface for BrowserSurface {
    fn observe(&mut self) -> Result<WorldState> {
        // Top frame first; then each cross-origin child frame, its @ids continuing
        // from the running total so every id is globally unique + resolvable.
        let mut world = parse_world(&web::eval_value(&perception_js(0))?, None);
        for sid in web::child_frames() {
            let base = world.elements.len() as u32;
            if let Ok(v) = web::eval_value_in(&perception_js(base), Some(&sid)) {
                world
                    .elements
                    .extend(parse_world(&v, Some(sid.clone())).elements);
            }
        }
        Ok(world)
    }

    fn execute(
        &mut self,
        el: &IndexedElement,
        verb: Verb,
        value: Option<&str>,
        _act: &ActCtx,
    ) -> Result<()> {
        let ElHandle::Browser { selector, session } = &el.handle else {
            anyhow::bail!("not a browser element");
        };
        match (verb, session.as_deref()) {
            (Verb::Fill, sess) => ok_or_err(&web::fill_in(selector, value.unwrap_or(""), sess)),
            (Verb::Select, sess) => self.select(selector, sess, value.unwrap_or("")),
            // Top frame: trusted coordinate click. Cross-origin frame: JS click.
            (Verb::Click | Verb::Submit | Verb::Toggle, None) => {
                ok_or_err(&web::click_selector(selector))
            }
            (Verb::Click | Verb::Submit | Verb::Toggle, Some(sess)) => {
                self.click_in_frame(selector, sess)
            }
        }
    }

    fn read_back(&mut self, el: &IndexedElement) -> ReadBack {
        let ElHandle::Browser { selector, session } = &el.handle else {
            return ReadBack::default();
        };
        // Passwords read back as null (masked — can't compare) → an honest "unknown".
        let expr = format!(
            r#"(() => {{ const e = document.querySelector('{}'); if(!e) return null;
                const t=(e.type||'').toLowerCase();
                return {{ value: (t==='password' ? null
                        : ('value' in e ? e.value : (e.isContentEditable ? e.innerText : null))),
                    validity: e.validationMessage || null }}; }})()"#,
            selector
        );
        match web::eval_value_in(&expr, session.as_deref()) {
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

fn ok_or_err(v: &Value) -> Result<()> {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        anyhow::bail!(
            "{}",
            v.get("error")
                .and_then(Value::as_str)
                .unwrap_or("action failed")
        )
    }
}

fn parse_world(v: &Value, session: Option<String>) -> WorldState {
    let elements = v
        .get("elements")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(|e| parse_el(e, &session)).collect())
        .unwrap_or_default();
    WorldState {
        elements,
        url: v.get("url").and_then(Value::as_str).map(str::to_string),
        title: v.get("title").and_then(Value::as_str).map(str::to_string),
    }
}

fn parse_el(e: &Value, session: &Option<String>) -> Option<IndexedElement> {
    let id = e.get("id").and_then(Value::as_u64)? as u32;
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
/// title/url. Skips hidden / zero-area / `type=hidden` nodes. `risk` is
/// LANGUAGE-NEUTRAL only (form-reset, logout/pay/delete path, sensitive-field
/// submit). Capped at 200 elements. Run it via `perception_js(base)`, not raw.
const PERCEPTION_JS: &str = r#"(() => {
  const out = []; let id = 0;
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
    // LANGUAGE-NEUTRAL high-stakes signal (structure only): form-reset, logout / pay
    // / delete paths, or a submit of a form that carries a password or payment field.
    const path = ((el.getAttribute('href')||'') + ' ' + (el.getAttribute('formaction')||'')).toLowerCase();
    let risk = null;
    if (type === 'reset') risk = 'clears the form';
    else if (/log[\s._-]?out|sign[\s._-]?out|logout|signout|log[\s._-]?off/.test(path)) risk = 'signs you out';
    else if (/delete[\s._-]?acc|deactivat|close[\s._-]?acc|delete[\s._-]?profile|cancel[\s._-]?subscription/.test(path)) risk = 'deletes or closes an account';
    else if (/checkout|\/payment|\/billing|\/subscribe|\/purchase|\/donate|\/order|payout/.test(path)) risk = 'starts a payment or money transfer';
    if (!risk && submit && el.form) { try {
      // STANDARDS only (language-neutral): a password field, or a payment field via
      // the autocomplete token spec. No localizable field-name guessing.
      if (el.form.querySelector('input[type=password], input[autocomplete*="cc-"]'))
        risk = 'submits a password or payment details';
    } catch (e) {} }
    id += 1; el.setAttribute('data-sgt-id', String(id));
    out.push({ id, role, name: labelFor(el), value, state, required, submit, form: formIndex(el), risk,
      disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true' });
  } catch (e) {} }
  return { title: document.title, url: location.href, elements: out.slice(0, 200) };
})()"#;
