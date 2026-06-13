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
            updateTitleBySelector('.model-icon[data-value="text-llm"]', overlayLocale.llmLabel);
            updateTitleBySelector('.model-icon[data-value="google-gtx"]', overlayLocale.gtxLabel);
            updateTitleById('transcription-model-btn', overlayLocale.transcriptionModelTitle);
            updateTitleById('trans-lang-badge', overlayLocale.transcriptionLanguageTitle);
            // Refresh the displayed picker label too, since its source switched to overlayLocale.
            const currentTranslationModelBtn = document.getElementById('translation-model-btn');
            if (currentTranslationModelBtn && window.setTranslationModel) {
                window.setTranslationModel(currentTranslationModelBtn.dataset.value || '');
            }
            updateTitleBySelector('.trans-model-icon[data-value="gemini-live-audio"]', 'Gemini Live');
            updateTitleBySelector('.trans-model-icon[data-value="gemini-live-audio-3.1"]', 'Gemini S2S');
            updateTitleBySelector('.trans-model-icon[data-value="gemini-3.5-translate"]', 'Gemini Translate');
            const currentTranscriptionModelBtn = document.getElementById('transcription-model-btn');
            if (currentTranscriptionModelBtn && window.setTranscriptionModel) {
                window.setTranscriptionModel(currentTranscriptionModelBtn.dataset.value || '');
            }
            updateTextNode('tts-modal-title-text', overlayLocale.ttsTitle);
            updateTextNode('tts-speed-label', overlayLocale.ttsSpeed);
            updateTextNode('tts-volume-label', overlayLocale.ttsVolume);
            updateTextNode('download-title', overlayLocale.downloadingModelTitle);
            updateTextNode('download-msg', overlayLocale.pleaseWaitText);
            updateTextNode('download-footnote', '');
            updateTextNode('download-cancel-text', overlayLocale.cancelText);
            applyS2sMode(s2sMode);
            const autoToggle = document.getElementById('auto-speed-toggle');
            if (autoToggle) {
                autoToggle.textContent = overlayLocale.ttsAuto;
                autoToggle.title = overlayLocale.ttsAuto;
            }
            updateTitleById('download-cancel-btn', overlayLocale.cancelText);
            if (langSelect) {
                langSelect.dataset.baseTitle = s2sMode
                    ? (overlayLocale.s2sTargetLanguageTitle || overlayLocale.targetLanguageTitle)
                    : overlayLocale.targetLanguageTitle;
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

        // Translation model button → native picker via IPC
        const translationModelBtn = document.getElementById('translation-model-btn');
        if (translationModelBtn) {
            installControlTapGuard(translationModelBtn);
            translationModelBtn.addEventListener('pointerup', (e) => {
                e.stopPropagation();
                e.preventDefault();
                if (s2sMode) return;
                if (window.ipc) window.ipc.postMessage('showTranslationModelPicker');
            });
        }

        // Transcription model button → native picker via IPC
        const transcriptionModelBtn = document.getElementById('transcription-model-btn');
        if (transcriptionModelBtn) {
            installControlTapGuard(transcriptionModelBtn);
            transcriptionModelBtn.addEventListener('pointerup', (e) => {
                e.stopPropagation();
                e.preventDefault();
                if (window.ipc) window.ipc.postMessage('showTranscriptionModelPicker');
            });
        }

        // Legacy: keep icon handlers for backward compat with old HTML
        const modelIcons = document.querySelectorAll('.model-icon');
        if (modelIcons.length) {
            modelIcons.forEach(icon => {
                installControlTapGuard(icon);
                icon.addEventListener('click', (e) => {
                    e.stopPropagation();
                    if (s2sMode) return;
                    preserveControlsScroll(() => {
                        setTranslationModel(icon.getAttribute('data-value'));
                        window.ipc.postMessage('translationModel:' + icon.getAttribute('data-value'));
                    });
                });
            });
        }
        const transModelIcons = document.querySelectorAll('.trans-model-icon');
        if (transModelIcons.length) {
            transModelIcons.forEach(icon => {
                installControlTapGuard(icon);
                icon.addEventListener('click', (e) => {
                    e.stopPropagation();
                    preserveControlsScroll(() => {
                        setTranscriptionModel(icon.getAttribute('data-value'));
                        window.ipc.postMessage('transcriptionModel:' + icon.getAttribute('data-value'));
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

            if (settings.transcriptionLanguage && window.setTranscriptionLanguage) {
                window.setTranscriptionLanguage(settings.transcriptionLanguage, settings.transcriptionLanguageName);
            }

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
        let previousFullText = '';

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
