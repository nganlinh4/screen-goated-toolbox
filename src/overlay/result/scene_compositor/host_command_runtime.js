window.applyHostCommand = function(command) {
  if (command.type === 'snapshot') {
    const incoming = new Set(command.cards.map(card => String(card.id)));
    for (const key of cards.keys()) if (!incoming.has(key)) removeCard(key);
    for (const card of command.cards) upsertCard(card);
  } else if (command.type === 'upsert') upsertCard(command.card);
  else if (command.type === 'stream') streamCard(command.card);
  else if (command.type === 'finalize') finalizeCard(command.card);
  else if (command.type === 'geometry') {
    for (const card of command.cards) updateGeometry(card);
  } else if (command.type === 'drag_settled') {
    const preservePreview = window.__SGT_BUTTON_SCENE__?.hasReleasedDragPreview?.() === true;
    if (!preservePreview) for (const card of command.cards) updateGeometry(card);
  } else if (command.type === 'theme') applyTheme(command.theme);
  else if (command.type === 'raise') {
    const entry = cards.get(String(command.id));
    if (entry) applyStacking(entry, command.stack_order);
  } else if (command.type === 'remove') removeCard(command.id);
  else if (command.type === 'navigate_back') {
    const entry = cards.get(String(command.id));
    if (entry && entry.navigationDepth > 0) {
      entry.navigationDepth--;
      if (entry.navigationDepth === 0) {
        if (entry.document === null) {
          useDirectSurface(entry);
          activateCard(entry, true);
        } else {
          entry.pendingContent = null;
          loadIsolatedDocument(entry, entry.document);
          queueCardContent(entry, { type: 'finalize', html: entry.body, refining: entry.refining });
        }
      } else {
        entry.ready = false;
        entry.frame.src = entry.navigationUrls[entry.navigationDepth - 1];
      }
      reportNavigation(command.id, entry);
    }
  } else if (command.type === 'navigate_forward') {
    const entry = cards.get(String(command.id));
    if (entry && entry.navigationDepth < entry.navigationUrls.length) {
      navigateTo(entry, entry.navigationUrls[entry.navigationDepth]);
    }
  }
};
