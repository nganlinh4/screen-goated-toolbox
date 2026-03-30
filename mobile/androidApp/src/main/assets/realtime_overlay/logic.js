        const inlineDiffEnabled = {{ENABLE_INLINE_DIFF}};

        function createChunk(text, tone, extraClass = '') {
            const chunk = document.createElement('span');
            chunk.className = ['text-chunk', tone, extraClass].filter(Boolean).join(' ');
            chunk.textContent = text;
            return chunk;
        }

        function appendClassifiedText(target, text, startIndex, committedLength, extraClass = '') {
            if (!text) return;

            const endIndex = startIndex + text.length;
            if (endIndex <= committedLength) {
                target.appendChild(createChunk(text, 'old', extraClass));
                return;
            }
            if (startIndex >= committedLength) {
                target.appendChild(createChunk(text, 'new', extraClass));
                return;
            }

            const splitPoint = committedLength - startIndex;
            const committedPart = text.substring(0, splitPoint);
            const draftPart = text.substring(splitPoint);
            if (committedPart) {
                target.appendChild(createChunk(committedPart, 'old', extraClass));
            }
            if (draftPart) {
                target.appendChild(createChunk(draftPart, 'new', extraClass));
            }
        }

        function rebuildStaticText(fullText, committedLength) {
            content.innerHTML = '';
            appendClassifiedText(content, fullText, 0, committedLength);
        }

        function appendAnimatedDelta(delta, deltaStart, committedLength) {
            if (!delta) return;

            const chunk = document.createElement('span');
            chunk.className = 'text-chunk appearing';
            chunk.textContent = delta;
            content.appendChild(chunk);

            requestAnimationFrame(() => {
                chunk.classList.add('show');
                setTimeout(() => {
                    chunk.classList.remove('appearing', 'show');
                    if (deltaStart + delta.length <= committedLength) {
                        chunk.classList.add('old');
                    } else if (deltaStart >= committedLength) {
                        chunk.classList.add('new');
                    } else {
                        const committedPart = delta.substring(0, committedLength - deltaStart);
                        const draftPart = delta.substring(committedLength - deltaStart);
                        const fragment = document.createDocumentFragment();
                        appendClassifiedText(fragment, committedPart, deltaStart, committedLength);
                        appendClassifiedText(fragment, draftPart, committedLength, committedLength);
                        chunk.replaceWith(fragment);
                    }
                }, 350);
            });
        }

        function appendSequentialDiffText(target, text, startIndex, committedLength) {
            if (!text) return;

            const tokens = text.match(/\s+|[^\s]+/gu) || [];
            const highlightableCount = tokens.filter((token) => !/^\s+$/u.test(token)).length;
            const highlightStep = highlightableCount > 1 ? Math.min(55, 260 / (highlightableCount - 1)) : 0;
            let tokenStart = startIndex;
            let highlightIndex = 0;

            for (const token of tokens) {
                if (/^\s+$/u.test(token)) {
                    appendClassifiedText(target, token, tokenStart, committedLength);
                } else {
                    const appendDiffPiece = (pieceText, pieceStart) => {
                        if (!pieceText) return;

                        const pieceEnd = pieceStart + pieceText.length;
                        let tone = 'new';
                        if (pieceEnd <= committedLength) {
                            tone = 'old';
                        } else if (pieceStart < committedLength) {
                            const committedPart = pieceText.substring(0, committedLength - pieceStart);
                            const draftPart = pieceText.substring(committedLength - pieceStart);
                            appendDiffPiece(committedPart, pieceStart);
                            appendDiffPiece(draftPart, committedLength);
                            return;
                        }

                        const chunk = createChunk(pieceText, tone, 'diff-updating');
                        chunk.style.transitionDelay = `${Math.round(highlightIndex * highlightStep)}ms`;
                        target.appendChild(chunk);
                    };

                    appendDiffPiece(token, tokenStart);
                    highlightIndex += 1;
                }

                tokenStart += token.length;
            }
        }

        function tokenizeForDiff(text) {
            return text.match(/\s+|[^\s]+/gu) || [];
        }

        function canAnimateWordDiff(previousText, nextText) {
            const previousTokens = tokenizeForDiff(previousText);
            const nextTokens = tokenizeForDiff(nextText);

            let prefixCount = 0;
            const maxPrefix = Math.min(previousTokens.length, nextTokens.length);
            while (prefixCount < maxPrefix && previousTokens[prefixCount] === nextTokens[prefixCount]) {
                prefixCount += 1;
            }

            let suffixCount = 0;
            const maxSuffix = Math.min(previousTokens.length - prefixCount, nextTokens.length - prefixCount);
            while (
                suffixCount < maxSuffix &&
                previousTokens[previousTokens.length - 1 - suffixCount] === nextTokens[nextTokens.length - 1 - suffixCount]
            ) {
                suffixCount += 1;
            }

            const changedPrev = previousTokens.slice(prefixCount, previousTokens.length - suffixCount);
            const changedNext = nextTokens.slice(prefixCount, nextTokens.length - suffixCount);
            if (changedPrev.length === 0 || changedNext.length === 0) {
                return false;
            }

            const compactChangedPrev = changedPrev.filter((token) => !/^\s+$/u.test(token));
            const compactChangedNext = changedNext.filter((token) => !/^\s+$/u.test(token));
            if (compactChangedPrev.length === 0 || compactChangedNext.length === 0) {
                return false;
            }

            if (compactChangedPrev.length !== compactChangedNext.length) {
                return false;
            }

            if (compactChangedNext.length > 8) {
                return false;
            }

            return compactChangedNext.every((token) => /^[\p{L}\p{N}'’"-]+$/u.test(token));
        }

        function renderCommitAdvance(fullText, previousCommittedLength, committedLength) {
            content.innerHTML = '';

            const prefixText = fullText.substring(0, previousCommittedLength);
            const promotingText = fullText.substring(previousCommittedLength, committedLength);
            const suffixText = fullText.substring(committedLength);
            const wordMatches = Array.from(promotingText.matchAll(/[^\s]+/gu));
            const highlightableCount = wordMatches.length;
            const highlightStep = highlightableCount > 1 ? Math.min(55, 320 / (highlightableCount - 1)) : 0;
            let highlightIndex = 0;
            let localIndex = 0;

            appendClassifiedText(content, prefixText, 0, committedLength);

            for (const match of wordMatches) {
                const wordStart = match.index ?? 0;
                const wordText = match[0];
                const gapText = promotingText.substring(localIndex, wordStart);
                if (gapText) {
                    appendClassifiedText(
                        content,
                        gapText,
                        previousCommittedLength + localIndex,
                        committedLength
                    );
                }

                const chunk = createChunk(wordText, 'new', 'commit-promoting');
                chunk.style.transitionDelay = `${Math.round(highlightIndex * highlightStep)}ms`;
                content.appendChild(chunk);
                highlightIndex += 1;
                localIndex = wordStart + wordText.length;
            }

            const trailingGap = promotingText.substring(localIndex);
            if (trailingGap) {
                appendClassifiedText(
                    content,
                    trailingGap,
                    previousCommittedLength + localIndex,
                    committedLength
                );
            }

            appendClassifiedText(content, suffixText, committedLength, committedLength);

            requestAnimationFrame(() => {
                for (const chunk of content.querySelectorAll('.commit-promoting')) {
                    chunk.classList.add('settled');
                }
            });
        }

        function renderWordDiff(previousText, nextText, committedLength) {
            content.innerHTML = '';

            const previousTokens = tokenizeForDiff(previousText);
            const nextTokens = tokenizeForDiff(nextText);

            let prefixCount = 0;
            const maxPrefix = Math.min(previousTokens.length, nextTokens.length);
            while (prefixCount < maxPrefix && previousTokens[prefixCount] === nextTokens[prefixCount]) {
                prefixCount += 1;
            }

            let suffixCount = 0;
            const maxSuffix = Math.min(previousTokens.length - prefixCount, nextTokens.length - prefixCount);
            while (
                suffixCount < maxSuffix &&
                previousTokens[previousTokens.length - 1 - suffixCount] === nextTokens[nextTokens.length - 1 - suffixCount]
            ) {
                suffixCount += 1;
            }

            let tokenStart = 0;
            nextTokens.forEach((token, index) => {
                const isChanged = index >= prefixCount && index < nextTokens.length - suffixCount;
                if (isChanged) {
                    appendSequentialDiffText(content, token, tokenStart, committedLength);
                } else {
                    appendClassifiedText(content, token, tokenStart, committedLength);
                }
                tokenStart += token.length;
            });

            requestAnimationFrame(() => {
                for (const chunk of content.querySelectorAll('.diff-updating')) {
                    chunk.classList.add('settled');
                }
            });
        }

        function updateText(oldText, newText) {
            const hasContent = oldText || newText;
            const fullText = oldText + newText;
            const previousCommittedLength = currentOldTextLength;
            const committedAdvanced = oldText.length > previousCommittedLength;

            if (isFirstText && hasContent) {
                content.innerHTML = '';
                isFirstText = false;
                minContentHeight = 0;
                currentOldTextLength = 0;
                previousFullText = '';
            }

            if (!hasContent) {
                content.innerHTML = window.getPlaceholderMarkup ? window.getPlaceholderMarkup() : '<span class="placeholder">{{PLACEHOLDER_TEXT}}</span>';
                content.style.minHeight = '';
                isFirstText = true;
                minContentHeight = 0;
                targetScrollTop = 0;
                currentScrollTop = 0;
                viewport.scrollTop = 0;
                currentOldTextLength = 0;
                previousFullText = '';
                return;
            }

            if (oldText.length < currentOldTextLength && !previousFullText.startsWith(fullText)) {
                content.innerHTML = '';
                currentOldTextLength = 0;
                previousFullText = '';
            }

            if (fullText === previousFullText) {
                if (oldText.length !== previousCommittedLength) {
                    if (committedAdvanced) {
                        renderCommitAdvance(fullText, previousCommittedLength, oldText.length);
                    } else {
                        rebuildStaticText(fullText, oldText.length);
                    }
                }
            } else if (committedAdvanced) {
                renderCommitAdvance(fullText, previousCommittedLength, oldText.length);
            } else if (previousFullText && fullText.startsWith(previousFullText)) {
                if (oldText.length !== previousCommittedLength) {
                    rebuildStaticText(previousFullText, oldText.length);
                }
                appendAnimatedDelta(fullText.substring(previousFullText.length), previousFullText.length, oldText.length);
            } else if (previousFullText && inlineDiffEnabled && canAnimateWordDiff(previousFullText, fullText)) {
                renderWordDiff(previousFullText, fullText, oldText.length);
            } else {
                rebuildStaticText(fullText, oldText.length);
            }

            currentOldTextLength = oldText.length;
            previousFullText = fullText;

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
        }

        window.updateText = updateText;

        // Canvas-based volume visualizer - cute pill bars scrolling left
        const volumeCanvas = document.getElementById('volume-canvas');
        const volumeCtx = volumeCanvas ? volumeCanvas.getContext('2d') : null;

        // Cute pill configuration
        const BAR_WIDTH = 4;
        const BAR_GAP = 3;
        const BAR_SPACING = BAR_WIDTH + BAR_GAP;
        const VISIBLE_BARS = 12;

        // Each bar has its own height that persists as it scrolls
        const barHeights = new Array(VISIBLE_BARS + 2).fill(3);
        let latestRMS = 0;
        let scrollProgress = 0; // 0 to 1, represents progress to next bar shift
        let lastTime = 0;
        let lastTextPerfAt = 0;

        function updateVolume(rms) {
            latestRMS = rms;
        }

        function drawWaveform(timestamp) {
            if (!volumeCtx) return;

            // Delta time
            const dt = lastTime ? (timestamp - lastTime) / 1000 : 0.016;
            lastTime = timestamp;
            if (dt > 0.034 && window.logPerf && timestamp - lastTextPerfAt < 500) {
                window.logPerf('frameJank', {
                    frameGapMs: Number((dt * 1000).toFixed(2)),
                    nearTextRender: true,
                    childCount: content.childElementCount
                });
            }

            // Scroll progress (one full bar every ~200ms for relaxed look)
            scrollProgress += dt / 0.2;

            // When we've scrolled one full bar, shift and add new
            while (scrollProgress >= 1) {
                scrollProgress -= 1;
                // Shift all bars left (oldest falls off)
                barHeights.shift();
                // Add new bar on right with current RMS
                const h = volumeCanvas.height;
                // RMS typically 0-0.3 for speech, multiply by 180 for better visibility
                const newHeight = Math.max(3, Math.min(h - 2, latestRMS * 180 + 3));
                barHeights.push(newHeight);
            }

            // Clear
            const w = volumeCanvas.width;
            const h = volumeCanvas.height;
            volumeCtx.clearRect(0, 0, w, h);

            // Gradient
            const grad = volumeCtx.createLinearGradient(0, h, 0, 0);
            grad.addColorStop(0, '#00a8e0');
            grad.addColorStop(0.5, '#00c8ff');
            grad.addColorStop(1, '#40e0ff');
            volumeCtx.fillStyle = grad;

            // Pixel offset for smooth scroll
            const pixelOffset = scrollProgress * BAR_SPACING;

            // Draw bars
            for (let i = 0; i < barHeights.length; i++) {
                const pillHeight = barHeights[i];
                const x = i * BAR_SPACING - pixelOffset;
                const y = (h - pillHeight) / 2;

                if (x > -BAR_WIDTH && x < w) {
                    volumeCtx.beginPath();
                    volumeCtx.roundRect(x, y, BAR_WIDTH, pillHeight, BAR_WIDTH / 2);
                    volumeCtx.fill();
                }
            }

            // Apply fading curtain effect on both edges
            const fadeWidth = 15; // Width of the fade zone in canvas pixels

            volumeCtx.save();
            volumeCtx.globalCompositeOperation = 'destination-out';

            // Left fade (fully transparent at edge -> fully opaque inward)
            const leftGrad = volumeCtx.createLinearGradient(0, 0, fadeWidth, 0);
            leftGrad.addColorStop(0, 'rgba(0, 0, 0, 1)');
            leftGrad.addColorStop(1, 'rgba(0, 0, 0, 0)');
            volumeCtx.fillStyle = leftGrad;
            volumeCtx.fillRect(0, 0, fadeWidth, h);

            // Right fade (fully opaque inward -> fully transparent at edge)
            const rightGrad = volumeCtx.createLinearGradient(w - fadeWidth, 0, w, 0);
            rightGrad.addColorStop(0, 'rgba(0, 0, 0, 0)');
            rightGrad.addColorStop(1, 'rgba(0, 0, 0, 1)');
            volumeCtx.fillStyle = rightGrad;
            volumeCtx.fillRect(w - fadeWidth, 0, fadeWidth, h);

            volumeCtx.restore();

            requestAnimationFrame(drawWaveform);
        }

        // Start animation
        if (volumeCanvas) {
            requestAnimationFrame(drawWaveform);
        }

        window.updateVolume = updateVolume;

        // Model switch animation (called when 429 fallback switches models)
        function switchModel(modelName) {
            const icons = document.querySelectorAll('.model-icon');
            if (!icons.length) return;

            icons.forEach(icon => {
                const val = icon.getAttribute('data-value');
                const shouldBeActive = val === modelName;

                // Update active state
                icon.classList.remove('active');
                if (shouldBeActive) {
                    icon.classList.add('active');
                    // Add switching animation
                    icon.classList.add('switching');
                    // Remove animation class after it completes (2s)
                    setTimeout(() => icon.classList.remove('switching'), 2000);
                }
            });
        }

        window.switchModel = switchModel;

        // Clear text and reset to initial placeholder state
        function clearText() {
            content.innerHTML = window.getPlaceholderMarkup ? window.getPlaceholderMarkup() : '<span class=\"placeholder\">{{PLACEHOLDER_TEXT}}</span>';
            content.style.minHeight = '';
            isFirstText = true;
            minContentHeight = 0;
            targetScrollTop = 0;
            currentScrollTop = 0;
            viewport.scrollTop = 0;
            currentOldTextLength = 0;
            previousFullText = '';
        }

        window.clearText = clearText;
