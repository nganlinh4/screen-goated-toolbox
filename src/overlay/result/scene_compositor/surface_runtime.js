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
  const pending = [];
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
    const ready = tasks.flatMap(function(task) { return task.items; })
      .filter(function(item) { return item.text.isConnected && item.box.isConnected; });
    const items = ready.filter(function(item) {
      return !item.footprint || !item.footprint.length || !window.__SGT_FIT_FOOTPRINT__(item);
    });
    for (const item of items) {
      // Advance bounds can exclude a glyph's horizontal ink overhang.
      item.width = Math.max(0, item.box.clientWidth - 2);
      item.height = Math.max(0, item.box.clientHeight);
      item.ceiling = Math.max(0.1, (item.vertical ? item.width : item.height) / 1.08);
      item.fontSize = item.wrap ? Math.max(1, Number(item.preferredFontSize) || 14)
        : item.ceiling;
      item.stretch = 100;
    }
    for (const item of items) {
      item.text.style.transform = '';
      item.text.setAttribute('dir', 'auto');
      item.text.style.whiteSpace = 'nowrap';
      item.text.style.width = 'auto';
      item.text.style.height = 'auto';
      item.text.style.maxHeight = 'none';
      applyTypography(item, item.fontSize, 100);
    }
    // One unwrapped shape supplies both a single-line candidate and a wrapped
    // area estimate. Use browser shaping, including fallback and bidi runs.
    for (const item of items) {
      const extent = shapedExtent(item);
      const crossScale = Math.min(1, item.height / Math.max(0.1, extent.height));
      const widthAxis = !item.vertical && extent.width * crossScale > item.width ? 87 : 100;
      item.singleStretch = widthAxis;
      item.singleSize = item.fontSize * Math.min(crossScale,
        item.width / Math.max(0.1, extent.width * widthAxis / 100));
      const along = item.vertical ? extent.height : extent.width;
      const area = Math.max(0.1, along * item.fontSize * 1.08);
      const wrappedSize = Math.min(item.ceiling,
        item.fontSize * Math.sqrt(item.width * item.height / area) * 0.95);
      item.wrapping = item.wrap || wrappedSize > item.singleSize * 1.08;
      item.fontSize = item.wrapping ? wrappedSize : item.singleSize;
      item.stretch = item.wrapping ? 100 : widthAxis;
    }
    function applyLayout(item) {
      item.text.style.whiteSpace = item.wrapping ? 'normal' : 'nowrap';
      item.text.style.overflowWrap = 'anywhere';
      item.text.style.textAlign = item.wrap ? 'start' : 'center';
      item.text.style.width = item.wrapping && !item.vertical ? '100%' : 'auto';
      item.text.style.height = item.wrapping && item.vertical ? '100%' : 'auto';
      applyTypography(item, item.fontSize, item.stretch);
    }
    for (const item of items) applyLayout(item);
    // Keep the conservative size as a fallback. A single area-based probe
    // avoids the abrupt full-height ratio reduction at a line-wrap boundary.
    for (const item of items) {
      const extent = shapedExtent(item);
      const ratio = Math.min(1, item.width / Math.max(0.1, extent.width),
        item.height / Math.max(0.1, extent.height));
      item.safeSize = item.fontSize * ratio;
      if (item.wrapping) {
        item.fontSize = Math.min(item.ceiling, item.fontSize
          * (ratio < 1 ? Math.sqrt(ratio) : 1.12));
      } else {
        item.fontSize = item.safeSize;
      }
    }
    for (const item of items) applyLayout(item);
    for (const item of items) {
      const extent = shapedExtent(item);
      if (extent.width > item.width || extent.height > item.height) {
        item.fontSize = item.safeSize;
      }
      if (!item.wrap && item.singleSize > item.fontSize * 1.03) {
        item.wrapping = false;
        item.fontSize = item.singleSize;
        item.stretch = item.singleStretch;
      }
    }
    for (const item of items) applyLayout(item);
    const widthCandidates = items.filter(function(item) {
      const extent = shapedExtent(item);
      item.widthBest = { size: item.fontSize, stretch: item.stretch, wrap: item.wrapping,
        effective: item.fontSize * Math.min(1, item.width / Math.max(0.1, extent.width),
          item.height / Math.max(0.1, extent.height)) };
      return !item.vertical && (item.wrapping || extent.height < item.height * 0.94);
    });
    // Shape actual condensed glyphs at full line height. Batch writes and reads;
    // do not estimate the variable font's width response as a linear scale.
    for (const stretch of [85, 70, 60]) {
      for (const item of widthCandidates) {
        item.wrapping = false; item.fontSize = item.ceiling; item.stretch = stretch;
        applyLayout(item);
      }
      for (const item of widthCandidates) {
        const extent = shapedExtent(item);
        const size = item.fontSize * Math.min(1, item.width / Math.max(0.1, extent.width),
          item.height / Math.max(0.1, extent.height));
        if (size >= item.widthBest.effective * 1.06) {
          item.widthBest = { size, stretch, wrap: false, effective: size };
        }
      }
    }
    for (const item of widthCandidates) {
      item.fontSize = item.widthBest.size; item.stretch = item.widthBest.stretch;
      item.wrapping = item.widthBest.wrap; applyLayout(item);
    }
    for (const item of items) {
      const extent = shapedExtent(item);
      item.visualScale = Math.min(1, item.width / Math.max(0.1, extent.width),
        item.height / Math.max(0.1, extent.height));
      const box = item.box.getBoundingClientRect();
      const text = item.text.getBoundingClientRect();
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
    // Finish each ready task independently. Dense captures yield between small
    // batches rather than blocking one frame on every region's font shaping.
    const started = performance.now();
    do {
      let budget = 16;
      const batch = [];
      const completed = [];
      while (pending.length && budget > 0) {
        const task = pending[0];
        const shaped = task.items[task.offset]?.footprint?.length > 1;
        const items = task.items.slice(task.offset, task.offset + (shaped ? 1 : budget));
        task.offset += items.length;
        budget -= items.length;
        batch.push({ items });
        if (task.offset >= task.items.length) completed.push(pending.shift());
        if (shaped) break;
      }
      try { fitBatch(batch); }
      finally { for (const task of completed) task.resolve(); }
    } while (pending.length && performance.now() - started < 4);
    if (pending.length) schedule();
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(flush);
  }

  function queue(items) {
    return new Promise(function(resolve) {
      pending.push({ items: items, offset: 0, resolve: resolve });
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
