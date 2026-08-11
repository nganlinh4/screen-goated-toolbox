// Drag support for standalone cards and cards inside the shared compositor.
let isCardDragging = false;
let cardDragStartX = 0;
let cardDragStartY = 0;

function beginCardDrag(e) {
    if (!window.REALTIME_COMPOSITOR_ROLE) {
        window.realtimePostMessage('startDrag');
        return;
    }
    e.preventDefault();
    isCardDragging = true;
    cardDragStartX = e.screenX;
    cardDragStartY = e.screenY;
    window.realtimePostMessage('interactionStart');
    document.addEventListener('mousemove', onCardDragMove);
    document.addEventListener('mouseup', onCardDragEnd);
}

function onCardDragMove(e) {
    if (!isCardDragging) return;
    const dx = e.screenX - cardDragStartX;
    const dy = e.screenY - cardDragStartY;
    if (dx !== 0 || dy !== 0) {
        window.realtimePostMessage('cardDragMove:' + dx + ',' + dy);
        cardDragStartX = e.screenX;
        cardDragStartY = e.screenY;
    }
}

function onCardDragEnd() {
    if (!isCardDragging) return;
    isCardDragging = false;
    document.removeEventListener('mousemove', onCardDragMove);
    document.removeEventListener('mouseup', onCardDragEnd);
    window.realtimePostMessage('interactionEnd');
}

container.addEventListener('mousedown', function(e) {
    if (e.button !== 0) return;
    if (e.target.closest('#controls') || e.target.closest('#header-toggle') ||
        e.target.id === 'resize-hint' || isResizing) return;
    beginCardDrag(e);
});

// Right-click moves both cards together, preserving the existing gesture.
let isGroupDragging = false;
let groupDragStartX = 0;
let groupDragStartY = 0;

container.addEventListener('mousedown', function(e) {
    if (e.button !== 2) return;
    if (e.target.closest('#controls') || e.target.closest('select')) return;
    e.preventDefault();
    isGroupDragging = true;
    groupDragStartX = e.screenX;
    groupDragStartY = e.screenY;
    window.realtimePostMessage(window.REALTIME_COMPOSITOR_ROLE ? 'interactionStart' : 'startGroupDrag');
    document.addEventListener('mousemove', onGroupDragMove);
    document.addEventListener('mouseup', onGroupDragEnd);
});

container.addEventListener('contextmenu', function(e) {
    if (e.target.closest('#controls') || e.target.closest('select')) return;
    e.preventDefault();
});

function onGroupDragMove(e) {
    if (!isGroupDragging) return;
    const dx = e.screenX - groupDragStartX;
    const dy = e.screenY - groupDragStartY;
    if (dx !== 0 || dy !== 0) {
        window.realtimePostMessage('groupDragMove:' + dx + ',' + dy);
        groupDragStartX = e.screenX;
        groupDragStartY = e.screenY;
    }
}

function onGroupDragEnd() {
    if (!isGroupDragging) return;
    isGroupDragging = false;
    document.removeEventListener('mousemove', onGroupDragMove);
    document.removeEventListener('mouseup', onGroupDragEnd);
    if (window.REALTIME_COMPOSITOR_ROLE) window.realtimePostMessage('interactionEnd');
}

resizeHint.addEventListener('mousedown', function(e) {
    e.stopPropagation();
    e.preventDefault();
    isResizing = true;
    resizeStartX = e.screenX;
    resizeStartY = e.screenY;
    if (window.REALTIME_COMPOSITOR_ROLE) window.realtimePostMessage('interactionStart');
    document.addEventListener('mousemove', onResizeMove);
    document.addEventListener('mouseup', onResizeEnd);
});

function onResizeMove(e) {
    if (!isResizing) return;
    const dx = e.screenX - resizeStartX;
    const dy = e.screenY - resizeStartY;
    if (Math.abs(dx) > 5 || Math.abs(dy) > 5) {
        window.realtimePostMessage('resize:' + dx + ',' + dy);
        resizeStartX = e.screenX;
        resizeStartY = e.screenY;
    }
}

function onResizeEnd() {
    isResizing = false;
    document.removeEventListener('mousemove', onResizeMove);
    document.removeEventListener('mouseup', onResizeEnd);
    window.realtimePostMessage('saveResize');
    if (window.REALTIME_COMPOSITOR_ROLE) window.realtimePostMessage('interactionEnd');
}
