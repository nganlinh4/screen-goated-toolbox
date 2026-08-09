pub const DOCUMENT: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="preload" href="/font.ttf?v=__SGT_FONT_VERSION__" as="font" type="font/ttf" crossorigin>
<style>
@font-face{font-family:'Google Sans Flex';font-style:normal;font-weight:100 1000;
  font-stretch:25% 151%;font-display:block;
  src:url('/font.ttf?v=__SGT_FONT_VERSION__') format('truetype')}
html,body,#scene{position:fixed;inset:0;margin:0;overflow:hidden;background:transparent}
body{font-family:'Google Sans Flex';user-select:none}
#scene{pointer-events:none}
.font-prewarm{position:absolute;visibility:hidden;pointer-events:none;font:400 16px 'Google Sans Flex'}
.result-card{position:absolute;overflow:hidden;border-radius:12px;pointer-events:auto;
  left:0;top:0;box-shadow:0 8px 28px rgba(0,0,0,.22);contain:layout paint style}
.result-frame{display:block;width:100%;height:100%;border:0;background:transparent}
</style>
</head>
<body>
<span class="font-prewarm" aria-hidden="true">SGT</span>
<main id="scene"></main>
<script>
const scene = document.getElementById('scene');
const cards = new Map();
let currentThemeCss = '';
let highestStackOrder = 0;

function reportCardDiagnostic(id, entry, phase, details) {
  details = details || {};
  window.ipc.postMessage(JSON.stringify({
    type: 'card_diagnostic',
    id: Number(id),
    phase: phase,
    revision: entry ? entry.revision : 0,
    visible: entry ? entry.visible : false,
    ready: entry ? entry.ready : false,
    payload_len: Number(details.payloadLen || 0),
    text_len: Number(details.textLen || 0),
    opacity: String(details.opacity || ''),
    error: details.error ? String(details.error) : null
  }));
}

function ensureCard(id) {
  const key = String(id);
  let entry = cards.get(key);
  if (entry) return entry;
  const card = document.createElement('section');
  card.className = 'result-card';
  card.dataset.id = key;
  const frame = document.createElement('iframe');
  frame.className = 'result-frame';
  frame.setAttribute('sandbox', 'allow-scripts allow-forms allow-popups');
  card.appendChild(frame);
  scene.appendChild(card);
  entry = {
    card,
    frame,
    html: null,
    loadedHtml: null,
    ready: false,
    pendingContent: null,
    contentPhase: 'document',
    streaming: false,
    visible: false,
    navigationDepth: 0,
    navigationUrls: [],
    revision: 0
  };
  frame.addEventListener('load', function() {
    if (entry.loadedHtml === null) return;
    entry.ready = true;
    reportCardDiagnostic(id, entry, 'document_loaded', {
      payloadLen: entry.loadedHtml.length
    });
    if (currentThemeCss) {
      postCardMessage(entry, { type: 'theme_update', css: currentThemeCss });
    }
    const flushed = flushPendingContent(entry);
    if (entry.visible && entry.navigationDepth === 0 && !flushed) {
      postCardMessage(entry, { type: 'run_fit', streaming: entry.streaming });
    }
  });
  cards.set(key, entry);
  return entry;
}

function postCardMessage(entry, message) {
  if (!entry.ready || !entry.frame.contentWindow) return false;
  entry.frame.contentWindow.postMessage(message, '*');
  return true;
}

function queueCardContent(entry, message) {
  const priority = message.type === 'finalize' ? 2 : 1;
  const current = entry.pendingContent;
  if (current && current.revision === entry.revision && current.priority > priority) {
    reportCardDiagnostic(entry.card.dataset.id, entry, 'stale_content_ignored', {
      payloadLen: message.html.length
    });
    return false;
  }
  entry.pendingContent = {
    type: message.type,
    html: message.html,
    revision: entry.revision,
    priority: priority
  };
  reportCardDiagnostic(entry.card.dataset.id, entry, 'content_queued', {
    payloadLen: message.html.length
  });
  return true;
}

function flushPendingContent(entry) {
  const message = entry.pendingContent;
  if (!message || !entry.ready || !entry.visible) return false;
  if (message.revision !== entry.revision) {
    entry.pendingContent = null;
    return false;
  }
  if (!postCardMessage(entry, message)) return false;
  entry.pendingContent = null;
  return true;
}

function loadCardDocument(entry, html) {
  entry.ready = false;
  entry.revision++;
  entry.loadedHtml = html;
  reportCardDiagnostic(entry.card.dataset.id, entry, 'document_load_requested', {
    payloadLen: html.length
  });
  entry.frame.srcdoc = html;
}

function applyGeometry(entry, model) {
  const scale = window.devicePixelRatio || 1;
  entry.card.style.transform = 'translate3d(' + (model.rect.x / scale) + 'px,' +
    (model.rect.y / scale) + 'px,0)';
  entry.card.style.width = (model.rect.width / scale) + 'px';
  entry.card.style.height = (model.rect.height / scale) + 'px';
}

function applyStacking(entry, stackOrder) {
  const order = Number(stackOrder || 0);
  highestStackOrder = Math.max(highestStackOrder, order);
  entry.card.style.zIndex = String(order);
}

function raiseCard(entry, stackOrder) {
  const order = Math.max(highestStackOrder + 1, Number(stackOrder || 0));
  highestStackOrder = order;
  entry.card.style.zIndex = String(order);
}

function applyAppearance(entry, model) {
  const becameVisible = !entry.visible && model.visible;
  entry.card.style.background = model.background;
  entry.card.style.opacity = String(Math.max(0, Math.min(100, model.opacity)) / 100);
  entry.visible = model.visible;
  entry.card.hidden = !model.visible;
  return becameVisible;
}

function activateCard(entry, becameVisible) {
  if (!entry.visible || entry.navigationDepth !== 0) return;
  if (entry.loadedHtml === null && entry.html !== null) {
    loadCardDocument(entry, entry.html);
    return;
  }
  if (flushPendingContent(entry)) return;
  if (becameVisible && entry.ready) {
    postCardMessage(entry, { type: 'run_fit', streaming: entry.streaming });
  }
}

function upsertCard(model) {
  const entry = ensureCard(model.id);
  applyGeometry(entry, model);
  applyStacking(entry, model.stack_order);
  const becameVisible = applyAppearance(entry, model);
  const htmlChanged = entry.html !== model.html;
  entry.html = model.html;
  entry.streaming = Boolean(model.streaming);
  if (htmlChanged && entry.loadedHtml !== model.html) {
    entry.pendingContent = null;
    entry.contentPhase = entry.streaming ? 'streaming' : 'document';
    loadCardDocument(entry, model.html);
    return;
  }
  if (entry.streaming) entry.contentPhase = 'streaming';
  activateCard(entry, becameVisible);
}

function streamCard(model) {
  const entry = cards.get(String(model.id));
  if (!entry) {
    reportCardDiagnostic(model.id, null, 'stream_missing_card', {
      payloadLen: model.body.length
    });
    return;
  }
  const becameVisible = applyAppearance(entry, model);
  if (entry.contentPhase === 'finalized') {
    reportCardDiagnostic(model.id, entry, 'stale_stream_ignored', {
      payloadLen: model.body.length
    });
    return;
  }
  entry.streaming = true;
  entry.contentPhase = 'streaming';
  queueCardContent(entry, { type: 'stream_update', html: model.body });
  activateCard(entry, becameVisible);
}

function finalizeCard(model) {
  const entry = cards.get(String(model.id));
  if (!entry) {
    reportCardDiagnostic(model.id, null, 'finalize_missing_card', {
      payloadLen: model.body.length
    });
    return;
  }
  const becameVisible = applyAppearance(entry, model);
  entry.html = model.html;
  entry.streaming = false;
  entry.contentPhase = 'finalized';
  if (entry.loadedHtml === null) {
    entry.pendingContent = null;
    activateCard(entry, becameVisible);
    return;
  }
  entry.loadedHtml = model.html;
  queueCardContent(entry, { type: 'finalize', html: model.body });
  activateCard(entry, becameVisible);
}

function updateGeometry(model) {
  const entry = cards.get(String(model.id));
  if (!entry) return;
  applyGeometry(entry, model);
  const becameVisible = !entry.visible && model.visible;
  entry.visible = model.visible;
  entry.card.hidden = !model.visible;
  activateCard(entry, becameVisible);
}

function removeCard(id) {
  const key = String(id);
  const entry = cards.get(key);
  if (!entry) return;
  entry.card.remove();
  cards.delete(key);
}

function applyTheme(theme) {
  currentThemeCss = String(theme.css || '');
  for (const appearance of theme.cards || []) {
    const entry = cards.get(String(appearance.id));
    if (!entry) continue;
    entry.card.style.background = appearance.background;
    postCardMessage(entry, { type: 'theme_update', css: currentThemeCss });
  }
}

function reportNavigation(id, entry) {
  window.ipc.postMessage(JSON.stringify({
    type: 'navigation',
    id: Number(id),
    depth: entry.navigationDepth,
    max_depth: entry.navigationUrls.length
  }));
}

window.addEventListener('message', event => {
  if (!event.data) return;
  for (const [id, entry] of cards) {
    if (event.source !== entry.frame.contentWindow) continue;
    if (event.data.type === 'fit_diagnostic') {
      window.ipc.postMessage(JSON.stringify({
        type: 'fit_diagnostic',
        id: Number(id),
        payload: event.data.payload
      }));
      return;
    }
    if (event.data.type === 'card_diagnostic') {
      reportCardDiagnostic(id, entry, event.data.phase || 'bridge_unknown', {
        payloadLen: event.data.payload_len,
        textLen: event.data.text_len,
        opacity: event.data.opacity,
        error: event.data.error
      });
      return;
    }
    if (event.data.type === 'card_interaction') {
      raiseCard(entry);
      window.ipc.postMessage(JSON.stringify({
        type: 'interaction',
        id: Number(id)
      }));
      return;
    }
    if (event.data.type !== 'card_navigation') return;
    entry.navigationUrls.splice(entry.navigationDepth);
    entry.navigationUrls.push(event.data.url);
    entry.navigationDepth = entry.navigationUrls.length;
    reportNavigation(id, entry);
    return;
  }
});

window.applyHostCommand = function(command) {
  if (command.type === 'snapshot') {
    const incoming = new Set(command.cards.map(card => String(card.id)));
    for (const key of cards.keys()) if (!incoming.has(key)) removeCard(key);
    for (const card of command.cards) upsertCard(card);
  } else if (command.type === 'upsert') {
    upsertCard(command.card);
  } else if (command.type === 'stream') {
    streamCard(command.card);
  } else if (command.type === 'finalize') {
    finalizeCard(command.card);
  } else if (command.type === 'geometry') {
    for (const card of command.cards) updateGeometry(card);
  } else if (command.type === 'theme') {
    applyTheme(command.theme);
  } else if (command.type === 'raise') {
    const entry = cards.get(String(command.id));
    if (entry) applyStacking(entry, command.stack_order);
  } else if (command.type === 'remove') {
    removeCard(command.id);
  } else if (command.type === 'navigate_back') {
    const entry = cards.get(String(command.id));
    if (entry && entry.navigationDepth > 0) {
      entry.navigationDepth--;
      if (entry.navigationDepth === 0) {
        entry.pendingContent = null;
        entry.contentPhase = 'document';
        loadCardDocument(entry, entry.html);
      } else {
        entry.ready = false;
        entry.frame.src = entry.navigationUrls[entry.navigationDepth - 1];
      }
      reportNavigation(command.id, entry);
    }
  } else if (command.type === 'navigate_forward') {
    const entry = cards.get(String(command.id));
    if (entry && entry.navigationDepth < entry.navigationUrls.length) {
      entry.ready = false;
      entry.frame.src = entry.navigationUrls[entry.navigationDepth];
      entry.navigationDepth++;
      reportNavigation(command.id, entry);
    }
  }
};

setInterval(() => window.ipc.postMessage('renderer_heartbeat'), 1000);
const rendererFontStarted = performance.now();
document.fonts.load("400 16px 'Google Sans Flex'").then(faces => {
  if (!faces.length || !document.fonts.check("400 16px 'Google Sans Flex'")) {
    throw new Error('Google Sans Flex did not enter the loaded font set');
  }
  window.ipc.postMessage(JSON.stringify({
    type: 'font_ready',
    duration_ms: performance.now() - rendererFontStarted
  }));
  window.ipc.postMessage('renderer_ready');
}).catch(error => {
  window.ipc.postMessage(JSON.stringify({
    type: 'command_error',
    command: 'font_bootstrap',
    id: null,
    error: String(error && error.message ? error.message : error)
  }));
});
</script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::DOCUMENT;

    #[test]
    fn hidden_cards_preload_before_activation() {
        assert!(DOCUMENT.contains("if (htmlChanged && entry.loadedHtml !== model.html)"));
        assert!(!DOCUMENT.contains("if (entry.visible && htmlChanged"));
        assert!(DOCUMENT.contains("activateCard(entry, becameVisible)"));
    }

    #[test]
    fn stream_and_final_updates_wait_for_frame_readiness() {
        assert!(DOCUMENT.contains("queueCardContent(entry, { type: 'stream_update'"));
        assert!(DOCUMENT.contains("queueCardContent(entry, { type: 'finalize'"));
        assert!(DOCUMENT.contains("frame.addEventListener('load'"));
    }

    #[test]
    fn fitting_cannot_replace_content_waiting_for_document_load() {
        let post_start = DOCUMENT.find("function postCardMessage").unwrap();
        let post_end = DOCUMENT[post_start..]
            .find("function queueCardContent")
            .map(|offset| post_start + offset)
            .unwrap();
        let post_function = &DOCUMENT[post_start..post_end];

        assert!(!post_function.contains("pendingContent ="));
        assert!(DOCUMENT.contains("if (becameVisible && entry.ready)"));
        assert!(DOCUMENT.contains("const priority = message.type === 'finalize' ? 2 : 1"));
    }

    #[test]
    fn finalized_cards_reject_late_stream_updates() {
        assert!(DOCUMENT.contains("if (entry.contentPhase === 'finalized')"));
        assert!(DOCUMENT.contains("'stale_stream_ignored'"));
        assert!(DOCUMENT.contains("entry.contentPhase = 'finalized'"));
    }

    #[test]
    fn card_lifecycle_crosses_the_renderer_boundary() {
        assert!(DOCUMENT.contains("'document_load_requested'"));
        assert!(DOCUMENT.contains("'document_loaded'"));
        assert!(DOCUMENT.contains("event.data.type === 'card_diagnostic'"));
        assert!(DOCUMENT.contains("type: 'card_diagnostic'"));
    }

    #[test]
    fn renderer_readiness_is_gated_on_the_bundled_font() {
        assert!(DOCUMENT.contains("rel=\"preload\" href=\"/font.ttf"));
        assert!(DOCUMENT.contains("document.fonts.load"));
        assert!(DOCUMENT.contains("type: 'font_ready'"));
        assert!(DOCUMENT.contains("window.ipc.postMessage('renderer_ready')"));
        assert!(!DOCUMENT.contains("'Segoe UI'"));
    }

    #[test]
    fn position_updates_move_the_composited_card_without_relaying_out_its_frame() {
        assert!(DOCUMENT.contains("entry.card.style.transform = 'translate3d('"));
        assert!(!DOCUMENT.contains("entry.card.style.left ="));
        assert!(!DOCUMENT.contains("entry.card.style.top ="));
    }

    #[test]
    fn theme_and_interaction_updates_do_not_reload_card_documents() {
        assert!(DOCUMENT.contains("postCardMessage(entry, { type: 'theme_update'"));
        assert!(DOCUMENT.contains("event.data.type === 'card_interaction'"));
        assert!(DOCUMENT.contains("command.type === 'raise'"));
        assert!(!DOCUMENT.contains("function applyTheme(theme) {\n  loadCardDocument"));
    }
}
