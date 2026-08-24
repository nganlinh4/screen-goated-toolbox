const scene = document.getElementById('scene'); const isolatedOrigin = __SGT_ISOLATED_ORIGIN_JSON__;
const cards = new Map();
const cardStyleText = __SGT_CARD_CSS_JSON__; let currentThemeCss = '';
let highestStackOrder = 0;
let activeFit = null; const pendingFits = new Map();
let sharedCardSheet = null;
function reportCardDiagnostic(id, entry, phase, details) {
  details = details || {};
  window.ipc.postMessage(JSON.stringify({
    type: 'card_diagnostic',
    id: Number(id),
    phase: phase,
    revision: Number(details.revision === undefined
      ? (entry ? entry.contentRevision : 0)
      : details.revision),
    visible: entry ? entry.visible : false,
    ready: entry ? entry.ready : false,
    payload_len: Number(details.payloadLen || 0),
    text_len: Number(details.textLen || 0),
    opacity: String(details.opacity || ''),
    error: details.error ? String(details.error.message || details.error) : null
  }));
}
function installCardStyles(shadow) {
  if (typeof CSSStyleSheet !== 'undefined' && 'adoptedStyleSheets' in shadow) {
    if (!sharedCardSheet) {
      sharedCardSheet = new CSSStyleSheet();
      sharedCardSheet.replaceSync(cardStyleText);
    }
    shadow.adoptedStyleSheets = [sharedCardSheet];
    return;
  }
  const style = document.createElement('style');
  style.textContent = cardStyleText;
  shadow.appendChild(style);
}
function publishSelectionCopy(event, id) {
  const selection = window.getSelection();
  const text = selection ? selection.toString() : '';
  if (!text) return;
  event.preventDefault();
  window.ipc.postMessage(JSON.stringify({
    action: 'copy_selection', hwnd: String(id), text: text
  }));
}
function ensureCard(id) {
  const key = String(id);
  let entry = cards.get(key);
  if (entry) return entry;
  const card = document.createElement('section');
  card.className = 'result-card';
  card.dataset.id = key;
  card.dataset.surface = 'result';
  const directHost = document.createElement('div');
  directHost.className = 'direct-host';
  const shadow = directHost.attachShadow({ mode: 'open' });
  installCardStyles(shadow);
  const bodyElement = document.createElement('div');
  bodyElement.className = 'result-body';
  bodyElement.dataset.sgtMode = 'result';
  shadow.appendChild(bodyElement);
  const frame = document.createElement('iframe');
  frame.className = 'result-frame';
  frame.hidden = true;
  frame.referrerPolicy = 'no-referrer';
  frame.setAttribute('sandbox', 'allow-scripts allow-forms allow-modals allow-popups allow-downloads allow-pointer-lock allow-presentation');
  const backdrop = document.createElement('img');
  backdrop.className = 'region-backdrop';
  backdrop.hidden = true;
  const processing = window.__SGT_CREATE_PROCESSING_AURA__();
  card.appendChild(backdrop);
  card.appendChild(directHost);
  card.appendChild(frame);
  card.appendChild(processing.element);
  scene.appendChild(card);
  entry = {
    card: card,
    backdrop: backdrop,
    directHost: directHost,
    bodyElement: bodyElement,
    frame: frame,
    processing: processing,
    body: '',
    document: null,
    loadedDocument: 'shared',
    mode: 'direct',
    ready: true,
    fontReady: true,
    pendingContent: null,
    commandPort: null,
    contentPhase: 'document',
    streaming: false,
    visible: false,
    navigationDepth: 0,
    navigationUrls: [],
    refining: false, navigationLoading: false, externalNavigation: false,
    processingEffect: 'standard', streamingEnabled: true,
    contentRevision: 0, revision: 0, resizeFit: 0,
    awaitingSettledReveal: false, settledRevealRevision: 0,
    pendingSettledPaint: null,
    directState: {
      wordCount: 0,
      renderCount: 0,
      overflowObserver: null,
      reveal: { queue: [], active: false, lastRevealedIndex: -1, lastTick: 0, credits: 0, generation: 0 },
      fit: {}
    }
  };
  entry.directRuntime = window.__SGT_CREATE_DIRECT_RUNTIME__(entry, {
    requestFit: function(streaming) { queueFit(entry, streaming); },
    diagnostic: function(phase, error) {
      reportCardDiagnostic(id, entry, phase, { error: error });
    }
  });
  entry.resizeRuntime = window.__SGT_CARD_RESIZE__.attach(entry);
  function activateIsolatedBridge() {
    if (entry.mode !== 'isolated') return;
    if (entry.ready || entry.navigationDepth !== 0) return;
    entry.ready = true;
    entry.fontReady = true;
    if (currentThemeCss) postCardMessage(entry, { type: 'theme_update', css: currentThemeCss });
    postCardMessage(entry, { type: 'activate_font' });
    const flushed = flushPendingContent(entry);
    if (entry.visible && entry.navigationDepth === 0 && !flushed) {
      queueFit(entry, entry.streaming);
    }
  }
  entry.activateIsolatedBridge = activateIsolatedBridge;
  frame.addEventListener('load', function() {
    if (entry.mode !== 'isolated') return;
    reportCardDiagnostic(id, entry, 'document_loaded', {
      payloadLen: entry.document ? entry.document.length : 0
    });
    activateIsolatedBridge();
  });
  card.addEventListener('pointerdown', function() {
    raiseCard(entry);
    window.ipc.postMessage(JSON.stringify({ type: 'interaction', id: Number(id) }));
  }, true);
  shadow.addEventListener('click', function(event) {
    const path = event.composedPath ? event.composedPath() : [];
    const anchor = path.find(node => node && node.tagName === 'A' && node.href);
    if (!anchor || event.defaultPrevented) return;
    if (!/^https?:\/\//i.test(anchor.href)) return;
    event.preventDefault();
    navigateTo(entry, anchor.href);
  }, true);
  shadow.addEventListener('copy', function(event) {
    publishSelectionCopy(event, id);
  }, true);
  cards.set(key, entry);
  reportCardDiagnostic(id, entry, 'shared_surface_ready', {});
  return entry;
}
function postCardMessage(entry, message) {
  if (entry.mode !== 'isolated' || !entry.ready || entry.navigationDepth !== 0) return false;
  message.card_id = entry.card.dataset.id;
  if (entry.commandPort) entry.commandPort.postMessage(message);
  else if (entry.frame.contentWindow) entry.frame.contentWindow.postMessage(message, '*');
  else return false;
  return true;
}
function cancelActiveFit(entry) {
  pendingFits.delete(entry.card.dataset.id);
  if (!activeFit || activeFit.entry !== entry) return;
  clearTimeout(activeFit.timeout);
  activeFit = null;
  scheduleFit();
}
function queueFit(entry, streaming) {
  if (!entry.ready || !entry.fontReady || !entry.visible || entry.navigationDepth !== 0 || !String(entry.body || '').trim()) return;
  const key = entry.card.dataset.id;
  const current = pendingFits.get(key);
  const next = {
    entry: entry,
    streaming: Boolean(streaming),
    revision: entry.revision,
    contentRevision: entry.contentRevision,
    priority: streaming ? 1 : 2
  };
  if (!current || next.priority >= current.priority || next.revision > current.revision) {
    pendingFits.set(key, next);
  }
  scheduleFit();
}
function runDirectFit(entry, streaming, settleBeforeReveal) {
  if (entry.sourceReplacement === true) {
    completeFit(entry);
    return;
  }
  window.__SGT_FIT_CONTEXT__ = {
    state: entry.directState.fit,
    body: entry.bodyElement,
    viewport: entry.card,
    fontReady: true,
    settleBeforeReveal: Boolean(settleBeforeReveal),
    reportDiagnostic: function(payload) {
      window.ipc.postMessage(JSON.stringify({
        type: 'fit_diagnostic', id: Number(entry.card.dataset.id), payload: payload
      }));
    },
    requestRefinement: function() {
      setTimeout(function() { queueFit(entry, true); }, 0);
    },
    complete: function() { completeFit(entry); }
  };
  try {
    window.__SGT_RUN_FIT__(Boolean(streaming));
  } catch (error) {
    reportCardDiagnostic(entry.card.dataset.id, entry, 'fit_failed', { error: error });
    completeFit(entry);
  } finally {
    window.__SGT_FIT_CONTEXT__ = null;
  }
}
function scheduleFit() {
  if (activeFit || pendingFits.size === 0) return;
  requestAnimationFrame(function() {
    if (activeFit) return;
    let selectedKey = null;
    let selected = null;
    for (const [key, candidate] of pendingFits) {
      if (!selected || candidate.priority > selected.priority) {
        selectedKey = key;
        selected = candidate;
      }
    }
    if (!selected) return;
    pendingFits.delete(selectedKey);
    const entry = selected.entry;
    if (!entry.ready || !entry.fontReady || !entry.visible || entry.navigationDepth !== 0
        || selected.revision !== entry.revision
        || selected.contentRevision !== entry.contentRevision) {
      scheduleFit();
      return;
    }
    const settleBeforeReveal = !selected.streaming && entry.awaitingSettledReveal
      && entry.settledRevealRevision === selected.contentRevision;
    activeFit = {
      entry: entry,
      streaming: selected.streaming,
      contentRevision: selected.contentRevision,
      timeout: setTimeout(function() {
        reportCardDiagnostic(entry.card.dataset.id, entry, 'fit_timeout', {});
        revealSettledContent(entry, selected.contentRevision);
        activeFit = null;
        scheduleFit();
      }, 2000)
    };
    if (entry.mode === 'direct') runDirectFit(entry, selected.streaming, settleBeforeReveal);
    else postCardMessage(entry, { type: 'run_fit', streaming: selected.streaming,
      settle_before_reveal: settleBeforeReveal });
  });
}
function completeFit(entry) {
  if (!activeFit || activeFit.entry !== entry) return;
  const completed = activeFit;
  if (!completed.streaming && completed.contentRevision === entry.contentRevision) {
    reportCardDiagnostic(entry.card.dataset.id, entry, 'final_fit_completed', {});
    revealSettledContent(entry, completed.contentRevision);
    window.__SGT_BUTTON_SCENE__?.pulseCompletion(entry.card.dataset.id);
  }
  clearTimeout(completed.timeout);
  activeFit = null;
  scheduleFit();
}
function applyDirectContent(entry, message) {
  try {
    entry.directRuntime.apply({
      html: message.html,
      runInlineSizing: true,
      finalizing: message.type === 'finalize',
      animateNewWords: message.type === 'stream_update',
      settleBeforeReveal: Boolean(message.settle_before_reveal),
      sourceReplacement: entry.sourceReplacement === true,
      preferredFontSize: entry.preferredFontSize,
      sourceVertical: entry.sourceVertical === true,
      sourceRegions: entry.sourceRegions,
      sourceSegments: entry.sourceSegments
    });
    entry.bodyElement.dataset.sgtMode = message.refining ? 'refining' : 'result';
    reportCardDiagnostic(entry.card.dataset.id, entry,
      message.type === 'finalize' ? 'finalize_applied' : 'stream_applied', {
        revision: message.content_revision,
        payloadLen: message.html.length,
        textLen: (entry.bodyElement.innerText || '').trim().length
      });
    reportOrDeferPaint(entry,
      message.type === 'finalize' ? 'final' : 'stream', message.content_revision);
    if (message.type === 'finalize') entry.directRuntime.initGrids();
    queueFit(entry, message.type === 'stream_update');
    return true;
  } catch (error) {
    reportCardDiagnostic(entry.card.dataset.id, entry,
      message.type === 'finalize' ? 'finalize_failed' : 'stream_failed', { error: error });
    return false;
  }
}
function queueCardContent(entry, message) {
  const priority = message.type === 'finalize' ? 2 : 1;
  const current = entry.pendingContent;
  if (current && current.revision === entry.revision && current.priority > priority) return false;
  entry.pendingContent = {
    type: message.type,
    html: message.html,
    refining: Boolean(message.refining),
    content_revision: ++entry.contentRevision,
    revision: entry.revision,
    priority: priority
  };
  reportCardDiagnostic(entry.card.dataset.id, entry, 'content_queued', {
    revision: entry.contentRevision,
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
  const applied = entry.mode === 'direct'
    ? applyDirectContent(entry, message)
    : postCardMessage(entry, message);
  if (applied) entry.pendingContent = null;
  return applied;
}
function documentKey(documentHtml) {
  return documentHtml === null ? 'shared' : 'inline:' + documentHtml;
}
function useDirectSurface(entry) {
  if (entry.mode === 'direct') return;
  cancelActiveFit(entry);
  entry.mode = 'direct';
  entry.ready = true;
  entry.fontReady = true;
  entry.revision++;
  entry.loadedDocument = 'shared';
  if (entry.commandPort) entry.commandPort.close();
  entry.commandPort = null;
  entry.frame.hidden = true;
  entry.directHost.hidden = false;
  reportCardDiagnostic(entry.card.dataset.id, entry, 'shared_surface_ready', {});
}
function loadIsolatedDocument(entry, documentHtml) {
  cancelActiveFit(entry);
  entry.mode = 'isolated';
  entry.ready = false;
  entry.fontReady = false;
  entry.revision++;
  entry.loadedDocument = documentKey(documentHtml);
  if (entry.commandPort) entry.commandPort.close();
  entry.commandPort = null;
  entry.directHost.hidden = true;
  entry.frame.hidden = false;
  reportCardDiagnostic(entry.card.dataset.id, entry, 'document_load_requested', {
    payloadLen: documentHtml ? documentHtml.length : 0
  });
  entry.frame.removeAttribute('src');
  entry.frame.srcdoc = documentHtml.replace('__SGT_CARD_FRAME_IDENTITY__',
    entry.card.dataset.id + ':' + entry.revision);
}
function selectSurface(entry, documentHtml) {
  const changed = documentKey(entry.document) !== documentKey(documentHtml);
  entry.document = documentHtml;
  if (documentHtml === null) {
    if (entry.mode !== 'direct') useDirectSurface(entry);
  } else if (entry.mode !== 'isolated' || changed || entry.navigationDepth > 0) {
    loadIsolatedDocument(entry, documentHtml);
  }
}
function applyGeometry(entry, model) {
  const scale = window.devicePixelRatio || 1; entry.card.style.setProperty('--sgt-box-radius', (__SGT_BOX_RADIUS_PX__ / scale) + 'px');
  entry.card.style.translate = '';
  const width = model.rect.width / scale;
  const height = model.rect.height / scale;
  const resized = entry.card.clientWidth !== width || entry.card.clientHeight !== height;
  entry.card.style.transform = 'translate3d(' + (model.rect.x / scale) + 'px,' +
    (model.rect.y / scale) + 'px,0)';
  entry.card.style.width = width + 'px';
  entry.card.style.height = height + 'px';
  entry.processing.resize(width, height, scale);
  if (resized && entry.ready && entry.visible) {
    clearTimeout(entry.resizeFit);
    entry.resizeFit = setTimeout(function() { queueFit(entry, entry.streaming); }, 40);
  }
}
function applyStacking(entry, stackOrder) {
  const order = Number(stackOrder || 0);
  highestStackOrder = Math.max(highestStackOrder, order);
  const current = Number(entry.card.style.zIndex || 0);
  if (order >= current) entry.card.style.zIndex = String(order);
}
function raiseCard(entry, stackOrder) {
  const order = Math.max(highestStackOrder + 1, Number(stackOrder || 0));
  highestStackOrder = order;
  entry.card.style.zIndex = String(order);
}
function applyAppearance(entry, model) {
  const becameVisible = !entry.visible && model.visible;
  entry.card.dataset.presentation = model.presentation || 'standard';
  entry.sourceReplacement = model.source_replacement === true;
  entry.sourceVertical = model.source_vertical === true;
  entry.sourceRegions = Array.isArray(model.source_regions) ? model.source_regions : [];
  entry.sourceSegments = Array.isArray(model.source_segments) ? model.source_segments : [];
  const preferredFontSize = Number(model.preferred_font_size);
  const scale = window.devicePixelRatio || 1;
  entry.preferredFontSize = Number.isFinite(preferredFontSize) && preferredFontSize > 0
    ? preferredFontSize / scale
    : null;
  const backdropUrl = model.backdrop_data_url || '';
  if (entry.backdrop.dataset.url !== backdropUrl) {
    entry.backdrop.dataset.url = backdropUrl;
    entry.backdrop.src = backdropUrl;
  }
  entry.backdrop.hidden = !backdropUrl;
  if (model.foreground_color) {
    entry.directHost.style.setProperty('--text-color', model.foreground_color, 'important');
  } else {
    entry.directHost.style.removeProperty('--text-color');
  }
  entry.card.style.background = model.background;
  entry.card.style.opacity = String(Math.max(0, Math.min(100, model.opacity)) / 100);
  entry.visible = model.visible;
  entry.card.hidden = !model.visible;
  return becameVisible;
}
function activateCard(entry, becameVisible) {
  if (!entry.visible || entry.navigationDepth !== 0) return;
  if (flushPendingContent(entry)) return;
  if (becameVisible && entry.ready) queueFit(entry, entry.streaming);
}
function applyContentModel(entry, model, type) {
  const becameVisible = applyAppearance(entry, model);
  if (type !== 'finalize' && entry.contentPhase === 'finalized') return;
  entry.body = model.body;
  entry.streaming = type !== 'finalize';
  entry.streamingEnabled = Boolean(model.streaming_enabled);
  entry.refining = Boolean(model.refining);
  entry.navigationLoading = Boolean(model.navigation_loading);
  entry.externalNavigation = model.external_navigation === true;
  entry.processingEffect = model.processing_effect === 'minimal' ? 'minimal' : 'standard';
  const processing = entry.refining || entry.navigationLoading;
  entry.card.dataset.processing = processing ? 'true' : 'false';
  entry.card.dataset.processingEffect = entry.processingEffect;
  entry.processing.setState(processing, entry.processingEffect);
  if (type === 'finalize') entry.contentPhase = 'finalized';
  else if (entry.contentPhase !== 'finalized') entry.contentPhase = 'streaming';
  const nextDocument = model.document === undefined ? null : model.document;
  if (documentKey(entry.document) !== documentKey(nextDocument)) entry.pendingContent = null;
  const settleBeforeReveal = type === 'finalize' && !entry.streamingEnabled;
  if (window.__SGT_APPLY_EXTERNAL_SURFACE__(entry, model, nextDocument, becameVisible, activateCard)) return;
  if (nextDocument !== null) {
    entry.pendingContent = null;
    entry.contentRevision++;
    selectSurface(entry, nextDocument);
    reportCardDiagnostic(entry.card.dataset.id, entry, 'document_content_committed', {
      revision: entry.contentRevision,
      payloadLen: nextDocument.length
    });
    activateCard(entry, becameVisible);
    return;
  }
  selectSurface(entry, null);
  queueCardContent(entry, { type: type, html: model.body, refining: model.refining,
    settle_before_reveal: settleBeforeReveal });
  if (settleBeforeReveal) {
    prepareSettledReveal(entry, entry.contentRevision);
  }
  activateCard(entry, becameVisible);
}
function upsertCard(model) {
  const entry = ensureCard(model.id);
  applyGeometry(entry, model);
  applyStacking(entry, model.stack_order);
  entry.contentPhase = 'document';
  applyContentModel(entry, model, model.streaming ? 'stream_update' : 'finalize');
}
function streamCard(model) {
  const entry = cards.get(String(model.id));
  if (!entry) return;
  if (entry.contentPhase === 'finalized') {
    reportCardDiagnostic(model.id, entry, 'stale_stream_ignored', { payloadLen: model.body.length });
    return;
  }
  applyContentModel(entry, model, 'stream_update');
}
function finalizeCard(model) {
  const entry = cards.get(String(model.id));
  if (!entry) return;
  applyContentModel(entry, model, 'finalize');
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
  cancelActiveFit(entry);
  clearTimeout(entry.resizeFit);
  entry.directRuntime.destroy();
  entry.resizeRuntime.destroy();
  entry.processing.destroy();
  if (entry.commandPort) entry.commandPort.close();
  entry.card.remove();
  cards.delete(key);
}
function applyTheme(theme) {
  currentThemeCss = String(theme.css || '');
  document.getElementById('sgt-theme-css').textContent = currentThemeCss;
  for (const appearance of theme.cards || []) {
    const entry = cards.get(String(appearance.id));
    if (!entry) continue;
    entry.card.style.background = appearance.background;
    postCardMessage(entry, { type: 'theme_update', css: currentThemeCss });
  }
}
function reportNavigation(id, entry) {
  window.ipc.postMessage(JSON.stringify({
    type: 'navigation', id: Number(id), depth: entry.navigationDepth,
    max_depth: entry.navigationUrls.length
  }));
}
function navigateTo(entry, url) {
  window.ipc.postMessage(JSON.stringify({
    type: 'navigation_request', id: Number(entry.card.dataset.id), url: String(url)
  }));
}
window.addEventListener('message', function(event) {
  if (!event.data) return;
  const id = String(event.data.card_id || '');
  const entry = cards.get(id);
  if (!entry || entry.mode !== 'isolated') return;
  if (event.source !== entry.frame.contentWindow) return;
  if (Number(event.data.document_revision || 0) !== entry.revision) return;
    if (event.data.type === 'fit_diagnostic') {
      window.ipc.postMessage(JSON.stringify({ type: 'fit_diagnostic', id: Number(id), payload: event.data.payload }));
    } else if (event.data.type === 'frame_request') {
      const handle = Number(event.data.handle || 0);
      const documentRevision = entry.revision;
      requestAnimationFrame(function(timestamp) {
        if (entry.revision !== documentRevision) return;
        postCardMessage(entry, { type: 'frame_tick', handle: handle, timestamp: timestamp });
      });
    } else if (event.data.type === 'fit_request') {
      queueFit(entry, event.data.streaming);
    } else if (event.data.type === 'fit_complete') {
      completeFit(entry);
    } else if (event.data.type === 'card_diagnostic') {
      const revision = Number(event.data.content_revision || 0);
      if (event.data.phase === 'bridge_ready') {
        entry.commandPort = event.ports && event.ports[0] ? event.ports[0] : null;
        if (entry.commandPort) entry.commandPort.start();
        entry.activateIsolatedBridge();
      }
      if (String(event.data.phase || '').startsWith('font_ready_')) {
        entry.fontReady = true;
        const flushed = flushPendingContent(entry);
        if (entry.visible && entry.navigationDepth === 0 && !flushed) {
          queueFit(entry, entry.streaming);
        }
      }
      if (!revision || revision === entry.contentRevision) {
        const phase = event.data.phase || 'bridge_unknown';
        const details = {
          revision: revision || entry.contentRevision,
          payloadLen: event.data.payload_len,
          textLen: event.data.text_len,
          opacity: event.data.opacity,
          error: event.data.error
        };
        if (!deferIsolatedSettledPaint(entry, phase, details)) {
          reportCardDiagnostic(id, entry, phase, details);
        }
        if (phase === 'interactive_document_alive') {
          const surface = isolatedSurfaceVisibility(entry);
          reportCardDiagnostic(id, entry, surface.visible ? 'interactive_surface_visible'
            : 'interactive_surface_rejected', { error: surface.error });
        }
      }
    } else if (event.data.type === 'card_interaction') {
      raiseCard(entry);
      window.ipc.postMessage(JSON.stringify({ type: 'interaction', id: Number(id) }));
    } else if (event.data.type === 'copy_selection') {
      window.ipc.postMessage(JSON.stringify({
        action: 'copy_selection', hwnd: id, text: String(event.data.text || '')
      }));
    } else if (event.data.type === 'card_navigation') {
      navigateTo(entry, event.data.url);
    }
});
