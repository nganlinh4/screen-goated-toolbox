//! Streaming content functions for markdown view

use windows::Win32::Foundation::*;

use super::conversion::markdown_to_html;
use super::webview::create_markdown_webview_ex;
use super::WEBVIEWS;

/// Stream markdown content - optimized for rapid updates during streaming
/// Uses innerHTML instead of document.write to avoid document recreation
/// Call this during streaming, then call update_markdown_content at the end for final render
pub fn stream_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };

    stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Stream markdown content - internal version for rapid streaming updates
/// Uses innerHTML on body to avoid document recreation overhead
pub fn stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;

    // Check if webview exists
    let exists = WEBVIEWS.with(|webviews| webviews.borrow().contains_key(&hwnd_key));

    if !exists {
        // Create the webview first if it doesn't exist
        return create_markdown_webview_ex(
            parent_hwnd,
            markdown_text,
            false, // is_hovered - during streaming, use compact view
            is_refining,
            preset_prompt,
            input_text,
        );
    }

    // For streaming, we just update the body innerHTML
    // This is much faster than document.write and doesn't recreate the document
    let html = markdown_to_html(markdown_text, is_refining, preset_prompt, input_text);

    // Extract just the body content from the full HTML
    // The HTML structure is: ....<body>CONTENT</body>....
    let body_content = if let Some(body_start) = html.find("<body>") {
        let after_body = &html[body_start + 6..];
        if let Some(body_end) = after_body.find("</body>") {
            &after_body[..body_end]
        } else {
            &html[..] // Fallback to full html
        }
    } else {
        &html[..] // Fallback to full html
    };

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            // Escape for JS template literal
            let escaped_content = body_content
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("${", "\\${");

            let script = format!(
                r#"(function() {{
    const newContent = `{}`;
    const prevWordCount = window._streamWordCount || 0;

    // Update content
    document.body.innerHTML = newContent;

    // --- INTEGRATED FONT SIZING (Heuristic Optimized) ---
    var body = document.body;
    var doc = document.documentElement;
    var winH = window.innerHeight;
    var winW = window.innerWidth;
    var isConstrainedWindow = (winH < 260 || winW < 420);

    // Detect new session
    var textLen = (body.innerText || body.textContent || '').trim().length;
    var isNewSession = (!window._streamWordCount || window._streamWordCount < 5 || textLen < 50);
    var isConstrainedShortContent = isConstrainedWindow && textLen < 450;
    function fitsVertically() {{
        void body.offsetHeight;
        return doc.scrollHeight <= (winH + 2);
    }}

    // Dynamic Minimum Size
    // If text is short (< 200 chars), we allow shrinking to 6px to fit purely visual content.
    // If text is longer, we enforce 14px floor for readability.
    var minSize = (textLen < 200) ? 6 : 14;

    if (isNewSession) {{
         // Reset styles from previous session so DOM is in a known state.
         // On the very first streaming chunk, the body is hidden (opacity 0) by Rust
         // and fit_font_to_window runs the full fitting + reveals. This basic sizing
         // is a fallback for non-first isNewSession triggers (e.g. after WIPE signal).
         var maxPossible = Math.min(200, winH);
         var estimated = Math.sqrt((winW * winH) / (textLen + 1));
         var low = Math.max(minSize, Math.floor(estimated * 0.5));
         var high = Math.min(maxPossible, Math.ceil(estimated * 1.5));
         if (low > high) low = high;

         body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
         body.style.letterSpacing = '0px';
         body.style.wordSpacing = '0px';
         body.style.lineHeight = '1.5';
         body.style.paddingTop = '0';
         body.style.paddingBottom = '0';
         var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
         for (var i = 0; i < blocks.length; i++) {{
             blocks[i].style.marginBottom = '0.5em';
             blocks[i].style.paddingBottom = '0';
         }}
         void body.offsetHeight;
         var best = low;
         while (low <= high) {{
             var mid = Math.floor((low + high) / 2);
             body.style.fontSize = mid + 'px';
             if (fitsVertically()) {{
                 best = mid;
                 low = mid + 1;
             }} else {{
                 high = mid - 1;
             }}
         }}
         if (best < minSize) best = minSize;
         body.style.fontSize = best + 'px';
         // Constrained window + short text can report a near-final height on first pass.
         // Re-run once after style settle so scale is final without requiring hover-triggered fit.
         if (isConstrainedShortContent) {{
             void body.offsetHeight;
             var settleLow = minSize;
             var settleHigh = best;
             var settleBest = minSize;
             while (settleLow <= settleHigh) {{
                 var settleMid = Math.floor((settleLow + settleHigh) / 2);
                 body.style.fontSize = settleMid + 'px';
                 if (fitsVertically()) {{
                     settleBest = settleMid;
                     settleLow = settleMid + 1;
                 }} else {{
                     settleHigh = settleMid - 1;
                 }}
             }}
             body.style.fontSize = settleBest + 'px';
         }}

    }} else {{
        // Incrementally adjust font size if overflow occurs
        var hasOverflow = !fitsVertically();
        if (hasOverflow) {{
            var currentSize = parseFloat(body.style.fontSize) || 14;
            if (currentSize > minSize) {{
                // Binary search from minSize to currentSize to find the largest fitting size
                var low = minSize;
                var high = currentSize;
                var best = minSize;
                while (low <= high) {{
                    var mid = Math.floor((low + high) / 2);
                    body.style.fontSize = mid + 'px';
                    if (fitsVertically()) {{
                        best = mid;
                        low = mid + 1;
                    }} else {{
                        high = mid - 1;
                    }}
                }}
                body.style.fontSize = best + 'px';
            }}
        }}
    }}
    // ----------------------------

    // Get all word spans
    const words = document.querySelectorAll('.word');
    const newWordCount = words.length;

    // SKIP animation for the very first chunk (isNewSession)
    if (!isNewSession) {{
        let newWords = [];
        for (let i = prevWordCount; i < newWordCount; i++) {{
            newWords.push(words[i]);
        }}

        if (newWords.length > 0) {{
            // Set initial state
            newWords.forEach(w => {{
                w.style.opacity = '0';
                w.style.filter = 'blur(2px)';
            }});

            requestAnimationFrame(() => {{
                 newWords.forEach(w => {{
                    w.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out';
                    w.style.opacity = '1';
                    w.style.filter = 'blur(0)';
                 }});
            }});
        }}
    }}

    window._streamWordCount = newWordCount;
    window.scrollTo({{ top: document.body.scrollHeight, behavior: 'smooth' }});
}})()"#,
                escaped_content
            );
            let _ = webview.evaluate_script(&script);
            return true;
        }
        false
    })
}

/// Reset the stream content tracker (call when streaming ends)
/// This ensures the next streaming session starts fresh
pub fn reset_stream_counter(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            // Reset stream counters only - font will be reset at start of next session
            let _ = webview.evaluate_script(
                "window._streamPrevLen = 0; window._streamPrevContent = ''; window._streamWordCount = 0;",
            );
        }
    });
}

/// Set body opacity (hide before fitting, reveal after)
pub fn set_body_opacity(parent_hwnd: HWND, visible: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    let opacity = if visible { "1" } else { "0" };
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(&format!(
                "if(document.body) document.body.style.opacity = '{}';",
                opacity
            ));
        }
    });
}

/// Fit font size to window - call after streaming ends or on content update
/// This runs a ONE-TIME font fit calculation (no loops, no observers, safe)
/// Scales font UP if there's unfilled space, scales DOWN if overflow (but never below 8px)
/// Also adjusts font width (wdth) to prevent text wrapping when possible
pub fn fit_font_to_window(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    // Multi-pass font fitting algorithm that guarantees filling the window
    // Uses all available tools: font-size, wdth, letter-spacing, line-height, margins
    // Strategy: First fit content, then fill remaining space with line-height/margins
    let script = r#"
    (function() {
        window._sgtFitCallCount = (window._sgtFitCallCount || 0) + 1;
        if (window._sgtFitting) return;
        window._sgtFitting = true;

        // Use longer delay + requestAnimationFrame to ensure content is fully rendered
        // This is critical after streaming ends, as the DOM needs time to settle
        setTimeout(function() {
        requestAnimationFrame(function() {
        requestAnimationFrame(function() {
            // Skip font fitting for image/audio input adapters - detect by checking for slider-container
            // These have special fixed layouts that shouldn't be affected by auto-scaling
            if (document.querySelector('.slider-container') || document.querySelector('.audio-player')) {
                window._sgtFitting = false;
                return;
            }

            var _fitStart = performance.now();
            var body = document.body;
            var doc = document.documentElement;

            // Force layout recalculation before reading dimensions
            void body.offsetHeight;

            var winH = window.innerHeight;
            var winW = body.clientWidth || window.innerWidth;

            // Helper: check if content fits (re-reads scrollHeight each time for accuracy)
            function fits() { void body.offsetHeight; return doc.scrollHeight <= winH; }
            function getGap() { void body.offsetHeight; return winH - doc.scrollHeight; }

            // Helper: reset last child margin (used during binary search phases)
            function clearLastMargin() {
                var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                if (blocks.length > 0) {
                    blocks[blocks.length - 1].style.marginBottom = '0';
                }
            }

            // Get content and length
            var text = body.innerText || body.textContent || '';
            var textLen = text.trim().length;

            var isShortContent = textLen < 1500;
            var isTinyContent = textLen < 300;
            var isConstrainedWindow = (winH < 260 || winW < 420);
            var isConstrainedShortContent = isConstrainedWindow && textLen < 450;

            // Allowed ranges — match streaming's 14px readability floor
            var minSize = (textLen < 200) ? 6 : 14;
            var maxSize = isTinyContent ? 200 : (isShortContent ? 100 : 24);

            // ===== LONG TEXT: readable size + scroll =====
            // For long text (>= 1500 chars), skip expensive 8-phase fitting.
            // Streaming already set a readable font (min 14px); the content
            // scrolls naturally. Fitting would either produce unreadably small
            // text or waste ~100+ reflows to arrive at the same size.
            if (!isShortContent) {
                var currentSize = parseFloat(body.style.fontSize) || 16;
                if (currentSize < 14) body.style.fontSize = '14px';
                body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
                body.style.letterSpacing = '0px';
                body.style.lineHeight = '1.5';
                body.style.paddingTop = '0';
                body.style.paddingBottom = '0';
                var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                for (var i = 0; i < blocks.length; i++) {
                    blocks[i].style.marginBottom = '0.5em';
                    blocks[i].style.paddingBottom = '0';
                }
                body.style.opacity = '1';
                window._sgtFitting = false;
                return;
            }

            // ===== PHASE 0: RESET (Start TIGHT like GDI) =====
            body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
            body.style.letterSpacing = '0px';
            body.style.lineHeight = '1.15'; // Start tight like GDI
            body.style.paddingTop = '0';
            body.style.paddingBottom = '0';
            var resetBlocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
            for (var i = 0; i < resetBlocks.length; i++) {
                resetBlocks[i].style.marginBottom = '0.15em'; // Minimal paragraph gap
                resetBlocks[i].style.paddingBottom = '0';
            }
            clearLastMargin();

            // Force reflow after reset to ensure measurements are accurate
            void body.offsetHeight;

            var startSize = parseFloat(window.getComputedStyle(body).fontSize) || 14;

            // ===== PHASE 1: FONT SIZE (with tight line-height) =====
            // Binary search for largest font size that fits
            var low = minSize, high = maxSize, bestSize = minSize;
            var foundFittingSize = false;
            while (low <= high) {
                var mid = Math.floor((low + high) / 2);
                body.style.fontSize = mid + 'px';
                clearLastMargin();
                if (fits()) {
                    foundFittingSize = true;
                    bestSize = mid;
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
            if (!foundFittingSize) {
                bestSize = minSize;
            }
            body.style.fontSize = bestSize + 'px';
            clearLastMargin();
            // Small-window + less-text path: run a settle pass to avoid "almost right" first paint.
            if (isConstrainedShortContent) {
                void body.offsetHeight;
                var settleLow = minSize, settleHigh = bestSize, settleBest = minSize;
                while (settleLow <= settleHigh) {
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    clearLastMargin();
                    if (fits()) {
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    } else {
                        settleHigh = settleMid - 1;
                    }
                }
                body.style.fontSize = settleBest + 'px';
                clearLastMargin();
            }

            // ===== PHASE 1.5: CONDENSE OPTIMIZATION (wdth < 90) =====
            // Dense/tall text can get stuck at small font sizes because wrapping is width-limited.
            // Try narrower wdth values and re-fit font size; keep the combo that yields the largest
            // fitting font size. This is the path that enables true "condensed" behavior.
            // Skip when font is already at/near max — condensing can't exceed maxSize,
            // so the 7-width sweep (each with a full binary search) would be pure waste.
            // Still run when !foundFittingSize (overflow rescue needs condensing).
            if (textLen > 0 && (bestSize < maxSize - 2 || !foundFittingSize)) {
                var baseSize = parseFloat(body.style.fontSize) || bestSize;
                var bestComboSize = baseSize;
                var bestComboWdth = 90;
                var bestComboFits = fits();
                var bestComboOverflow = Math.max(0, doc.scrollHeight - winH);

                for (var testWdth = 85; testWdth >= 55; testWdth -= 5) {
                    body.style.fontVariationSettings = "'wght' 400, 'wdth' " + testWdth + ", 'slnt' 0, 'ROND' 100";
                    clearLastMargin();

                    var cLow = minSize, cHigh = maxSize, cBest = minSize;
                    var cFoundFit = false;
                    while (cLow <= cHigh) {
                        var cMid = Math.floor((cLow + cHigh) / 2);
                        body.style.fontSize = cMid + 'px';
                        clearLastMargin();
                        if (fits()) {
                            cFoundFit = true;
                            cBest = cMid;
                            cLow = cMid + 1;
                        } else {
                            cHigh = cMid - 1;
                        }
                    }
                    if (!cFoundFit) {
                        cBest = minSize;
                        body.style.fontSize = cBest + 'px';
                        clearLastMargin();
                    }
                    var cFits = fits();
                    var cOverflow = Math.max(0, doc.scrollHeight - winH);

                    // Selection policy:
                    // 1) Prefer any fitting candidate over non-fitting.
                    // 2) If both fit, prefer larger fitting font size.
                    // 3) If neither fits, prefer smaller overflow; tie-break to LESS condensing
                    //    (higher wdth) to avoid unnecessary right-side gap.
                    if (
                        (!bestComboFits && cFits)
                        || (bestComboFits && cFits && cBest > bestComboSize)
                        || (!bestComboFits && !cFits && (cOverflow < bestComboOverflow || (cOverflow === bestComboOverflow && testWdth > bestComboWdth)))
                    ) {
                        bestComboSize = cBest;
                        bestComboWdth = testWdth;
                        bestComboFits = cFits;
                        bestComboOverflow = cOverflow;
                    }
                }

                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestComboWdth + ", 'slnt' 0, 'ROND' 100";
                body.style.fontSize = bestComboSize + 'px';
                clearLastMargin();
            }

            // ===== PHASE 2: LINE HEIGHT (expand from tight baseline to fill gap) =====
            // Start from tight 1.15, expand up to 2.5 to fill remaining space
            if (fits() && getGap() > 2) {
                var lowLH = 1.15, highLH = 2.5, bestLH = 1.15;
                while (highLH - lowLH > 0.01) {
                    var midLH = (lowLH + highLH) / 2;
                    body.style.lineHeight = midLH;
                    clearLastMargin();
                    if (fits()) {
                        bestLH = midLH;
                        lowLH = midLH;
                    } else {
                        highLH = midLH;
                    }
                }
                body.style.lineHeight = bestLH;
                clearLastMargin();
            }

            // ===== PHASE 3: BLOCK MARGINS (Distribute space between paragraphs) =====
            if (fits() && getGap() > 2) {
                var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
                var numGaps = Math.max(1, blocks.length - 1);

                var lowM = 0, highM = 3.0, bestM = 0;
                while (highM - lowM > 0.02) {
                    var midM = (lowM + highM) / 2;
                    for (var i = 0; i < blocks.length - 1; i++) {
                        blocks[i].style.marginBottom = midM + 'em';
                    }
                    if (blocks.length > 0) blocks[blocks.length - 1].style.marginBottom = '0';
                    if (fits()) {
                        bestM = midM;
                        lowM = midM;
                    } else {
                        highM = midM;
                    }
                }
                for (var i = 0; i < blocks.length - 1; i++) {
                    blocks[i].style.marginBottom = bestM + 'em';
                }
                if (blocks.length > 0) blocks[blocks.length - 1].style.marginBottom = '0';
            }

            // ===== PHASE 4: FONT SIZE MICRO-ADJUST =====
            // Try bumping font size by 0.5px increments if there's still gap
            if (fits() && getGap() > 5) {
                var currentSize = parseFloat(body.style.fontSize) || bestSize;
                var testSize = currentSize;
                while (testSize < maxSize) {
                    testSize += 0.5;
                    body.style.fontSize = testSize + 'px';
                    clearLastMargin();
                    if (!fits()) {
                        body.style.fontSize = (testSize - 0.5) + 'px';
                        clearLastMargin();
                        break;
                    }
                }
            }

            // ===== PHASE 5: LETTER SPACING (Fine-tune horizontal density) =====
            // Expanding letter spacing can cause more wrapping, filling vertical space
            if (fits() && getGap() > 2 && isShortContent) {
                var lowLS = 0, highLS = 20, bestLS = 0;
                while (highLS - lowLS > 0.1) {
                    var midLS = (lowLS + highLS) / 2;
                    body.style.letterSpacing = midLS + 'px';
                    clearLastMargin();
                    if (fits()) {
                        bestLS = midLS;
                        lowLS = midLS;
                    } else {
                        highLS = midLS;
                    }
                }
                body.style.letterSpacing = bestLS + 'px';
                clearLastMargin();
            }

            // ===== PHASE 6: FONT WIDTH (wdth) =====
            // Expanding font width can also cause more wrapping
            if (fits() && getGap() > 2 && isShortContent) {
                var lowW = 90, highW = 150, bestW = 90;
                while (lowW <= highW) {
                    var midW = Math.floor((lowW + highW) / 2);
                    body.style.fontVariationSettings = "'wght' 400, 'wdth' " + midW + ", 'slnt' 0, 'ROND' 100";
                    clearLastMargin();
                    if (fits()) {
                        bestW = midW;
                        lowW = midW + 1;
                    } else {
                        highW = midW - 1;
                    }
                }
                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestW + ", 'slnt' 0, 'ROND' 100";
                clearLastMargin();
            }

            // ===== PHASE 7: HORIZONTAL FILL (for short/single-line content) =====
            // If content is only 1-2 lines tall, stretch to fill horizontal space
            var fontSize = parseFloat(body.style.fontSize) || 14;
            var lineH = parseFloat(body.style.lineHeight) || 1.5;
            var approxLineHeight = fontSize * lineH;
            var isFewLines = doc.scrollHeight < approxLineHeight * 3;

            if (fits() && isFewLines) {
                // For very short content, maximize wdth to fill horizontal space
                var lowW = 90, highW = 500, bestW = 90;
                var baseHeight = doc.scrollHeight;
                while (lowW <= highW) {
                    var midW = Math.floor((lowW + highW) / 2);
                    body.style.fontVariationSettings = "'wght' 400, 'wdth' " + midW + ", 'slnt' 0, 'ROND' 100";
                    // Only accept if height doesn't increase (no wrapping)
                    if (doc.scrollHeight <= baseHeight && fits()) {
                        bestW = midW;
                        lowW = midW + 1;
                    } else {
                        highW = midW - 1;
                    }
                }
                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + bestW + ", 'slnt' 0, 'ROND' 100";

                // Also stretch letter-spacing if more room
                baseHeight = doc.scrollHeight;
                var lowLS = 0, highLS = 100, bestLS = 0;
                while (highLS - lowLS > 0.5) {
                    var midLS = (lowLS + highLS) / 2;
                    body.style.letterSpacing = midLS + 'px';
                    if (doc.scrollHeight <= baseHeight && fits()) {
                        bestLS = midLS;
                        lowLS = midLS;
                    } else {
                        highLS = midLS;
                    }
                }
                body.style.letterSpacing = bestLS + 'px';
            }

            // ===== PHASE 8: OVERFLOW RESCUE CONDENSE =====
            // If content still overflows after all fill phases, prioritize eliminating overflow
            // by narrowing wdth at the current size (rather than keeping a wider wdth that wraps).
            if (!fits()) {
                var rescueSize = Math.max(minSize, parseFloat(body.style.fontSize) || minSize);
                body.style.fontSize = rescueSize + 'px';
                body.style.letterSpacing = '0px';
                clearLastMargin();

                var rescueBestWdth = 90;
                var rescueBestOverflow = Math.max(0, doc.scrollHeight - winH);
                for (var rescueWdth = 90; rescueWdth >= 45; rescueWdth -= 5) {
                    body.style.fontVariationSettings = "'wght' 400, 'wdth' " + rescueWdth + ", 'slnt' 0, 'ROND' 100";
                    clearLastMargin();
                    var rescueOverflow = Math.max(0, doc.scrollHeight - winH);
                    // Only condense further when it actually reduces overflow.
                    // On tie, prefer less condensing to keep adaptive width usage.
                    if (rescueOverflow < rescueBestOverflow || (rescueOverflow === rescueBestOverflow && rescueWdth > rescueBestWdth)) {
                        rescueBestOverflow = rescueOverflow;
                        rescueBestWdth = rescueWdth;
                    }
                }
                body.style.fontVariationSettings = "'wght' 400, 'wdth' " + rescueBestWdth + ", 'slnt' 0, 'ROND' 100";
                clearLastMargin();
            }

            // ===== FINAL: Fill any remaining gap by distributing space =====
            // After all optimizations, if there's still a gap, distribute it via body padding
            var finalGap = winH - doc.scrollHeight;
            if (finalGap > 2) {
                // Distribute gap: more at bottom, some at top for visual balance
                body.style.paddingTop = Math.floor(finalGap * 0.3) + 'px';
                body.style.paddingBottom = Math.floor(finalGap * 0.7) + 'px';
            } else {
                body.style.paddingTop = '0';
                body.style.paddingBottom = '0';
            }

            // Debug telemetry for runtime font-axis behavior and final fit result.
            // Disabled by default (enable with `window.__SGT_FIT_DEBUG__ = true` in devtools).
            try {
                if (window.__SGT_FIT_DEBUG__ === undefined) {
                    window.__SGT_FIT_DEBUG__ = false;
                }
                if (window.__SGT_FIT_DEBUG__ && window.ipc && typeof window.ipc.postMessage === 'function') {
                    var cs = window.getComputedStyle(body);
                    var probe = document.createElement('span');
                    probe.textContent = 'MMMMMMMMMMMMMMMMMMMM';
                    probe.style.position = 'absolute';
                    probe.style.visibility = 'hidden';
                    probe.style.pointerEvents = 'none';
                    probe.style.whiteSpace = 'nowrap';
                    probe.style.fontFamily = cs.fontFamily;
                    probe.style.fontSize = cs.fontSize;
                    probe.style.fontWeight = cs.fontWeight;
                    probe.style.lineHeight = cs.lineHeight;
                    document.body.appendChild(probe);
                    probe.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
                    var widthAt90 = probe.getBoundingClientRect().width;
                    probe.style.fontVariationSettings = "'wght' 400, 'wdth' 55, 'slnt' 0, 'ROND' 100";
                    var widthAt55 = probe.getBoundingClientRect().width;
                    if (probe.parentNode) probe.parentNode.removeChild(probe);

                    var payload = {
                        action: 'fit_debug',
                        phase: 'fit_font_to_window_final',
                        textLen: textLen,
                        winH: winH,
                        winW: winW,
                        scrollH: doc.scrollHeight,
                        finalGap: finalGap,
                        computedFontFamily: cs.fontFamily,
                        computedFontSize: cs.fontSize,
                        computedFontStretch: cs.fontStretch,
                        computedFontVariationSettings: cs.fontVariationSettings,
                        bodyStyleFontVariationSettings: body.style.fontVariationSettings || '',
                        letterSpacing: cs.letterSpacing,
                        lineHeight: cs.lineHeight,
                        googleSansFlexReady: (document.fonts && document.fonts.check)
                            ? document.fonts.check("16px 'Google Sans Flex'")
                            : null,
                        documentFontsStatus: (document.fonts && document.fonts.status) ? document.fonts.status : null,
                        probeWidthAtWdth90: widthAt90,
                        probeWidthAtWdth55: widthAt55,
                        probeWdthDelta: widthAt90 - widthAt55,
                        fitDurationMs: performance.now() - _fitStart,
                        fitCallCount: window._sgtFitCallCount || 0
                    };
                    window.ipc.postMessage(JSON.stringify(payload));
                }
            } catch (_err) {}

            // Reveal body (may have been hidden to prevent flash of unstyled content)
            body.style.opacity = '1';
            window._sgtFitting = false;
        }); }); }, 100);
    })();
    "#;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script(script);
        }
    });
}

/// Trigger Grid.js initialization on any tables in the WebView
/// Call this after streaming ends to convert tables to interactive Grid.js tables
pub fn init_gridjs(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            // Trigger the table initialization via the MutationObserver's mechanism
            // The observer watches for DOM changes and schedules initGridJs via window.gridJsTimeout
            // We can simulate this by triggering a DOM change or directly calling the init logic
            let script = r#"
                (function() {
                    if (typeof gridjs === 'undefined') return;

                    var tables = document.querySelectorAll('table:not(.gridjs-table):not([data-processed-table="true"])');
                    for (var i = 0; i < tables.length; i++) {
                        var table = tables[i];
                        if (table.closest('.gridjs-container') || table.closest('.gridjs-injected-wrapper')) continue;

                        table.setAttribute('data-processed-table', 'true');

                        var wrapper = document.createElement('div');
                        wrapper.className = 'gridjs-injected-wrapper';
                        table.parentNode.insertBefore(wrapper, table);

                        try {
                            var grid = new gridjs.Grid({
                                from: table,
                                sort: true,
                                fixedHeader: true,
                                search: false,
                                resizable: false,
                                autoWidth: false,
                                style: {
                                    table: { 'width': '100%' },
                                    td: { 'border': '1px solid #333' },
                                    th: { 'border': '1px solid #333' }
                                },
                                className: {
                                    table: 'gridjs-table-premium',
                                    th: 'gridjs-th-premium',
                                    td: 'gridjs-td-premium'
                                }
                            });
                            grid.on('ready', function() {
                                table.classList.add('gridjs-hidden-source');
                            });
                            grid.render(wrapper);
                        } catch (e) {
                            console.error('Grid.js streaming init error:', e);
                            if(wrapper.parentNode) wrapper.parentNode.removeChild(wrapper);
                        }
                    }
                })();
            "#;
            let _ = webview.evaluate_script(script);
        }
    });
}
