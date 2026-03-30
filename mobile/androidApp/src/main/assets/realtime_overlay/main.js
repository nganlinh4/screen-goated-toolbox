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
            geminiLive31Title: 'Gemini Live 3.1 (Cloud)',
            parakeetTitle: 'Parakeet (Local)',
            gemmaTitle: 'AI Translation (Gemma)',
            cerebrasTitle: 'Instant AI (Cerebras)',
            gtxTitle: 'Unlimited Translation (Google)',
            targetLanguageTitle: 'Target language',
            ttsSettingsTitle: 'Text-to-speech settings',
            ttsTitle: 'Read',
            ttsSpeed: 'Speed',
            ttsAuto: 'Auto',
            ttsVolume: 'Volume',
            downloadingModelTitle: 'Downloading model',
            pleaseWaitText: 'Please wait...',
            cancelText: 'Cancel',
            parakeetNote: '(English only)',
        };

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

        function setTranslationModel(modelName) {
            if (!modelIcons.length) return;
            setSelectedByDataValue(modelIcons, modelName);
        }

        function setTranscriptionModel(modelName) {
            if (!transModelIcons.length) return;
            setSelectedByDataValue(transModelIcons, modelName);
        }

        function setFontSize(fontSize) {
            if (!fontSize || fontSize === currentFontSize) return;
            currentFontSize = fontSize;
            content.style.fontSize = currentFontSize + 'px';
            minContentHeight = 0;
            content.style.minHeight = '';
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

        container.addEventListener('contextmenu', function(e) {
            if (e.target.closest('#controls') || e.target.closest('.language-btn')) return;
            e.preventDefault();
        });

        function onWindowDragMove(e) {
            if (!isDraggingWindow) return;
            const dx = e.screenX - dragStartX;
            const dy = e.screenY - dragStartY;
            if (dx !== 0 || dy !== 0) {
                window.ipc.postMessage('dragWindow:' + dx + ',' + dy);
                dragStartX = e.screenX;
                dragStartY = e.screenY;
            }
        }

        function onWindowDragEnd() {
            if (isDraggingWindow) {
                isDraggingWindow = false;
                document.removeEventListener('mousemove', onWindowDragMove);
                document.removeEventListener('mouseup', onWindowDragEnd);
            }
        }

        // Visibility toggle buttons
        toggleMic.addEventListener('click', function(e) {
            e.stopPropagation();
            preserveControlsScroll(() => {
                micVisible = !micVisible;
                this.classList.toggle('active', micVisible);
                this.classList.toggle('inactive', !micVisible);
                window.ipc.postMessage('toggleMic:' + (micVisible ? '1' : '0'));
            });
        });

        toggleTrans.addEventListener('click', function(e) {
            e.stopPropagation();
            preserveControlsScroll(() => {
                transVisible = !transVisible;
                this.classList.toggle('active', transVisible);
                this.classList.toggle('inactive', !transVisible);
                window.ipc.postMessage('toggleTrans:' + (transVisible ? '1' : '0'));
            });
        });

        // Function to update visibility state from native side
        window.setVisibility = function(mic, trans) {
            micVisible = mic;
            transVisible = trans;
            toggleMic.classList.toggle('active', mic);
            toggleMic.classList.toggle('inactive', !mic);
            toggleTrans.classList.toggle('active', trans);
            toggleTrans.classList.toggle('inactive', !trans);
        };

        // Function to update current TTS speed from native side
        window.updateTtsSpeed = function(speed) {
            ttsSpeed = speed;
            if (speedSlider) speedSlider.value = speed;
            if (speedValue) speedValue.textContent = (speed / 100).toFixed(1) + 'x';
        };

        // Font size controls
        fontDecrease.addEventListener('click', function(e) {
            e.stopPropagation();
            preserveControlsScroll(() => {
                if (currentFontSize > 10) {
                    currentFontSize -= 2;
                    content.style.fontSize = currentFontSize + 'px';
                    // Reset min height so text can shrink properly
                    minContentHeight = 0;
                    content.style.minHeight = '';
                    window.ipc.postMessage('fontSize:' + currentFontSize);
                }
            });
        });

        fontIncrease.addEventListener('click', function(e) {
            e.stopPropagation();
            preserveControlsScroll(() => {
                if (currentFontSize < 32) {
                    currentFontSize += 2;
                    content.style.fontSize = currentFontSize + 'px';
                    // Reset min height for fresh calculation
                    minContentHeight = 0;
                    content.style.minHeight = '';
                    window.ipc.postMessage('fontSize:' + currentFontSize);
                }
            });
        });

        // Audio source toggle buttons
        const micBtn = document.getElementById('mic-btn');
        const deviceBtn = document.getElementById('device-btn');

        if (micBtn) {
            installControlTapGuard(micBtn);
            micBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                preserveControlsScroll(() => {
                    setAudioSource('mic');
                    window.ipc.postMessage('audioSource:mic');
                });
            });
        }

        if (deviceBtn) {
            installControlTapGuard(deviceBtn);
            deviceBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                preserveControlsScroll(() => {
                    setAudioSource('device');
                    window.ipc.postMessage('audioSource:device');
                });
            });
        }



        // Language Select Logic - show short code when collapsed, full name when open
        const langSelect = document.getElementById('language-select');
        const langSelectCode = document.getElementById('language-select-code');

        window.setLocaleStrings = function(locale) {
            overlayLocale = Object.assign({}, overlayLocale, locale || {});
            updateTitleById('copy-btn', overlayLocale.copyTextTitle);
            updateTitleById('font-decrease', overlayLocale.decreaseFontTitle);
            updateTitleById('font-increase', overlayLocale.increaseFontTitle);
            updateTitleById('toggle-mic', overlayLocale.toggleTranscriptionTitle);
            updateTitleById('toggle-trans', overlayLocale.toggleTranslationTitle);
            updateTitleById('header-toggle', overlayLocale.toggleHeaderTitle);
            updateTitleById('mic-btn', overlayLocale.micInputTitle);
            updateTitleById('device-btn', overlayLocale.deviceAudioTitle);
            updateTitleById('speak-btn', overlayLocale.ttsSettingsTitle);
            updateTitleBySelector('.model-icon[data-value="google-gemma"]', overlayLocale.gemmaTitle);
            updateTitleBySelector('.model-icon[data-value="cerebras-oss"]', overlayLocale.cerebrasTitle);
            updateTitleBySelector('.model-icon[data-value="google-gtx"]', overlayLocale.gtxTitle);
            updateTitleBySelector('.trans-model-icon[data-value="gemini-live-audio"]', overlayLocale.geminiLive25Title);
            updateTitleBySelector('.trans-model-icon[data-value="parakeet"]', overlayLocale.parakeetTitle);
            updateTextNode('tts-modal-title-text', overlayLocale.ttsTitle);
            updateTextNode('tts-speed-label', overlayLocale.ttsSpeed);
            updateTextNode('tts-volume-label', overlayLocale.ttsVolume);
            updateTextNode('download-title', overlayLocale.downloadingModelTitle);
            updateTextNode('download-msg', overlayLocale.pleaseWaitText);
            updateTextNode('download-footnote', overlayLocale.parakeetNote);
            updateTextNode('download-cancel-text', overlayLocale.cancelText);
            if (ttsToggle) {
                ttsToggle.title = overlayLocale.ttsTitle;
            }
            const autoToggle = document.getElementById('auto-speed-toggle');
            if (autoToggle) {
                autoToggle.textContent = overlayLocale.ttsAuto;
                autoToggle.title = overlayLocale.ttsAuto;
            }
            updateTitleById('download-cancel-btn', overlayLocale.cancelText);
            if (langSelect) {
                langSelect.dataset.baseTitle = overlayLocale.targetLanguageTitle;
                if (window.setTargetLanguage) {
                    window.setTargetLanguage(
                        langSelect.dataset.language || '',
                        langSelect.dataset.code || '',
                    );
                }
            }
            refreshPlaceholderIfNeeded();
        };

        if (langSelect) {
            installControlTapGuard(langSelect);
            window.setTargetLanguage = function(language, code) {
                const shortCode = code || (language || '').substring(0, 2).toUpperCase();
                const baseTitle = langSelect.dataset.baseTitle || overlayLocale.targetLanguageTitle || langSelect.title || '';
                if (langSelectCode) {
                    langSelectCode.textContent = shortCode;
                }
                langSelect.dataset.language = language || '';
                langSelect.dataset.code = shortCode;
                langSelect.title = language ? (baseTitle + ': ' + language) : baseTitle;
            };

            if (langSelect.tagName === 'BUTTON') {
                langSelect.addEventListener('click', function(e) {
                    e.stopPropagation();
                    preserveControlsScroll(() => {
                        window.ipc.postMessage('showLanguagePicker');
                    });
                });
                window.setTargetLanguage(langSelect.dataset.language || '', langSelect.dataset.code || '');
            } else {
                const options = langSelect.querySelectorAll('option');
                options.forEach(opt => {
                    opt.dataset.fullname = opt.textContent;
                });

                function showCodes() {
                    options.forEach(opt => {
                        opt.textContent = opt.dataset.code || opt.dataset.fullname.substring(0, 2).toUpperCase();
                    });
                }

                function showFullNames() {
                    options.forEach(opt => {
                        opt.textContent = opt.dataset.fullname;
                    });
                }

                showCodes();
                langSelect.addEventListener('focus', showFullNames);
                langSelect.addEventListener('mousedown', function(e) {
                    e.stopPropagation();
                    showFullNames();
                });
                langSelect.addEventListener('blur', showCodes);
                langSelect.addEventListener('change', function(e) {
                    e.stopPropagation();
                    preserveControlsScroll(() => {
                        window.ipc.postMessage('language:' + this.value);
                        setTimeout(showCodes, 100);
                    });
                });
            }
        }

        // Model Toggle Switch Logic - for translation
        const modelIcons = document.querySelectorAll('.model-icon');
        if (modelIcons.length) {
            modelIcons.forEach(icon => {
                installControlTapGuard(icon);
                icon.addEventListener('click', (e) => {
                    e.stopPropagation();
                    preserveControlsScroll(() => {
                        setTranslationModel(icon.getAttribute('data-value'));
                        const val = icon.getAttribute('data-value');
                        window.ipc.postMessage('translationModel:' + val);
                    });
                });
            });
        }

        // Transcription Model Logic
        const transModelIcons = document.querySelectorAll('.trans-model-icon');
        if (transModelIcons.length) {
            transModelIcons.forEach(icon => {
                installControlTapGuard(icon);
                icon.addEventListener('click', (e) => {
                    e.stopPropagation();
                    preserveControlsScroll(() => {
                        setTranscriptionModel(icon.getAttribute('data-value'));
                        const val = icon.getAttribute('data-value');
                        window.ipc.postMessage('transcriptionModel:' + val);
                    });
                });
            });
        }
        window.setLocaleStrings(overlayLocale);

        // Signal readiness to native side so initial settings can be applied
        if (window.ipc && window.ipc.postMessage) {
            window.ipc.postMessage('overlayReady');
        }

        // Download Modal Functions
        window.showDownloadModal = function(title, msg, progress) {
            const modal = document.getElementById('download-modal');
            const overlay = document.getElementById('download-modal-overlay');
            const titleEl = document.getElementById('download-title');
            const msgEl = document.getElementById('download-msg');
            const fillEl = document.getElementById('download-fill');

            if (modal && overlay) {
                modal.classList.add('show');
                overlay.classList.add('show');
                if (titleEl) titleEl.textContent = title;
                if (msgEl) msgEl.textContent = msg;
                if (fillEl) fillEl.style.width = progress + '%';
            }
        };

        window.hideDownloadModal = function() {
            const modal = document.getElementById('download-modal');
            const overlay = document.getElementById('download-modal-overlay');
            if (modal && overlay) {
                modal.classList.remove('show');
                overlay.classList.remove('show');
            }
        };

        // Cancel download button handler
        const downloadCancelBtn = document.getElementById('download-cancel-btn');
        if (downloadCancelBtn) {
            downloadCancelBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                // Send cancel message to native side
                window.ipc.postMessage('cancelDownload');
            });
        }

        // Update settings from native side (used when overlay is shown with saved config)
        window.updateSettings = function(settings) {
            const pinnedControlsScroll = controls ? controls.scrollLeft : controlsScrollLeft;
            if (settings.audioSource) setAudioSource(settings.audioSource);

            if (settings.targetLanguage && langSelect) {
                if (window.setTargetLanguage) {
                    window.setTargetLanguage(settings.targetLanguage, settings.targetLanguageCode);
                } else {
                    langSelect.value = settings.targetLanguage;
                }
            }

            if (settings.translationModel) setTranslationModel(settings.translationModel);

            if (settings.transcriptionModel) setTranscriptionModel(settings.transcriptionModel);

            if (settings.fontSize) setFontSize(settings.fontSize);
            restoreControlsScroll(pinnedControlsScroll);
        };

        // Handle resize to keep text at bottom
        let lastWidth = viewport.clientWidth;
        const resizeObserver = new ResizeObserver(entries => {
            for (let entry of entries) {
                if (Math.abs(entry.contentRect.width - lastWidth) > 5) {
                    lastWidth = entry.contentRect.width;
                    // Reset min height on width change (reflow)
                    minContentHeight = 0;
                    content.style.minHeight = '';

                    // Force scroll to bottom immediately to prevent jump
                    if (content.scrollHeight > viewport.clientHeight) {
                        viewport.scrollTop = content.scrollHeight - viewport.clientHeight;
                    }
                    targetScrollTop = viewport.scrollTop;
                    currentScrollTop = targetScrollTop;
                }
            }
        });
        resizeObserver.observe(viewport);

        let isFirstText = true;
        let currentScrollTop = 0;
        let targetScrollTop = 0;
        let animationFrame = null;
        let layoutFrame = null;
        let minContentHeight = 0;

        function animateScroll() {
            const diff = targetScrollTop - currentScrollTop;

            if (Math.abs(diff) > 0.5) {
                const ease = Math.min(0.08, Math.max(0.02, Math.abs(diff) / 1000));
                currentScrollTop += diff * ease;
                viewport.scrollTop = currentScrollTop;
                animationFrame = requestAnimationFrame(animateScroll);
            } else {
                currentScrollTop = targetScrollTop;
                viewport.scrollTop = currentScrollTop;
                animationFrame = null;
            }
        }

        let currentOldTextLength = 0;
        let previousOldText = '';
        let previousNewText = '';

        function scheduleLayoutUpdate() {
            if (layoutFrame) {
                return;
            }
            layoutFrame = requestAnimationFrame(() => {
                const startedAt = performance.now();
                layoutFrame = null;
                const naturalHeight = content.offsetHeight;
                if (naturalHeight > minContentHeight) {
                    minContentHeight = naturalHeight;
                }
                content.style.minHeight = minContentHeight + 'px';
                const viewportHeight = viewport.offsetHeight;
                if (minContentHeight > viewportHeight) {
                    const maxScroll = minContentHeight - viewportHeight;
                    if (maxScroll > targetScrollTop) {
                        targetScrollTop = maxScroll;
                    }
                }
                if (!animationFrame) {
                    animationFrame = requestAnimationFrame(animateScroll);
                }
                const durationMs = performance.now() - startedAt;
                if (durationMs > PERF_THRESHOLD_MS && window.logPerf) {
                    window.logPerf('layout', {
                        durationMs: Number(durationMs.toFixed(2)),
                        naturalHeight,
                        viewportHeight,
                        minContentHeight,
                        childCount: content.childElementCount
                    });
                }
            });
        }
