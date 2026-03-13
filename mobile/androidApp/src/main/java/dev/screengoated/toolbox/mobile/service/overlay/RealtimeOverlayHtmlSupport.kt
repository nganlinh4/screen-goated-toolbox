package dev.screengoated.toolbox.mobile.service.overlay

internal fun overlayBaseHtmlTemplate(): String {
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <style>{{FONT_CSS}}</style>
            <style id="main-style">
                {{CSS_CONTENT}}
            </style>
        </head>
        <body>
            <div id="loading-overlay">{{LOADING_ICON}}</div>
            <div id="container">
                <div id="header">
                    <div id="title">{{TITLE_CONTENT}}</div>
                    <div id="controls">
                        {{AUDIO_SELECTOR}}
                        <span class="ctrl-btn" id="copy-btn" title="{{COPY_TEXT_TITLE}}"><span class="material-symbols-rounded">{{CONTENT_COPY_SVG}}</span></span>
                        <div class="pill-group">
                            <span class="ctrl-btn" id="font-decrease" title="{{DECREASE_FONT_TITLE}}"><span class="material-symbols-rounded">{{REMOVE_SVG}}</span></span>
                            <span class="ctrl-btn" id="font-increase" title="{{INCREASE_FONT_TITLE}}"><span class="material-symbols-rounded">{{ADD_SVG}}</span></span>
                        </div>
                        <div class="btn-group">
                            <span class="vis-btn mic active" id="toggle-mic" title="{{TOGGLE_TRANSCRIPTION_TITLE}}"><span class="material-symbols-rounded">{{SUBTITLES_SVG}}</span></span>
                            <span class="vis-btn trans active" id="toggle-trans" title="{{TOGGLE_TRANSLATION_TITLE}}"><span class="material-symbols-rounded">{{TRANSLATE_SVG}}</span></span>
                        </div>
                    </div>
                </div>
                <div id="header-toggle" title="{{TOGGLE_HEADER_TITLE}}"><span class="material-symbols-rounded">{{EXPAND_LESS_SVG}}</span></div>
                <div id="viewport">
                    <div id="content">
                        <span class="placeholder">{{PLACEHOLDER_TEXT}}</span>
                    </div>
                </div>
            </div>
            <div id="download-modal-overlay"></div>
            <div id="download-modal">
                <div class="download-modal-title">
                    <span class="material-symbols-rounded">{{DOWNLOAD_SVG}}</span>
                    <span id="download-title">{{DOWNLOAD_TITLE}}</span>
                </div>
                <div class="download-modal-msg" id="download-msg">{{PLEASE_WAIT_TEXT}}</div>
                <div class="download-progress-bar">
                    <div class="download-progress-fill" id="download-fill" style="width: 0%;"></div>
                </div>
                <div class="download-modal-footnote">{{SUPPORTS_ENGLISH}}</div>
                <button class="download-cancel-btn" id="download-cancel-btn" title="{{CANCEL_DOWNLOAD_TITLE}}">
                    <span class="material-symbols-rounded">{{CLOSE_SVG}}</span>
                    {{CANCEL_TEXT}}
                </button>
            </div>
            <div id="tts-modal-overlay"></div>
            <div id="tts-modal">
                <div class="tts-modal-title">
                    <span class="material-symbols-rounded">{{VOLUME_UP_SVG}}</span>
                    {{TTS_TITLE}}
                    <div class="toggle-switch" id="tts-toggle" style="margin-left: auto;"></div>
                </div>
                <div class="tts-modal-row">
                    <span class="tts-modal-label">{{TTS_SPEED}}</span>
                    <div class="speed-slider-container">
                        <input type="range" class="speed-slider" id="speed-slider" min="50" max="200" value="100" step="10">
                        <span class="speed-value" id="speed-value">1.0x</span>
                        <button class="auto-toggle on" id="auto-speed-toggle" title="Auto-adjust speed to catch up">{{TTS_AUTO}}</button>
                    </div>
                </div>
                <div class="tts-modal-row">
                    <span class="tts-modal-label">{{TTS_VOLUME}}</span>
                    <div class="speed-slider-container">
                        <input type="range" class="speed-slider" id="volume-slider" min="0" max="100" value="100" step="5">
                        <span class="speed-value" id="volume-value">100%</span>
                    </div>
                </div>
            </div>
            <script>
                {{JS_CONTENT}}
            </script>
        </body>
        </html>
    """.trimIndent()
}

internal fun overlayFontCss(): String {
    return """
        @font-face {
            font-family: 'Google Sans Flex';
            font-style: normal;
            font-weight: 100 1000;
            font-stretch: 25% 1000%;
            font-display: swap;
            src: url('GoogleSansFlex.ttf') format('truetype');
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

            if (container) {
                container.addEventListener('touchstart', event => {
                    if (event.touches.length !== 1 || blockInteractive(event.target)) return;
                    const touch = event.touches[0];
                    dragTouch = { x: touch.screenX, y: touch.screenY };
                }, { passive: true });

                container.addEventListener('touchmove', event => {
                    if (!dragTouch || event.touches.length !== 1 || blockInteractive(event.target)) return;
                    const touch = event.touches[0];
                    const dx = Math.round(touch.screenX - dragTouch.x);
                    const dy = Math.round(touch.screenY - dragTouch.y);
                    if (dx !== 0 || dy !== 0) {
                        window.ipc.postMessage('dragWindow:' + dx + ',' + dy);
                        dragTouch = { x: touch.screenX, y: touch.screenY };
                        if (event.cancelable) event.preventDefault();
                    }
                }, { passive: false });

                container.addEventListener('touchend', () => { dragTouch = null; }, { passive: true });
                container.addEventListener('touchcancel', () => { dragTouch = null; }, { passive: true });
            }

            window.setTtsState = function(enabled, speed, autoSpeed, volume) {
                if (ttsToggle) ttsToggle.classList.toggle('on', !!enabled);
                if (speakBtn) speakBtn.classList.toggle('active', !!enabled);
                if (speedSlider) speedSlider.value = speed;
                if (speedValue) speedValue.textContent = (speed / 100).toFixed(1) + 'x';
                if (autoToggle) autoToggle.classList.toggle('on', !!autoSpeed);
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
