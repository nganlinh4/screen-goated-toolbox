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
