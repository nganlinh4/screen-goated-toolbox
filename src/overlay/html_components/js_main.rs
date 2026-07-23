mod tail;

pub fn get(font_size: u32) -> String {
    format!(
        r###"        const container = document.getElementById('container');
        const viewport = document.getElementById('viewport');
        const content = document.getElementById('content');
        const header = document.getElementById('header');
        const headerToggle = document.getElementById('header-toggle');
        const toggleMic = document.getElementById('toggle-mic');
        const toggleTrans = document.getElementById('toggle-trans');
        const fontDecrease = document.getElementById('font-decrease');
        const fontIncrease = document.getElementById('font-increase');
        const resizeHint = document.getElementById('resize-hint');
        const copyBtn = document.getElementById('copy-btn');

        let currentFontSize = {font_size};
        let isResizing = false;
        let resizeStartX = 0;
        let resizeStartY = 0;
        let micVisible = true;
        let transVisible = true;
        let headerCollapsed = false;
        let isS2sMode = document.body && document.body.dataset.s2s === '1';

        // TTS Modal elements
        const speakBtn = document.getElementById('speak-btn');
        const ttsModal = document.getElementById('tts-modal');
        const ttsModalOverlay = document.getElementById('tts-modal-overlay');
        const ttsToggle = document.getElementById('tts-toggle');
        const speedSlider = document.getElementById('speed-slider');
        const speedValue = document.getElementById('speed-value');
        let ttsEnabled = isS2sMode;
        let ttsSpeed = 100;

        // TTS Modal Logic
        if (speakBtn && ttsModal && ttsModalOverlay) {{
            speakBtn.addEventListener('click', function(e) {{
                e.stopPropagation();
                ttsModal.classList.toggle('show');
                ttsModalOverlay.classList.toggle('show');
            }});

            ttsModalOverlay.addEventListener('click', function() {{
                ttsModal.classList.remove('show');
                ttsModalOverlay.classList.remove('show');
            }});
        }}

        if (ttsToggle) {{
            ttsToggle.addEventListener('click', function(e) {{
                e.stopPropagation();
                if (isS2sMode) {{
                    e.preventDefault();
                    ttsEnabled = true;
                    this.classList.add('on');
                    if (speakBtn) speakBtn.classList.add('active');
                    window.ipc.postMessage('ttsEnabled:1');
                    return;
                }}
                ttsEnabled = !ttsEnabled;
                this.classList.toggle('on', ttsEnabled);
                if (speakBtn) speakBtn.classList.toggle('active', ttsEnabled);
                window.ipc.postMessage('ttsEnabled:' + (ttsEnabled ? '1' : '0'));
            }});
        }}

        window.setTtsEnabled = function(enabled) {{
            ttsEnabled = isS2sMode ? true : !!enabled;
            if (ttsToggle) ttsToggle.classList.toggle('on', ttsEnabled);
            if (speakBtn) speakBtn.classList.toggle('active', ttsEnabled);
        }};
        if (isS2sMode) {{
            window.setTtsEnabled(true);
        }}

        function isS2sTranscriptionModel(modelName) {{
            return modelName === 'gemini-live-s2s' || modelName === 'google-gemini-3-5-live-translate-audio';
        }}
        window.isS2sTranscriptionModel = isS2sTranscriptionModel;

        function isLiveTranslateTranscriptionModel(modelName) {{
            return modelName === 'google-gemini-3-5-live-translate-audio';
        }}
        window.isLiveTranslateTranscriptionModel = isLiveTranslateTranscriptionModel;

        function applyS2sMode(isS2s, modelName) {{
            const activeModel = modelName ||
                (document.getElementById('transcription-model-select') || {{}}).value ||
                '';
            const isLiveTranslate = isLiveTranslateTranscriptionModel(activeModel);
            isS2sMode = !!isS2s;
            document.body.dataset.s2s = isS2s ? '1' : '0';
            document.body.dataset.liveTranslate = isLiveTranslate ? '1' : '0';
            if (isS2s) {{
                ttsEnabled = true;
            }}

            const translationModelSelect = document.getElementById('translation-model-select');
            if (translationModelSelect) {{
                translationModelSelect.disabled = isS2s;
                translationModelSelect.hidden = isS2s;
                translationModelSelect.title = isS2s
                    ? window.REALTIME_L10N.s2sTranslationModel
                    : window.REALTIME_L10N.translationModel;
            }}
            if (langSelect) {{
                langSelect.disabled = false;
                langSelect.title = isS2s
                    ? window.REALTIME_L10N.s2sTargetLanguage
                    : window.REALTIME_L10N.targetLanguage;
            }}
            if (speakBtn) {{
                speakBtn.classList.toggle('active', isS2s || ttsEnabled);
                speakBtn.classList.toggle('locked', isS2s);
                speakBtn.title = isS2s ? window.REALTIME_L10N.directSpeech : window.REALTIME_L10N.ttsSettings;
            }}
            if (ttsToggle) {{
                ttsToggle.classList.toggle('locked', isS2s);
                ttsToggle.classList.toggle('on', isS2s || ttsEnabled);
                ttsToggle.title = isS2s
                    ? window.REALTIME_L10N.ttsS2sLocked
                    : window.REALTIME_L10N.ttsEnable;
            }}
        }}
        window.applyS2sMode = applyS2sMode;

        if (speedSlider && speedValue) {{
            const autoToggle = document.getElementById('auto-speed-toggle');
            let autoSpeed = true; // Default: auto is on

            speedSlider.addEventListener('input', function(e) {{
                e.stopPropagation();
                ttsSpeed = parseInt(this.value);
                speedValue.textContent = (ttsSpeed / 100).toFixed(1) + 'x';
                window.ipc.postMessage('ttsSpeed:' + ttsSpeed);
                // Auto turns off when user manually adjusts slider
                if (autoSpeed && autoToggle) {{
                    autoSpeed = false;
                    autoToggle.classList.remove('on');
                }}
            }});

            if (autoToggle) {{
                autoToggle.addEventListener('click', function(e) {{
                    e.stopPropagation();
                    autoSpeed = !autoSpeed;
                    this.classList.toggle('on', autoSpeed);
                    window.ipc.postMessage('ttsAutoSpeed:' + (autoSpeed ? '1' : '0'));
                }});
            }}
        }}

        const volumeSlider = document.getElementById('volume-slider');
        const volumeValue = document.getElementById('volume-value');
        if (volumeSlider && volumeValue) {{
            volumeSlider.addEventListener('input', function(e) {{
                e.stopPropagation();
                const vol = parseInt(this.value);
                volumeValue.textContent = vol + '%';
                window.ipc.postMessage('ttsVolume:' + vol);
            }});
        }}

        // Header toggle (with null check in case element is commented out)
        if (headerToggle) {{
            headerToggle.addEventListener('click', function(e) {{
                e.stopPropagation();
                headerCollapsed = !headerCollapsed;
                header.classList.toggle('collapsed', headerCollapsed);
                headerToggle.classList.toggle('collapsed', headerCollapsed);
            }});
        }}

        // Copy button handler
        if (copyBtn) {{
            copyBtn.addEventListener('click', function(e) {{
                e.stopPropagation();
                // Get all text content (excluding placeholder)
                const textContent = content.textContent.trim();
                if (textContent && !content.querySelector('.placeholder')) {{
                    // Send to Rust via IPC for clipboard (navigator.clipboard not available in WebView2)
                    window.ipc.postMessage('copyText:' + textContent);
                    // Show success feedback
                    copyBtn.classList.add('copied');
                    const icon = copyBtn.querySelector('.inline-svg-icon');
                    if (icon) icon.innerHTML = '{check_svg}';
                    setTimeout(() => {{
                        copyBtn.classList.remove('copied');
                        if (icon) icon.innerHTML = '{copy_svg}';
                    }}, 1500);
                }}
            }});
        }}

        // Modals cover the whole window when it is small — let their background drag the window
        ['tts-modal', 'app-modal', 'download-modal'].forEach(function(id) {{
            const modal = document.getElementById(id);
            if (!modal) return;
            modal.addEventListener('mousedown', function(e) {{
                if (e.button !== 0) return;
                if (e.target.closest('input, button, select, .toggle-switch, .ctrl-btn, .auto-toggle, .app-item')) return;
                window.ipc.postMessage('startDrag');
            }});
        }});

        // Header controls drag-to-scroll (mouse pan, mirrors Android touch scrolling)
        const controlsBar = document.getElementById('controls');
        if (controlsBar) {{
            let controlsPanning = false;
            let controlsPanned = false;
            let controlsPanStartX = 0;
            let controlsPanStartScroll = 0;

            controlsBar.addEventListener('mousedown', function(e) {{
                if (e.button !== 0) return;
                if (e.target.closest('select') || (e.target.tagName === 'INPUT' && e.target.type === 'range')) return;
                controlsPanning = true;
                controlsPanned = false;
                controlsPanStartX = e.screenX;
                controlsPanStartScroll = controlsBar.scrollLeft;
                document.addEventListener('mousemove', onControlsPanMove);
                document.addEventListener('mouseup', onControlsPanEnd);
            }});

            function onControlsPanMove(e) {{
                if (!controlsPanning) return;
                const dx = e.screenX - controlsPanStartX;
                if (!controlsPanned && Math.abs(dx) < 4) return;
                controlsPanned = true;
                controlsBar.scrollLeft = controlsPanStartScroll - dx;
            }}

            function onControlsPanEnd() {{
                controlsPanning = false;
                document.removeEventListener('mousemove', onControlsPanMove);
                document.removeEventListener('mouseup', onControlsPanEnd);
            }}

            // Swallow the click that ends a pan so buttons don't activate
            controlsBar.addEventListener('click', function(e) {{
                if (controlsPanned) {{
                    controlsPanned = false;
                    e.stopPropagation();
                    e.preventDefault();
                }}
            }}, true);

            // Mouse wheel pans the header horizontally
            controlsBar.addEventListener('wheel', function(e) {{
                if (controlsBar.scrollWidth <= controlsBar.clientWidth) return;
                e.preventDefault();
                controlsBar.scrollLeft += (e.deltaX || e.deltaY);
            }}, {{ passive: false }});
        }}

        // Drag support (left click for single window)
        container.addEventListener('mousedown', function(e) {{
            if (e.button !== 0) return; // Only left click
            if (e.target.closest('#controls') || e.target.closest('#header-toggle') || e.target.id === 'resize-hint' || isResizing) return;
            window.ipc.postMessage('startDrag');
        }});

        // Right-click group drag support (moves both windows together)
        let isGroupDragging = false;
        let groupDragStartX = 0;
        let groupDragStartY = 0;

        container.addEventListener('mousedown', function(e) {{
            if (e.button !== 2) return; // Only right click
            // Allow context menu on interactive controls
            if (e.target.closest('#controls') || e.target.closest('select')) return;

            e.preventDefault();
            isGroupDragging = true;
            groupDragStartX = e.screenX;
            groupDragStartY = e.screenY;
            window.ipc.postMessage('startGroupDrag');
            document.addEventListener('mousemove', onGroupDragMove);
            document.addEventListener('mouseup', onGroupDragEnd);
        }});

        // Prevent context menu when right-click dragging on the window body
        container.addEventListener('contextmenu', function(e) {{
            // Allow context menu on interactive controls and selects
            if (e.target.closest('#controls') || e.target.closest('select')) return;
            e.preventDefault();
        }});

        function onGroupDragMove(e) {{
            if (!isGroupDragging) return;
            const dx = e.screenX - groupDragStartX;
            const dy = e.screenY - groupDragStartY;
            if (dx !== 0 || dy !== 0) {{
                window.ipc.postMessage('groupDragMove:' + dx + ',' + dy);
                groupDragStartX = e.screenX;
                groupDragStartY = e.screenY;
            }}
        }}

        function onGroupDragEnd(e) {{
            if (isGroupDragging) {{
                isGroupDragging = false;
                document.removeEventListener('mousemove', onGroupDragMove);
                document.removeEventListener('mouseup', onGroupDragEnd);
            }}
        }}

        // Resize support
        resizeHint.addEventListener('mousedown', function(e) {{
            e.stopPropagation();
            e.preventDefault();
            isResizing = true;
            resizeStartX = e.screenX;
            resizeStartY = e.screenY;
            document.addEventListener('mousemove', onResizeMove);
            document.addEventListener('mouseup', onResizeEnd);
        }});

        function onResizeMove(e) {{
            if (!isResizing) return;
            const dx = e.screenX - resizeStartX;
            const dy = e.screenY - resizeStartY;
            if (Math.abs(dx) > 5 || Math.abs(dy) > 5) {{
                window.ipc.postMessage('resize:' + dx + ',' + dy);
                resizeStartX = e.screenX;
                resizeStartY = e.screenY;
            }}
        }}

        function onResizeEnd(e) {{
            isResizing = false;
            document.removeEventListener('mousemove', onResizeMove);
            document.removeEventListener('mouseup', onResizeEnd);
            window.ipc.postMessage('saveResize');
        }}

        // Visibility toggle buttons
        toggleMic.addEventListener('click', function(e) {{
            e.stopPropagation();
            micVisible = !micVisible;
            this.classList.toggle('active', micVisible);
            this.classList.toggle('inactive', !micVisible);
            window.ipc.postMessage('toggleMic:' + (micVisible ? '1' : '0'));
        }});

        toggleTrans.addEventListener('click', function(e) {{
            e.stopPropagation();
            transVisible = !transVisible;
            this.classList.toggle('active', transVisible);
            this.classList.toggle('inactive', !transVisible);
            window.ipc.postMessage('toggleTrans:' + (transVisible ? '1' : '0'));
        }});

        // Function to update visibility state from native side
        window.setVisibility = function(mic, trans) {{
            micVisible = mic;
            transVisible = trans;
            toggleMic.classList.toggle('active', mic);
            toggleMic.classList.toggle('inactive', !mic);
            toggleTrans.classList.toggle('active', trans);
            toggleTrans.classList.toggle('inactive', !trans);
        }};

        // Function to update current TTS speed from native side
        window.updateTtsSpeed = function(speed) {{
            ttsSpeed = speed;
            if (speedSlider) speedSlider.value = speed;
            if (speedValue) speedValue.textContent = (speed / 100).toFixed(1) + 'x';
        }};

        // Font size controls
        fontDecrease.addEventListener('click', function(e) {{
            e.stopPropagation();
            if (currentFontSize > 10) {{
                currentFontSize -= 2;
                content.style.fontSize = currentFontSize + 'px';
                // Reset min height so text can shrink properly
                minContentHeight = 0;
                content.style.minHeight = '';
                window.ipc.postMessage('fontSize:' + currentFontSize);
            }}
        }});

        fontIncrease.addEventListener('click', function(e) {{
            e.stopPropagation();
            if (currentFontSize < 32) {{
                currentFontSize += 2;
                content.style.fontSize = currentFontSize + 'px';
                // Reset min height for fresh calculation
                minContentHeight = 0;
                content.style.minHeight = '';
                window.ipc.postMessage('fontSize:' + currentFontSize);
            }}
        }});

        // Audio source toggle buttons
        const micBtn = document.getElementById('mic-btn');
        const deviceBtn = document.getElementById('device-btn');

        if (micBtn) {{
            micBtn.addEventListener('click', (e) => {{
                e.stopPropagation();
                e.preventDefault();

                // Switch to mic mode
                micBtn.classList.add('active');
                if (deviceBtn) deviceBtn.classList.remove('active');

                window.ipc.postMessage('audioSource:mic');
            }});
        }}

        if (deviceBtn) {{
            deviceBtn.addEventListener('click', (e) => {{
                e.stopPropagation();
                e.preventDefault();

                // Switch to device mode
                if (micBtn) micBtn.classList.remove('active');
                deviceBtn.classList.add('active');

                window.ipc.postMessage('audioSource:device');
            }});
        }}



        // Language Select Logic - show short code when collapsed, full name when open
        const langSelect = document.getElementById('language-select');
        if (langSelect) {{
            // Store original full names
            const options = langSelect.querySelectorAll('option');
            options.forEach(opt => {{
                opt.dataset.fullname = opt.textContent;
            }});

            // Function to show short codes (when collapsed)
            function showCodes() {{
                options.forEach(opt => {{
                    opt.textContent = opt.dataset.code || opt.dataset.fullname.substring(0, 2).toUpperCase();
                }});
            }}

            // Function to show full names (when dropdown open)
            function showFullNames() {{
                options.forEach(opt => {{
                    opt.textContent = opt.dataset.fullname;
                }});
            }}

            // Initially show codes
            showCodes();

            // Show full names when dropdown opens
            langSelect.addEventListener('focus', showFullNames);
            langSelect.addEventListener('mousedown', function(e) {{
                e.stopPropagation();
                showFullNames();
            }});

            // Show codes when dropdown closes
            langSelect.addEventListener('blur', showCodes);
            langSelect.addEventListener('change', function(e) {{
                e.stopPropagation();
                window.ipc.postMessage('language:' + this.value);
                // Delay to let the dropdown close animation finish
                setTimeout(showCodes, 100);
            }});
        }}

        // Translation model dropdown
        const translationModelSelect = document.getElementById('translation-model-select');
        if (translationModelSelect) {{
            translationModelSelect.addEventListener('change', (e) => {{
                e.stopPropagation();
                window.ipc.postMessage('translationModel:' + translationModelSelect.value);
            }});
        }}

        // Transcription model dropdown
        const transcriptionModelSelect = document.getElementById('transcription-model-select');
        if (transcriptionModelSelect) {{
            transcriptionModelSelect.addEventListener('change', (e) => {{
                e.stopPropagation();
                window.ipc.postMessage('transcriptionModel:' + transcriptionModelSelect.value);
                updateTransLangSelectState(transcriptionModelSelect.value);
            }});
        }}

        // Legacy icon handlers (backward compat)
        const modelIcons = document.querySelectorAll('.model-icon');
        if (modelIcons.length) {{
            modelIcons.forEach(icon => {{
                icon.addEventListener('click', (e) => {{
                    e.stopPropagation();
                    modelIcons.forEach(i => i.classList.remove('active'));
                    icon.classList.add('active');
                    window.ipc.postMessage('translationModel:' + icon.getAttribute('data-value'));
                }});
            }});
        }}
        const transModelIcons = document.querySelectorAll('.trans-model-icon');
        if (transModelIcons.length) {{
            transModelIcons.forEach(icon => {{
                icon.addEventListener('click', (e) => {{
                    e.stopPropagation();
                    transModelIcons.forEach(i => i.classList.remove('active'));
                    icon.classList.add('active');
                    window.ipc.postMessage('transcriptionModel:' + icon.getAttribute('data-value'));
                }});
            }});
        }}

        // Transcription Language Dropdown
        const transLangSelect = document.getElementById('transcription-lang-select');

        function updateTransLangSelectState(modelName) {{
            if (!transLangSelect) return;
            if (modelName === 'zipformer') {{
                transLangSelect.disabled = false;
                transLangSelect.hidden = false;
                if (transLangSelect.value === 'all') transLangSelect.value = 'en';
            }} else if (modelName === 'parakeet') {{
                transLangSelect.disabled = true;
                transLangSelect.hidden = true;
                transLangSelect.value = 'en';
            }} else {{
                // Gemini, Qwen, or any language-agnostic model
                transLangSelect.disabled = true;
                transLangSelect.hidden = true;
                transLangSelect.value = 'all';
            }}
        }}

        if (transcriptionModelSelect) {{
            transcriptionModelSelect.addEventListener('change', () => {{
                applyS2sMode(isS2sTranscriptionModel(transcriptionModelSelect.value), transcriptionModelSelect.value);
            }});
            applyS2sMode(isS2sTranscriptionModel(transcriptionModelSelect.value) || isS2sMode, transcriptionModelSelect.value);
        }} else {{
            applyS2sMode(isS2sMode);
        }}

        if (transLangSelect) {{
            transLangSelect.addEventListener('change', (e) => {{
                e.stopPropagation();
                window.ipc.postMessage('transcriptionLanguage:' + transLangSelect.value);
            }});
        }}

        // Download Modal Functions
        window.showDownloadModal = function(title, msg, progress) {{
            const modal = document.getElementById('download-modal');
            const overlay = document.getElementById('download-modal-overlay');
            const titleEl = document.getElementById('download-title');
            const msgEl = document.getElementById('download-msg');
            const fillEl = document.getElementById('download-fill');

            if (modal && overlay) {{
                modal.classList.add('show');
                overlay.classList.add('show');
                if (titleEl) titleEl.textContent = title;
                if (msgEl) msgEl.textContent = msg;
                if (fillEl) fillEl.style.width = progress + '%';
            }}
        }};

        window.hideDownloadModal = function() {{
            const modal = document.getElementById('download-modal');
            const overlay = document.getElementById('download-modal-overlay');
            if (modal && overlay) {{
                modal.classList.remove('show');
                overlay.classList.remove('show');
            }}
        }};

        // Cancel download button handler
        const downloadCancelBtn = document.getElementById('download-cancel-btn');
        if (downloadCancelBtn) {{
            downloadCancelBtn.addEventListener('click', function(e) {{
                e.stopPropagation();
                // Send cancel message to native side
                window.ipc.postMessage('cancelDownload');
            }});
        }}

        {tail}
"###,
        font_size = font_size,
        check_svg = crate::overlay::html_components::icons::get_icon_svg("check"),
        copy_svg = crate::overlay::html_components::icons::get_icon_svg("content_copy"),
        tail = tail::get()
    )
}
