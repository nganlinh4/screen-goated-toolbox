fn bridged_document(name: &str, document: &str) -> String {
    let bridge = format!(
        r#"<script>window.ipc={{postMessage:(message)=>parent.__sgtStatusFrameMessage({name:?},message)}};</script>"#
    );
    let document = document.replacen("<head>", &format!("<head>{bridge}"), 1);
    if name == "recording" {
        document
            .replace(".btn:hover {", ".btn:hover, .btn.sgt-native-hover {")
            .replace(".btn:active {", ".btn:active, .btn.sgt-native-active {")
    } else {
        document
    }
}

pub(super) fn document() -> String {
    let recording = bridged_document("recording", &crate::overlay::recording::ui::generate_html());
    let notification =
        bridged_document("notification", &crate::overlay::auto_copy_badge::document());
    let selection = bridged_document(
        "selection",
        &crate::overlay::text_selection::html::get_html("Select text..."),
    );
    let documents = serde_json::json!({
        "recording": recording,
        "notification": notification,
        "selection": selection,
    })
    .to_string()
    .replace("</", "<\\/");

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
* {{ box-sizing: border-box; }}
html, body {{ width: 100%; height: 100%; margin: 0; overflow: hidden; background: transparent; }}
.status-frame {{ position: absolute; display: none; border: 0; background: transparent; }}
#recording {{ pointer-events: auto; }}
#notification, #selection {{ pointer-events: none; }}
</style></head><body>
<iframe class="status-frame" id="recording"></iframe>
<iframe class="status-frame" id="notification"></iframe>
<iframe class="status-frame" id="selection"></iframe>
<script>
const documents = {documents};
const frames = Object.fromEntries(['recording','notification','selection'].map(name => [name, document.getElementById(name)]));
let virtualX = 0;
let virtualY = 0;
let displayScale = 1;
const frameRects = {{}};
let textVisible = false;
let imageVisible = false;
let captureVisible = true;
let notificationWatermark = 0;

function post(value) {{ window.ipc.postMessage(JSON.stringify(value)); }}
function frameWindow(name) {{ return frames[name].contentWindow; }}
function invoke(name, method, ...args) {{
  const fn = frameWindow(name)?.[method];
  if (typeof fn === 'function') fn(...args);
}}
function place(name, rect) {{
  frameRects[name] = rect;
  const scale = displayScale;
  const frame = frames[name];
  frame.style.left = `${{(rect.x - virtualX) / scale}}px`;
  frame.style.top = `${{(rect.y - virtualY) / scale}}px`;
  frame.style.width = `${{rect.width / scale}}px`;
  frame.style.height = `${{rect.height / scale}}px`;
  if (name === 'recording') requestAnimationFrame(reportRecordingRegions);
}}
function show(name) {{ frames[name].style.display = 'block'; }}
function hide(name) {{ frames[name].style.display = 'none'; }}
function recordingElementRect(selector) {{
  const frameRect = frames.recording.getBoundingClientRect();
  const elementRect = frameWindow('recording')?.document.querySelector(selector)?.getBoundingClientRect();
  if (!elementRect) return null;
  const scale = displayScale;
  return {{
    x: Math.round(virtualX + (frameRect.left + elementRect.left) * scale),
    y: Math.round(virtualY + (frameRect.top + elementRect.top) * scale),
    width: Math.round(elementRect.width * scale),
    height: Math.round(elementRect.height * scale)
  }};
}}
function reportRecordingRegions() {{
  const pause = recordingElementRect('#btn-pause');
  const cancel = recordingElementRect('.btn-close');
  if (pause && cancel) post({{type:'recording_regions',pause,cancel}});
}}
function recordingTheme(isDark) {{
  const values = isDark
    ? ['rgba(18, 18, 18, 0.85)','rgba(255, 255, 255, 0.1)','white','rgba(255, 255, 255, 0.7)','rgba(255, 255, 255, 0.05)','rgba(255, 255, 255, 0.15)','rgba(255, 255, 255, 0.8)','0 1px 2px rgba(0, 0, 0, 0.3)']
    : ['rgba(255, 255, 255, 0.92)','rgba(0, 0, 0, 0.1)','#222222','rgba(0, 0, 0, 0.6)','rgba(0, 0, 0, 0.05)','rgba(0, 0, 0, 0.1)','rgba(0, 0, 0, 0.7)','0 1px 2px rgba(255, 255, 255, 0.3)'];
  invoke('recording', 'updateTheme', isDark, ...values);
}}
function applyTheme(isDark) {{
  recordingTheme(isDark);
  invoke('notification', 'setTheme', isDark);
  invoke('selection', 'updateTheme', isDark);
}}
function updateSelectionFrameVisibility() {{
  frames.selection.style.visibility = captureVisible ? 'visible' : 'hidden';
  if (textVisible || imageVisible) show('selection'); else setTimeout(() => {{
    if (!textVisible && !imageVisible) hide('selection');
  }}, 160);
}}
function applySnapshot(scene) {{
  applyTheme(scene.is_dark);
  invoke('notification', 'resetNotifications');
  notificationWatermark = 0;
  hide('notification');
  place('notification', scene.notification_rect);
  if (scene.recording) {{
    place('recording', scene.recording.rect);
    invoke('recording', 'updateState', scene.recording.state, scene.recording.rms);
    frameWindow('recording').document.body.classList.toggle('visible', scene.recording.visible);
    if (scene.recording.visible) show('recording'); else hide('recording');
  }} else {{
    invoke('recording', 'hideState'); hide('recording');
  }}
  const notificationItems = (scene.notifications || []).map(notification => ({{
    order: notification.id || 0, notification
  }}));
  if (scene.progress) notificationItems.push({{order: scene.progress.order || 0, progress: scene.progress}});
  notificationItems.sort((left, right) => left.order - right.order);
  for (const item of notificationItems) {{
    show('notification');
    if (item.notification) {{
      const notification = item.notification;
      notificationWatermark = Math.max(notificationWatermark, notification.id || 0);
      invoke('notification', 'addNotification', notification.title, notification.snippet,
        notification.kind, notification.duration_ms);
    }} else {{
      const progress = item.progress;
      invoke('notification', 'upsertProgressNotification', progress.title, progress.snippet, progress.progress);
    }}
  }}
  const selection = scene.selection;
  place('selection', selection.rect);
  textVisible = selection.text_visible;
  imageVisible = selection.image_visible;
  captureVisible = selection.capture_visible;
  invoke('selection', 'updateState', selection.selecting, selection.text);
  invoke('selection', 'updateImageText', selection.image_text);
  if (textVisible) invoke('selection', 'playEntry');
  if (imageVisible) invoke('selection', 'showImageBadge');
  updateSelectionFrameVisibility();
}}
window.applyStatusCommand = command => {{
  switch (command.type) {{
    case 'snapshot': applySnapshot(command.scene); break;
    case 'theme': applyTheme(command.is_dark); break;
    case 'recording_prepare':
      place('recording', command.scene.rect); hide('recording'); invoke('recording', 'resetState'); break;
    case 'recording_show':
      place('recording', command.rect); show('recording');
      setTimeout(() => frameWindow('recording').document.body.classList.add('visible'), 50); break;
    case 'recording_update': invoke('recording', 'updateState', command.state, command.rms); break;
    case 'recording_hide': invoke('recording', 'hideState'); hide('recording'); break;
    case 'notification_add':
      place('notification', command.rect);
      notificationWatermark = Math.max(notificationWatermark, command.notification.id || 0);
      show('notification');
      invoke('notification', 'addNotification', command.notification.title, command.notification.snippet,
        command.notification.kind, command.notification.duration_ms); break;
    case 'progress_upsert':
      place('notification', command.rect);
      show('notification');
      invoke('notification', 'upsertProgressNotification', command.progress.title,
        command.progress.snippet, command.progress.progress); break;
    case 'progress_remove': invoke('notification', 'removeProgressNotification'); break;
    case 'selection_show':
      place('selection', command.rect); textVisible = true; show('selection');
      invoke('selection', 'updateState', false, command.text); invoke('selection', 'playEntry');
      updateSelectionFrameVisibility(); break;
    case 'selection_hide': textVisible = false; invoke('selection', 'playExit'); updateSelectionFrameVisibility(); break;
    case 'selection_update': invoke('selection', 'updateState', command.selecting, command.text); break;
    case 'selection_position': place('selection', command.rect); break;
    case 'image_badge_show':
      place('selection', command.rect); imageVisible = true; show('selection');
      if (!textVisible) invoke('selection', 'playExit');
      invoke('selection', 'updateImageText', command.text); invoke('selection', 'showImageBadge');
      updateSelectionFrameVisibility(); break;
    case 'image_badge_hide': imageVisible = false; invoke('selection', 'hideImageBadge'); updateSelectionFrameVisibility(); break;
    case 'selection_capture':
      captureVisible = command.visible; updateSelectionFrameVisibility();
      requestAnimationFrame(() => requestAnimationFrame(() => post({{type:'selection_capture_applied',request_id:command.request_id}}))); break;
  }}
}};
window.moveStatusRecording = rect => place('recording', rect);
window.statusDisplayChanged = display => {{
  virtualX = display.x;
  virtualY = display.y;
  displayScale = Math.max(1, Number(display.scale) || 1);
  for (const [name, rect] of Object.entries(frameRects)) place(name, rect);
}};
window.setStatusRecordingPointer = (target, active) => {{
  const pause = frameWindow('recording')?.document.querySelector('#btn-pause');
  const cancel = frameWindow('recording')?.document.querySelector('.btn-close');
  for (const [name, element] of [['pause', pause], ['cancel', cancel]]) {{
    element?.classList.toggle('sgt-native-hover', target === name);
    element?.classList.toggle('sgt-native-active', active && target === name);
  }}
}};
window.__sgtStatusFrameMessage = (frame, message) => {{
  if (frame === 'recording') {{
    if (message === 'ready') post({{type:'recording_ready'}});
    else if (message === 'pause_toggle') post({{type:'recording_pause_toggle'}});
    else if (message === 'cancel' || message === 'close') post({{type:'recording_cancel'}});
  }} else if (frame === 'notification' && message === 'finished') {{
    hide('notification'); post({{type:'notification_finished',through_id:notificationWatermark}});
  }} else if (String(message).startsWith('error:')) {{
    post({{type:'renderer_error',source:frame,error:String(message)}});
  }}
}};
Promise.all(Object.entries(frames).map(([name, frame]) => new Promise(resolve => {{
  frame.addEventListener('load', resolve, {{once:true}}); frame.srcdoc = documents[name];
}}))).then(() => Promise.all(Object.values(frames).map(frame => frame.contentDocument.fonts?.ready))).then(() => {{
  post({{type:'ready'}}); setInterval(() => post({{type:'heartbeat'}}), 1000);
}}).catch(error => post({{type:'renderer_error',source:'bootstrap',error:String(error)}}));
</script></body></html>"#,
        documents = documents
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_keeps_passive_frames_non_interactive() {
        let html = document();
        assert!(html.contains("#notification, #selection { pointer-events: none; }"));
        assert!(html.contains("#recording { pointer-events: auto; }"));
        assert!(html.contains("recording_regions"));
        assert!(html.contains("setStatusRecordingPointer"));
        assert!(html.contains(".btn:hover, .btn.sgt-native-hover"));
        assert!(html.contains("displayScale = Math.max(1, Number(display.scale) || 1)"));
        assert!(html.contains("Object.entries(frameRects)"));
        assert!(html.contains("invoke('notification', 'resetNotifications')"));
        assert!(html.contains("classList.toggle('visible', scene.recording.visible)"));
    }
}
