window.applyHostCommand = function(command) {
  if (command.type === 'source_image') window.__SGT_SOURCE_IMAGE__.render(command.request);
  else if (command.type === 'source_image_status') window.__SGT_SOURCE_IMAGE__.status(command.id, command.success);
  else if (command.type === 'processing') window.__SGT_PROCESSING_CONTOURS__.apply(command.effects);
  else if (command.type === 'snapshot') {
    const incoming = new Set(command.cards.map(card => String(card.id)));
    for (const key of cards.keys()) if (!incoming.has(key)) removeCard(key);
    for (const card of command.cards) upsertCard(card);
  } else if (command.type === 'upsert') upsertCard(command.card);
  else if (command.type === 'upsert_batch') {
    for (const card of command.cards) upsertCard(card);
  }
  else if (command.type === 'stream') streamCard(command.card);
  else if (command.type === 'finalize') finalizeCard(command.card);
  else if (command.type === 'geometry') {
    for (const card of command.cards) updateGeometry(card);
  } else if (command.type === 'drag_settled') {
    for (const card of command.cards) updateGeometry(card);
  } else if (command.type === 'opacity') {
    const entry = cards.get(String(command.id));
    if (entry) {
      // Prewarmed source cards stay mounted, but opacity must not reveal them.
      entry.card.style.opacity = entry.visible
        ? String(Math.max(0, Math.min(100, command.opacity)) / 100) : '0';
      syncSourceBackdrop(entry);
    }
  } else if (command.type === 'theme') applyTheme(command.theme);
  else if (command.type === 'locale') window.__SGT_SET_CONTROL_LOCALE__(command.strings);
  else if (command.type === 'raise') {
    const entry = cards.get(String(command.id));
    if (entry) applyStacking(entry, command.stack_order);
  } else if (command.type === 'remove') removeCard(command.id);
};
