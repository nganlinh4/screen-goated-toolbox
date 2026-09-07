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

window.__SGT_QUEUE_SOURCE_FIT__ = (function() {
  let pending = [];
  let scheduled = false;

  function applyTypography(item, fontSize, stretch) {
    item.text.style.fontWeight = '400';
    item.text.style.fontOpticalSizing = 'none';
    item.text.style.fontSize = fontSize + 'px';
    item.text.style.lineHeight = '1.08';
    item.text.style.fontStretch = stretch + '%';
    item.text.style.fontVariationSettings = "'slnt' 0, 'ROND' 100";
  }

  function shapedExtent(item) {
    if (!item.text.textContent) return { width: 0, height: 0 };
    const range = document.createRange();
    range.selectNodeContents(item.text);
    const rect = range.getBoundingClientRect();
    return rect;
  }

  function fitBatch(tasks) {
    const items = tasks.flatMap(function(task) { return task.items; })
      .filter(function(item) { return item.text.isConnected && item.box.isConnected; });
    // Read every box before writing typography. Only two width-axis instances
    // are needed; continuous axis searches are particularly expensive cold.
    for (const item of items) {
      item.width = Math.max(0, item.box.clientWidth);
      item.height = Math.max(0, item.box.clientHeight);
      item.fontSize = Math.max(0.1, (item.vertical ? item.width : item.height) / 1.08);
      item.stretch = 100;
    }
    for (const item of items) {
      item.text.style.transform = '';
      applyTypography(item, item.fontSize, item.stretch);
    }
    // First shape: preserve normal width when possible, mildly condense only
    // overflowing horizontal lines, and estimate the contained size directly.
    for (const item of items) {
      const extent = shapedExtent(item);
      const heightScale = Math.min(1, item.height / Math.max(0.1, extent.height));
      if (!item.vertical && extent.width * heightScale > item.width) item.stretch = 87;
      item.fontSize *= Math.min(heightScale,
        item.width / Math.max(0.1, extent.width * item.stretch / 100));
    }
    for (const item of items) applyTypography(item, item.fontSize, item.stretch);
    // Optical sizing and shaping need not scale linearly. One measured
    // correction, then a paint-only containment transform, never a search loop.
    for (const item of items) {
      const extent = shapedExtent(item);
      item.fontSize *= Math.min(1, item.width / Math.max(0.1, extent.width),
        item.height / Math.max(0.1, extent.height));
    }
    for (const item of items) applyTypography(item, item.fontSize, item.stretch);
    for (const item of items) {
      const extent = shapedExtent(item);
      item.visualScale = Math.min(1, item.width / Math.max(0.1, extent.width),
        item.height / Math.max(0.1, extent.height));
      const box = item.box.getBoundingClientRect();
      const text = item.text.getBoundingClientRect();
      // Center the shaped glyph bounds, not the CSS line box: ascent/descent,
      // RTL overhangs and max-width constraints can offset the ink inside it.
      item.shiftX = box.left + box.width / 2 - text.left
        - ((extent.left || 0) + extent.width / 2 - text.left) * item.visualScale;
      item.shiftY = box.top + box.height / 2 - text.top
        - ((extent.top || 0) + extent.height / 2 - text.top) * item.visualScale;
    }
    for (const item of items) {
      item.text.style.transform = 'translate(' + item.shiftX + 'px,' + item.shiftY
        + 'px) scale(' + item.visualScale + ')';
      item.text.style.transformOrigin = '0 0';
      item.box.style.overflow = 'hidden';
      item.text.style.overflow = 'visible';
    }
  }

  function flush() {
    scheduled = false;
    const tasks = pending;
    pending = [];
    try { fitBatch(tasks); }
    finally { for (const task of tasks) task.resolve(); }
    if (pending.length) schedule();
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(flush);
  }

  function queue(items) {
    return new Promise(function(resolve) {
      pending.push({ items: items, resolve: resolve });
      schedule();
    });
  }

  queue.warmup = function() {
    const sample = document.querySelector('.font-prewarm');
    if (!sample) return;
    const probe = sample.cloneNode(true);
    probe.removeAttribute('class');
    probe.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap;pointer-events:none;';
    probe.style.fontFamily = getComputedStyle(sample).fontFamily;
    document.body.appendChild(probe);
    try {
      const item = { text: probe };
      for (const stretch of [100, 87]) {
        applyTypography(item, 16, stretch);
        shapedExtent(item);
      }
    } finally { probe.remove(); }
  };
  return queue;
})();

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
