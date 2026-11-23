package dev.screengoated.toolbox.mobile.service.preset

internal fun presetTextInputMobileShim(): String = """
    window.sgtText_activate({nonce:window.__sgtDocumentNonce,session:window.__sgtDocumentNonce});
    let editSequence = 0n;
    const ownsInput = owner => owner === window.__sgtTextSession && !window.__sgtInputClosing;
    const applyInput = (owner, action, text, revision) => {
        if (!ownsInput(owner)) return {status:'rejected'};
        const sequence = (++editSequence).toString();
        return window.sgtText_apply({nonce:owner,session:owner,id:sequence,sequence,action,
            text:text || '',revision:revision || '',passive:false,snapshot:'',focus:true});
    };
    window.setEditorText = (owner, text) => applyInput(owner, 'replace', text);
    window.insertTextAtCursor = (owner, text) => applyInput(owner, 'insert', text);
    window.clearInput = (owner, revision) => applyInput(owner, 'clear', '', revision);
    window.focusEditor = owner => applyInput(owner, 'focus');
    window.updateInputChrome = (owner, titleText, footerText, placeholderText) => {
        if (!ownsInput(owner)) return;
        const title = document.getElementById('headerTitle');
        title.removeAttribute('data-wrapped');
        title.textContent = titleText;
        document.getElementById('footerRegion').textContent = footerText;
        editor.placeholder = placeholderText;
    };
    window.closeWithAnimation = owner => {
        if (!ownsInput(owner)) return;
        window.__sgtInputClosing = true;
        window.playExit();
        window.setTimeout(() => postInput('input_exit_done'), 170);
    };
    const requestEditorFocus = () => {
        if (!window.__sgtInputClosing) postInput('request_focus');
    };
    editor.addEventListener('touchstart', requestEditorFocus, {passive:true});
    editor.addEventListener('mousedown', requestEditorFocus);
    const headerRegion = document.getElementById('headerRegion');
    let dragTouch = null;
    let dragWasActive = false;
    const TOUCH_DRAG_GAIN = Math.max(window.devicePixelRatio || 1, 1.85);
    headerRegion.addEventListener('touchstart', e => {
        if (e.touches.length !== 1) return;
        dragTouch = {x:e.touches[0].screenX,y:e.touches[0].screenY};
    }, {passive:true});
    headerRegion.addEventListener('touchmove', e => {
        if (!dragTouch || e.touches.length !== 1) return;
        const touch = e.touches[0];
        const dx = (touch.screenX - dragTouch.x) * TOUCH_DRAG_GAIN;
        const dy = (touch.screenY - dragTouch.y) * TOUCH_DRAG_GAIN;
        if (Math.abs(dx) >= 1 || Math.abs(dy) >= 1) {
            dragWasActive = true;
            postInput(JSON.stringify({type:'dragInputWindow',dx,dy}));
            postInput('dragAt:' + Math.round(touch.screenX) + ',' + Math.round(touch.screenY));
        }
        dragTouch = {x:touch.screenX,y:touch.screenY};
        e.preventDefault();
    }, {passive:false});
    const finishDragTouch = event => {
        if (dragWasActive) {
            const point = event.changedTouches && event.changedTouches[0];
            const position = point ? {x:point.screenX,y:point.screenY} : dragTouch;
            if (position) postInput('dragEnd:' + Math.round(position.x) + ',' + Math.round(position.y));
        }
        dragWasActive = false;
        dragTouch = null;
    };
    headerRegion.addEventListener('touchend', finishDragTouch);
    headerRegion.addEventListener('touchcancel', finishDragTouch);
""".trimIndent()
