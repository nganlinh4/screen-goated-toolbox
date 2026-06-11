        const container = document.getElementById('container');
        const viewport = document.getElementById('viewport');
        const content = document.getElementById('content');
        const header = document.getElementById('header');
        const headerToggle = document.getElementById('header-toggle');
        const toggleMic = document.getElementById('toggle-mic');
        const toggleTrans = document.getElementById('toggle-trans');
        const fontDecrease = document.getElementById('font-decrease');
        const fontIncrease = document.getElementById('font-increase');
        const copyBtn = document.getElementById('copy-btn');
        const controls = document.getElementById('controls');

        let currentFontSize = {{FONT_SIZE}};
        let micVisible = true;
        let transVisible = true;
        let headerCollapsed = false;
        let controlsScrollLeft = 0;
        let overlayLocale = {
            placeholderText: 'Waiting for speech...',
            copyTextTitle: 'Copy text',
            decreaseFontTitle: 'Decrease font size',
            increaseFontTitle: 'Increase font size',
            toggleTranscriptionTitle: 'Toggle transcription',
            toggleTranslationTitle: 'Toggle translation',
            toggleHeaderTitle: 'Toggle header',
            micInputTitle: 'Microphone input',
            deviceAudioTitle: 'Device audio',
            geminiLive25Title: 'Gemini Live 2.5 (Cloud)',
            geminiLiveTitle: 'Gemini Live (Cloud)',
            geminiLive31Title: 'Gemini Live 3.1 (Cloud)',
            geminiS2sTitle: 'Gemini S2S',
            unavailableSuffix: 'Unavailable',
            llmLabel: 'LLM',
            gtxLabel: 'Google Translate',
            transcriptionModelTitle: 'Transcription model',
            translationModelTitle: 'Translation model',
            transcriptionLanguageTitle: 'Transcription language',
            targetLanguageTitle: 'Target language',
            s2sTranslationModelTitle: 'Gemini S2S uses the TTS Gemini Live model',
            s2sTargetLanguageTitle: 'Change target language and restart the current S2S session',
            directSpeechTitle: 'Direct speech output',
            ttsSettingsTitle: 'Text-to-speech settings',
            ttsS2sLockedTitle: 'Direct speech output is always on for Gemini S2S',
            ttsEnableTitle: 'Enable realtime reading',
            ttsTitle: 'Read',
            ttsSpeed: 'Speed',
            ttsAuto: 'Auto',
            ttsVolume: 'Volume',
            downloadingModelTitle: 'Downloading model',
            pleaseWaitText: 'Please wait...',
            cancelText: 'Cancel',
        };
        let s2sMode = false;

        function restoreControlsScroll(pinnedScrollLeft) {
            if (!controls) return;
            controlsScrollLeft = pinnedScrollLeft;
            requestAnimationFrame(() => {
                controls.scrollLeft = pinnedScrollLeft;
                controlsScrollLeft = pinnedScrollLeft;
            });
        }

        function preserveControlsScroll(callback) {
            const pinnedScrollLeft = controls ? controls.scrollLeft : 0;
            callback();
            restoreControlsScroll(pinnedScrollLeft);
        }

        if (controls) {
            controls.addEventListener('scroll', function() {
                controlsScrollLeft = controls.scrollLeft;
            }, { passive: true });
        }

        function installControlTapGuard(element) {
            if (!element) return;
            if (element.tagName === 'INPUT' && element.type === 'range') return;
            if (typeof element.tabIndex === 'number') {
                element.tabIndex = -1;
            }
        }

        function updateTextNode(id, value) {
            const element = document.getElementById(id);
            if (element) {
                element.textContent = value;
            }
        }

        function updateTitleById(id, value) {
            const element = document.getElementById(id);
            if (element) {
                element.title = value;
            }
        }

        function updateTitleBySelector(selector, value) {
            document.querySelectorAll(selector).forEach(node => {
                node.title = value;
            });
        }

        function refreshPlaceholderIfNeeded() {
            const placeholder = content ? content.querySelector('.placeholder') : null;
            if (placeholder) {
                placeholder.textContent = overlayLocale.placeholderText;
            }
        }

        window.getPlaceholderMarkup = function() {
            return '<span class="placeholder">' + overlayLocale.placeholderText + '</span>';
        };

        function setSelectedByDataValue(nodes, value) {
            nodes.forEach(node => {
                node.classList.toggle('active', node.getAttribute('data-value') === value);
            });
        }

        function setAudioSource(source) {
            if (!micBtn || !deviceBtn) return;
            micBtn.classList.toggle('active', source === 'mic');
            deviceBtn.classList.toggle('active', source === 'device');
        }

        // Model display labels: resolved from overlayLocale at call time so
        // setLocaleStrings can swap them without rebuilding the DOM.
        function translationModelLabel(modelName) {
            switch (modelName) {
                case 'text-llm': return overlayLocale.llmLabel || 'LLM';
                case 'google-gtx': return overlayLocale.gtxLabel || 'Google Translate';
                default: return modelName;
            }
        }
        const TRANSCRIPTION_MODEL_LABELS = {
            'gemini-live-audio': 'Gemini Live',
            'gemini-3.5-translate': 'Gemini 3.5 translate',
            'moonshine-tiny-streaming': 'Moonshine Tiny',
            'moonshine-small-streaming': 'Moonshine Small',
            'moonshine-medium-streaming': 'Moonshine Medium',
            'zipformer': 'Zipformer',
        };

        function transcriptionModelLabel(modelName) {
            if (modelName === 'gemini-live-s2s') {
                return overlayLocale.geminiS2sTitle || 'Gemini S2S';
            }
            if (modelName === 'parakeet') {
                return 'Parakeet (' + (overlayLocale.unavailableSuffix || 'Unavailable') + ')';
            }
            return TRANSCRIPTION_MODEL_LABELS[modelName] || modelName;
        }

        function applyS2sMode(enabled) {
            s2sMode = !!enabled;
            document.documentElement.dataset.s2s = s2sMode ? '1' : '0';
            document.body.dataset.s2s = s2sMode ? '1' : '0';
            const translationBtn = document.getElementById('translation-model-btn');
            if (translationBtn) {
                translationBtn.disabled = s2sMode;
                translationBtn.hidden = s2sMode;
                translationBtn.classList.toggle('disabled', s2sMode);
                translationBtn.title = s2sMode
                    ? (overlayLocale.s2sTranslationModelTitle || overlayLocale.translationModelTitle)
                    : overlayLocale.translationModelTitle;
            }
            const langBtn = document.getElementById('language-select');
            if (langBtn) {
                langBtn.dataset.baseTitle = s2sMode
                    ? (overlayLocale.s2sTargetLanguageTitle || overlayLocale.targetLanguageTitle)
                    : overlayLocale.targetLanguageTitle;
                if (window.setTargetLanguage) {
                    window.setTargetLanguage(langBtn.dataset.language || '', langBtn.dataset.code || '');
                }
            }
            if (speakBtn) {
                speakBtn.classList.toggle('active', s2sMode || ttsEnabled);
                speakBtn.classList.toggle('locked', s2sMode);
                speakBtn.title = s2sMode
                    ? (overlayLocale.directSpeechTitle || overlayLocale.ttsSettingsTitle)
                    : overlayLocale.ttsSettingsTitle;
            }
            if (ttsToggle) {
                ttsToggle.classList.toggle('on', s2sMode || ttsEnabled);
                ttsToggle.classList.toggle('locked', s2sMode);
                ttsToggle.title = s2sMode
                    ? (overlayLocale.ttsS2sLockedTitle || overlayLocale.ttsTitle)
                    : (overlayLocale.ttsEnableTitle || overlayLocale.ttsTitle);
            }
        }

        function setTranslationModel(modelName) {
            const btn = document.getElementById('translation-model-btn');
            const label = document.getElementById('translation-model-label');
            if (btn) btn.dataset.value = modelName;
            if (label) label.textContent = translationModelLabel(modelName);
            // Legacy
            const icons = document.querySelectorAll('.model-icon');
            if (icons.length) setSelectedByDataValue(icons, modelName);
        }

        function setTranscriptionModel(modelName) {
            const btn = document.getElementById('transcription-model-btn');
            const label = document.getElementById('transcription-model-label');
            if (btn) btn.dataset.value = modelName;
            if (label) {
                label.textContent = transcriptionModelLabel(modelName);
            }
            // Legacy
            const icons = document.querySelectorAll('.trans-model-icon');
            if (icons.length) setSelectedByDataValue(icons, modelName);
            applyS2sMode(modelName === 'gemini-live-s2s' || modelName === 'gemini-3.5-translate');
        }

        function setFontSize(fontSize) {
            if (!fontSize || fontSize === currentFontSize) return;
            currentFontSize = fontSize;
            content.style.fontSize = currentFontSize + 'px';
            minContentHeight = 0;
            content.style.minHeight = '';
        }

        // Transcription language badge
        const transLangBadge = document.getElementById('trans-lang-badge');
        let currentTransLangCode = 'EN';

        function updateTransLangBadgeState(modelName) {
            if (!transLangBadge) return;
            if (modelName && (modelName.includes('gemini') || modelName.includes('qwen'))) {
                transLangBadge.textContent = 'ALL';
                transLangBadge.dataset.code = 'ALL';
                transLangBadge.classList.add('greyed');
                transLangBadge.hidden = true;
            } else if (modelName && modelName.includes('moonshine')) {
                transLangBadge.textContent = 'EN';
                transLangBadge.dataset.code = 'EN';
                transLangBadge.classList.add('greyed');
                transLangBadge.hidden = true;
            } else if (modelName === 'zipformer') {
                // Zipformer — show current language, pressable
                transLangBadge.textContent = currentTransLangCode.toUpperCase();
                transLangBadge.classList.remove('greyed');
                transLangBadge.hidden = false;
            }
        }

        // Override setTranscriptionModel to also update badge
        const _origSetTranscriptionModel = setTranscriptionModel;
        setTranscriptionModel = function(modelName) {
            _origSetTranscriptionModel(modelName);
            updateTransLangBadgeState(modelName);
        };

        window.setTranscriptionLanguage = function(langCode, langName) {
            if (!transLangBadge) return;
            currentTransLangCode = langCode || 'EN';
            transLangBadge.textContent = currentTransLangCode.toUpperCase();
            transLangBadge.dataset.code = currentTransLangCode;
            if (langName) transLangBadge.title = langName;
        };

        if (transLangBadge) {
            transLangBadge.addEventListener('click', function() {
                if (this.classList.contains('greyed')) return;
                if (window.ipc) window.ipc.postMessage('showTranscriptionLanguagePicker');
            });
        }

        window.setAudioSource = setAudioSource;
        window.setTranslationModel = setTranslationModel;
        window.setTranscriptionModel = setTranscriptionModel;
        window.setFontSize = setFontSize;
        window.setTheme = function(isDark) {
            document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light');
        };

        // TTS Modal elements
        const speakBtn = document.getElementById('speak-btn');
        const ttsModal = document.getElementById('tts-modal');
        const ttsModalOverlay = document.getElementById('tts-modal-overlay');
        const ttsToggle = document.getElementById('tts-toggle');
        const ttsModalTitleText = document.getElementById('tts-modal-title-text');
        const ttsSpeedLabel = document.getElementById('tts-speed-label');
        const ttsVolumeLabel = document.getElementById('tts-volume-label');
        const speedSlider = document.getElementById('speed-slider');
        const speedValue = document.getElementById('speed-value');
        const downloadFootnote = document.getElementById('download-footnote');
        const downloadCancelText = document.getElementById('download-cancel-text');
        let ttsEnabled = false;
        let ttsSpeed = 100;
        const PERF_THRESHOLD_MS = 8;

        window.logPerf = function(stage, details) {
            try {
                const payload = Object.assign({ stage }, details || {});
                window.ipc.postMessage('perf:' + JSON.stringify(payload));
            } catch (_error) {
                // Ignore perf logging errors
            }
        };

        // TTS Modal Logic
        if (speakBtn && ttsModal && ttsModalOverlay) {
            speakBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                preserveControlsScroll(() => {
                    ttsModal.classList.toggle('show');
                    ttsModalOverlay.classList.toggle('show');
                });
            });

            ttsModalOverlay.addEventListener('click', function() {
                ttsModal.classList.remove('show');
                ttsModalOverlay.classList.remove('show');
            });
        }

        if (ttsToggle) {
            ttsToggle.addEventListener('click', function(e) {
                e.stopPropagation();
                preserveControlsScroll(() => {
                    if (s2sMode) {
                        applyS2sMode(true);
                        return;
                    }
                    ttsEnabled = !ttsEnabled;
                    this.classList.toggle('on', ttsEnabled);
                    if (speakBtn) speakBtn.classList.toggle('active', ttsEnabled);
                    window.ipc.postMessage('ttsEnabled:' + (ttsEnabled ? '1' : '0'));
                });
            });
        }

        if (speedSlider && speedValue) {
            const autoToggle = document.getElementById('auto-speed-toggle');
            let autoSpeed = true; // Default: auto is on

            speedSlider.addEventListener('input', function(e) {
                e.stopPropagation();
                ttsSpeed = parseInt(this.value);
                speedValue.textContent = (ttsSpeed / 100).toFixed(1) + 'x';
                window.ipc.postMessage('ttsSpeed:' + ttsSpeed);
                // Auto turns off when user manually adjusts slider
                if (autoSpeed && autoToggle) {
                    autoSpeed = false;
                    autoToggle.classList.remove('on');
                }
            });

            if (autoToggle) {
                autoToggle.addEventListener('click', function(e) {
                    e.stopPropagation();
                    autoSpeed = !autoSpeed;
                    this.classList.toggle('on', autoSpeed);
                    window.ipc.postMessage('ttsAutoSpeed:' + (autoSpeed ? '1' : '0'));
                });
            }
        }

        const volumeSlider = document.getElementById('volume-slider');
        const volumeValue = document.getElementById('volume-value');
        [
            copyBtn,
            fontDecrease,
            fontIncrease,
            toggleMic,
            toggleTrans,
            headerToggle,
            speakBtn,
            ttsToggle,
        ].forEach(installControlTapGuard);
        if (volumeSlider && volumeValue) {
            volumeSlider.addEventListener('input', function(e) {
                e.stopPropagation();
                const vol = parseInt(this.value);
                volumeValue.textContent = vol + '%';
                window.ipc.postMessage('ttsVolume:' + vol);
            });
        }

        // Header toggle (with null check in case element is commented out)
        if (headerToggle) {
            headerToggle.addEventListener('click', function(e) {
                e.stopPropagation();
                headerCollapsed = !headerCollapsed;
                header.classList.toggle('collapsed', headerCollapsed);
                headerToggle.classList.toggle('collapsed', headerCollapsed);
            });
        }

        // Copy button handler
        if (copyBtn) {
            copyBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                preserveControlsScroll(() => {
                    // Get all text content (excluding placeholder)
                    const textContent = content.textContent.trim();
                    if (textContent && !content.querySelector('.placeholder')) {
                        // Send to Rust via IPC for clipboard (navigator.clipboard not available in WebView2)
                        window.ipc.postMessage('copyText:' + textContent);
                        // Show success feedback
                        copyBtn.classList.add('copied');
                        const icon = copyBtn.querySelector('.material-symbols-rounded');
                        if (icon) icon.innerHTML = '{{CHECK_SVG}}';
                        setTimeout(() => {
                            copyBtn.classList.remove('copied');
                            if (icon) icon.innerHTML = '{{COPY_SVG}}';
                        }, 1500);
                    }
                });
            });
        }

        // Drag support
        let isDraggingWindow = false;
        let dragStartX = 0;
        let dragStartY = 0;

        container.addEventListener('mousedown', function(e) {
            if (e.button !== 0) return; // Only left click
            if (e.target.closest('#controls') || e.target.closest('#header-toggle') || e.target.closest('#tts-modal')) return;
            e.preventDefault();
            isDraggingWindow = true;
            dragStartX = e.screenX;
            dragStartY = e.screenY;
            document.addEventListener('mousemove', onWindowDragMove);
            document.addEventListener('mouseup', onWindowDragEnd);
        });
