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
    return { width: rect.width, height: rect.height };
  }

  function itemFits(item) {
    const extent = shapedExtent(item);
    return extent.width <= item.box.clientWidth + 0.5
      && extent.height <= item.box.clientHeight + 0.5;
  }

  function forceLayout() {
    void document.documentElement.offsetHeight;
  }

  function fitBatch(tasks) {
    const items = tasks.flatMap(function(task) { return task.items; });
    for (const item of items) {
      item.minorExtent = item.vertical ? item.box.clientWidth : item.box.clientHeight;
      item.fontLow = 0.1;
      item.fontHigh = Math.max(1, item.minorExtent * 2);
      item.fontSize = 0.1;
    }
    for (let fontAttempt = 0; fontAttempt < 12; fontAttempt++) {
      for (const item of items) {
        item.fontMiddle = (item.fontLow + item.fontHigh) / 2;
        applyTypography(item, item.fontMiddle, 50);
      }
      forceLayout();
      for (const item of items) {
        if (itemFits(item)) {
          item.fontSize = item.fontMiddle;
          item.fontLow = item.fontMiddle;
        } else {
          item.fontHigh = item.fontMiddle;
        }
      }
    }
    for (const item of items) {
      item.widthLow = 50;
      item.widthHigh = 151;
      item.chosenWidth = 50;
    }
    for (let widthAttempt = 0; widthAttempt < 12; widthAttempt++) {
      for (const item of items) {
        item.widthMiddle = (item.widthLow + item.widthHigh) / 2;
        applyTypography(item, item.fontSize, item.widthMiddle);
      }
      forceLayout();
      for (const item of items) {
        if (itemFits(item)) {
          item.chosenWidth = item.widthMiddle;
          item.widthLow = item.widthMiddle;
        } else {
          item.widthHigh = item.widthMiddle;
        }
      }
    }
    for (const item of items) applyTypography(item, item.fontSize, item.chosenWidth);
    forceLayout();
    for (const item of items) {
      const extent = shapedExtent(item);
      const visualScale = Math.min(
        1,
        item.box.clientWidth / Math.max(1, extent.width),
        item.box.clientHeight / Math.max(1, extent.height)
      );
      if (visualScale < 1) {
        item.text.style.transform = 'scale(' + visualScale + ')';
        item.text.style.transformOrigin = 'center center';
      } else {
        item.text.style.transform = '';
      }
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

  return function(items) {
    return new Promise(function(resolve) {
      pending.push({ items: items, resolve: resolve });
      schedule();
    });
  };
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
