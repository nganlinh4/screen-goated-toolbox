window.__SGT_APPLY_EXTERNAL_SURFACE__ = function(entry, model, nextDocument, becameVisible, activateCard) {
  if (entry.navigationLoading) {
    entry.card.dataset.surface = 'navigation-loading';
    entry.pendingContent = null;
    entry.mode = 'navigation-loading';
    entry.directHost.hidden = true;
    entry.frame.hidden = true;
    return true;
  }
  if (model.external_navigation === true) {
    entry.card.dataset.surface = 'native';
    entry.pendingContent = null;
    entry.document = nextDocument;
    entry.mode = 'native';
    entry.directHost.hidden = true;
    entry.frame.hidden = true;
    activateCard(entry, becameVisible);
    return true;
  }
  entry.card.dataset.surface = 'result';
  return false;
};

function documentKey(documentHtml) {
  return documentHtml === null ? 'shared' : 'inline:' + documentHtml;
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

let sourceMeasureContext = null;
function measureSourceText(text) {
  if (!sourceMeasureContext) sourceMeasureContext = document.createElement('canvas').getContext('2d');
  if (!sourceMeasureContext || !text) return 0.1;
  sourceMeasureContext.font = "400 100px 'Google Sans Flex'";
  return Math.max(0.1, sourceMeasureContext.measureText(text).width);
}

window.__SGT_QUEUE_SOURCE_LAYOUT__ = (function() {
  let pending = [];
  let queued = false;

  function flush() {
    queued = false;
    const batch = pending;
    pending = [];
    const measurements = batch.map(function(task) {
      try { return task.measure(); } catch (_) { return null; }
    });
    batch.forEach(function(task, index) {
      try { task.commit(measurements[index]); } finally { task.resolve(); }
    });
  }

  return function(measure, commit) {
    return new Promise(function(resolve) {
      pending.push({ measure: measure, commit: commit, resolve: resolve });
      if (queued) return;
      queued = true;
      if (typeof queueMicrotask === 'function') queueMicrotask(flush);
      else Promise.resolve().then(flush);
    });
  };
})();

function sourceTypography(text, width, height, vertical) {
  const advance = measureSourceText(text);
  const along = vertical ? height : width;
  const cross = vertical ? width : height;
  const minimumAdvanceRatio = 0.75;
  const fontSize = Math.max(0.1, vertical
    ? Math.min(cross * 2, along * 100 / advance)
    : Math.min(cross / 1.08, along * 100 / (advance * minimumAdvanceRatio)));
  const uncondensedAlong = advance * fontSize / 100;
  const availableRatio = along / Math.max(0.1, uncondensedAlong);
  const stretch = vertical ? cross * 100 / fontSize
    : availableRatio <= minimumAdvanceRatio ? 50
    : availableRatio <= 1 ? 50 + (availableRatio - minimumAdvanceRatio) * 200
    : 100 + (availableRatio - 1) * 250;
  return { fontSize: fontSize, stretch: Math.max(50, Math.min(150, stretch)) };
}

function setSourceReplacementSurface(entry, enabled) {
  const surface = entry.visualSurface;
  if (enabled && !surface.isConnected) {
    surface.appendChild(entry.backdrop);
    surface.appendChild(entry.directHost);
    surface.appendChild(entry.frame);
    entry.card.insertBefore(surface, entry.processing.element);
  } else if (!enabled && surface.isConnected) {
    entry.card.insertBefore(entry.backdrop, surface);
    entry.card.insertBefore(entry.directHost, surface);
    entry.card.insertBefore(entry.frame, surface);
    surface.remove();
  }
}
