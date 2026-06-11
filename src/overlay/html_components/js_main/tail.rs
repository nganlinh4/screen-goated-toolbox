pub(super) fn get() -> String {
    r###"        // Update settings from native side (used when overlay is shown with saved config)
        window.updateSettings = function(settings) {
            // Update audio source toggle
            if (settings.audioSource && micBtn && deviceBtn) {
                if (settings.audioSource === 'device') {
                    micBtn.classList.remove('active');
                    deviceBtn.classList.add('active');
                } else {
                    micBtn.classList.add('active');
                    deviceBtn.classList.remove('active');
                }
            }

            // Update language select
            if (settings.targetLanguage && langSelect) {
                langSelect.value = settings.targetLanguage;
            }

            // Update translation model dropdown
            if (settings.translationModel) {
                const tlSel = document.getElementById('translation-model-select');
                if (tlSel) tlSel.value = settings.translationModel;
                // Legacy icons
                if (modelIcons.length) {
                    modelIcons.forEach(icon => {
                        icon.classList.toggle('active', icon.getAttribute('data-value') === settings.translationModel);
                    });
                }
            }

            // Update transcription model dropdown + language badge
            if (settings.transcriptionModel) {
                const tcSel = document.getElementById('transcription-model-select');
                if (tcSel) tcSel.value = settings.transcriptionModel;
                updateTransLangSelectState(settings.transcriptionModel);
                applyS2sMode(
                    isS2sTranscriptionModel(settings.transcriptionModel),
                    settings.transcriptionModel
                );
                // Legacy icons
                if (transModelIcons && transModelIcons.length) {
                    transModelIcons.forEach(icon => {
                        icon.classList.toggle('active', icon.getAttribute('data-value') === settings.transcriptionModel);
                    });
                }
            }

            if (settings.transcriptionLanguage && transLangSelect && settings.transcriptionModel === 'zipformer') {
                transLangSelect.value = settings.transcriptionLanguage.toLowerCase();
            }

            // Update font size
            if (settings.fontSize && settings.fontSize !== currentFontSize) {
                currentFontSize = settings.fontSize;
                content.style.fontSize = currentFontSize + 'px';
                minContentHeight = 0;
                content.style.minHeight = '';
            }
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
"###
    .to_string()
}
