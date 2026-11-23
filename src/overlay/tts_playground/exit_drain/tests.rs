use super::*;

#[test]
fn page_barrier_follows_queued_edit_and_thaw_is_order_independent() {
    let source = format!(
        r#"
const vm = require('node:vm');
const messages = [];
const body = {{ inert: false }};
const context = {{
  window: {{ ipc: {{ postMessage: value => messages.push(JSON.parse(value)) }},
             __TTS_SET_STATE__: () => {{}}, __TTS_PATCH_STATE__: () => {{}} }},
  document: {{ body, activeElement: null }},
  HTMLElement: class {{}},
}};
vm.createContext(context);
// The page posts this edit before the exit marker; WebView2 delivers posts
// from the same page in that order.
context.window.ipc.postMessage(JSON.stringify({{ cmd: 'set_draft_text', args: {{ text: 'last' }} }}));
vm.runInContext({barrier}, context);
if (!body.inert || messages.length !== 2 || messages[0].cmd !== 'set_draft_text' ||
    messages[1].cmd !== 'tts_exit_drain_ack' || messages[1].args.request !== 1) throw Error('barrier ordering');
vm.runInContext({thaw}, context);
if (body.inert) throw Error('thaw failed');
// Execute cancellation before a delayed barrier script. A timed-out request
// must never leave the page inert when its script eventually runs.
vm.runInContext({late_thaw}, context);
vm.runInContext({late_barrier}, context);
if (body.inert || messages.length !== 2) throw Error('late barrier was not retired');
// A later attempt can run before an earlier asynchronous thaw completes.
// It must retain the original inert state, not inherit the temporary freeze.
vm.runInContext({barrier_three}, context);
vm.runInContext({barrier_four}, context);
vm.runInContext({thaw_three}, context);
if (!body.inert) throw Error('older thaw released newer barrier');
vm.runInContext({thaw_four}, context);
if (body.inert) throw Error('newer barrier inherited temporary inert state');
// A runtime without inert still drains: editing is frozen by a capturing
// blocker, and the matching thaw removes exactly that blocker.
delete body.inert;
const listeners = new Map();
context.window.addEventListener = (type, callback, capture) => {{
  if (capture === true) listeners.set(type, callback);
}};
context.window.removeEventListener = (type, callback, capture) => {{
  if (capture === true && listeners.get(type) === callback) listeners.delete(type);
}};
vm.runInContext({barrier_five}, context);
if (messages.at(-1).cmd !== 'tts_exit_drain_ack' || messages.at(-1).args.request !== 5)
  throw Error('runtime without inert did not acknowledge the drain');
if ('inert' in body) throw Error('runtime without inert gained an inert property');
let blocked = 0;
for (const type of ['beforeinput', 'keydown', 'paste', 'drop']) {{
  listeners.get(type)?.({{ preventDefault() {{ blocked += 1; }}, stopImmediatePropagation() {{}} }});
}}
if (blocked !== 4) throw Error('edits were not blocked during the drain');
vm.runInContext({thaw_five}, context);
if (listeners.size !== 0) throw Error('thaw left an edit blocker installed');
const countBeforeUnready = messages.length;
body.inert = false;
let timer = null;
context.window.__TTS_SET_STATE__ = undefined;
context.window.setTimeout = callback => {{ timer = callback; }};
vm.runInContext({barrier_six}, context);
if (body.inert || messages.length !== countBeforeUnready || !timer) throw Error('unready page acknowledged');
context.window.__TTS_SET_STATE__ = () => {{}};
timer();
if (!body.inert || messages.at(-1).args.request !== 6) throw Error('ready page did not acknowledge');
"#,
        barrier = serde_json::to_string(&barrier_script(1)).unwrap(),
        thaw = serde_json::to_string(&thaw_script(1)).unwrap(),
        late_thaw = serde_json::to_string(&thaw_script(2)).unwrap(),
        late_barrier = serde_json::to_string(&barrier_script(2)).unwrap(),
        barrier_three = serde_json::to_string(&barrier_script(3)).unwrap(),
        barrier_four = serde_json::to_string(&barrier_script(4)).unwrap(),
        thaw_three = serde_json::to_string(&thaw_script(3)).unwrap(),
        thaw_four = serde_json::to_string(&thaw_script(4)).unwrap(),
        barrier_five = serde_json::to_string(&barrier_script(5)).unwrap(),
        thaw_five = serde_json::to_string(&thaw_script(5)).unwrap(),
        barrier_six = serde_json::to_string(&barrier_script(6)).unwrap(),
    );
    let result = std::process::Command::new("node")
        .arg("-e")
        .arg(source)
        .output()
        .expect("Node is required for frontend development");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn exit_admission_blocks_new_windows_until_guard_releases() {
    let mut lifecycle = super::super::lifecycle::WindowLifecycle::new();
    assert!(lifecycle.begin_exit());
    assert_eq!(lifecycle.exit_observation(), Ok(None));
    assert_eq!(
        lifecycle.show(),
        super::super::lifecycle::ShowAction::Deferred
    );
    assert!(!lifecycle.begin_exit());
    lifecycle.end_exit();
    assert_eq!(lifecycle.show(), super::super::lifecycle::ShowAction::Start);
    assert_eq!(lifecycle.exit_observation(), Err(()));
}

#[test]
fn missing_or_lost_ack_never_authorizes_exit() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let timeout = wait_for_ack(&receiver, Duration::ZERO).unwrap_err();
    assert!(timeout.to_string().contains("timed out"));
    drop(sender);
    let disconnected = wait_for_ack(&receiver, Duration::ZERO).unwrap_err();
    assert!(
        disconnected
            .to_string()
            .contains("lost its acknowledgement")
    );
}
