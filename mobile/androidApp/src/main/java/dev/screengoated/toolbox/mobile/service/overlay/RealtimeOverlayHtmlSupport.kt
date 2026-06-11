package dev.screengoated.toolbox.mobile.service.overlay

internal fun overlayFontCss(): String {
    return """
        @font-face {
            font-family: 'Google Sans Flex';
            font-style: normal;
            font-weight: 100 1000;
            font-stretch: 25% 1000%;
            font-display: swap;
            src: url('../GoogleSansFlex.ttf') format('truetype');
        }
    """.trimIndent()
}

internal fun overlayBridgePrelude(): String {
    return """
        window.ipc = {
            postMessage(message) {
                if (window.sgtAndroid && window.sgtAndroid.postMessage) {
                    window.sgtAndroid.postMessage(String(message));
                }
            }
        };
    """.trimIndent()
}

internal fun overlayMobileShim(): String {
    return """
        (() => {
            const container = document.getElementById('container');
            const ttsModal = document.getElementById('tts-modal');
            const ttsModalOverlay = document.getElementById('tts-modal-overlay');
            const speakBtn = document.getElementById('speak-btn');
            const ttsToggle = document.getElementById('tts-toggle');
            const speedSlider = document.getElementById('speed-slider');
            const speedValue = document.getElementById('speed-value');
            const autoToggle = document.getElementById('auto-speed-toggle');
            const volumeSlider = document.getElementById('volume-slider');
            const volumeValue = document.getElementById('volume-value');
            const blockInteractive = target =>
                !!(target.closest('#controls') || target.closest('#tts-modal') || target.closest('.language-btn'));
            let dragTouch = null;
            let resizeTouch = null; // {x, y, corner: 'bl'|'br'}

            const RESIZE_ZONE_PX = 44; // corner touch zone size
            const TOUCH_DRAG_GAIN = Math.max(window.devicePixelRatio || 1, 1.85);

            function detectCorner(touchX, touchY) {
                if (!container) return null;
                const rect = container.getBoundingClientRect();
                const localX = touchX - rect.left;
                const localY = touchY - rect.top;
                const inBottom = localY > rect.height - RESIZE_ZONE_PX;
                if (!inBottom) return null;
                if (localX < RESIZE_ZONE_PX) return 'bl';
                if (localX > rect.width - RESIZE_ZONE_PX) return 'br';
                return null;
            }

            function attachMoveResize(surface, blockFn) {
                surface.addEventListener('touchstart', event => {
                    if (event.touches.length !== 1 || blockFn(event.target)) return;
                    const touch = event.touches[0];
                    const corner = detectCorner(touch.clientX, touch.clientY);
                    if (corner) {
                        resizeTouch = { x: touch.screenX, y: touch.screenY, corner: corner };
                        dragTouch = null;
                    } else {
                        dragTouch = { x: touch.screenX, y: touch.screenY };
                        resizeTouch = null;
                    }
                }, { passive: true });

                surface.addEventListener('touchmove', event => {
                    if (event.touches.length !== 1 || blockFn(event.target)) return;
                    const touch = event.touches[0];

                    if (resizeTouch) {
                        const dx = Math.round((touch.screenX - resizeTouch.x) * TOUCH_DRAG_GAIN);
                        const dy = Math.round((touch.screenY - resizeTouch.y) * TOUCH_DRAG_GAIN);
                        if (dx !== 0 || dy !== 0) {
                            window.ipc.postMessage('resizeCorner:' + resizeTouch.corner + ',' + dx + ',' + dy);
                            resizeTouch.x = touch.screenX;
                            resizeTouch.y = touch.screenY;
                            if (event.cancelable) event.preventDefault();
                        }
                        return;
                    }

                    if (!dragTouch) return;
                    const dx = Math.round((touch.screenX - dragTouch.x) * TOUCH_DRAG_GAIN);
                    const dy = Math.round((touch.screenY - dragTouch.y) * TOUCH_DRAG_GAIN);
                    if (dx !== 0 || dy !== 0) {
                        window.ipc.postMessage('dragWindow:' + dx + ',' + dy);
                        window.ipc.postMessage('dragAt:' + Math.round(touch.screenX) + ',' + Math.round(touch.screenY));
                        dragTouch = { x: touch.screenX, y: touch.screenY };
                        if (event.cancelable) event.preventDefault();
                    }
                }, { passive: false });

                surface.addEventListener('touchend', event => {
                    if (dragTouch && event.changedTouches.length > 0) {
                        const touch = event.changedTouches[0];
                        window.ipc.postMessage('dragEnd:' + Math.round(touch.screenX) + ',' + Math.round(touch.screenY));
                    }
                    dragTouch = null;
                    resizeTouch = null;
                }, { passive: true });
                surface.addEventListener('touchcancel', () => { dragTouch = null; resizeTouch = null; }, { passive: true });
            }

            if (container) {
                attachMoveResize(container, blockInteractive);
            }

            // Modals cover the whole window when it is small — keep drag/resize working on
            // their background (interactive controls inside the modal stay untouched)
            const blockModalInteractive = target =>
                !!target.closest('input, button, select, .toggle-switch, .auto-toggle, .ctrl-btn');
            ['tts-modal', 'download-modal'].forEach(id => {
                const modal = document.getElementById(id);
                if (modal) attachMoveResize(modal, blockModalInteractive);
            });

            window.setTtsState = function(enabled, speed, autoSpeed, volume) {
                const s2sMode = document.body && document.body.dataset.s2s === '1';
                const liveTranslateMode = document.body && document.body.dataset.liveTranslate === '1';
                if (ttsToggle) {
                    ttsToggle.classList.toggle('on', s2sMode || !!enabled);
                    ttsToggle.classList.toggle('locked', s2sMode);
                    ttsToggle.hidden = s2sMode;
                }
                if (speakBtn) {
                    speakBtn.classList.toggle('active', s2sMode || !!enabled);
                    speakBtn.classList.toggle('locked', s2sMode);
                }
                if (speedSlider) speedSlider.value = speed;
                if (speedValue) speedValue.textContent = (speed / 100).toFixed(1) + 'x';
                if (autoToggle) autoToggle.classList.toggle('on', !!autoSpeed);
                const speedRow = document.querySelector('.tts-speed-row');
                if (speedRow) speedRow.hidden = liveTranslateMode;
                if (volumeSlider) volumeSlider.value = volume;
                if (volumeValue) volumeValue.textContent = volume + '%';
            };

            window.closeTtsModal = function() {
                if (ttsModal) ttsModal.classList.remove('show');
                if (ttsModalOverlay) ttsModalOverlay.classList.remove('show');
            };
        })();
    """.trimIndent()
}
