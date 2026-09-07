(function() {
    function createRuntime(body, reveal) {
        function update(animate, isNewSession) {
            var words = body.querySelectorAll('.word');
            if (reveal.lastRevealedIndex >= words.length) reveal.lastRevealedIndex = -1;
            var generation = reveal.generation;
            var wasActive = reveal.active;
            reveal.queue = [];
            if (isNewSession || !animate) {
                destroy();
                for (var word of words) {
                    word.style.visibility = 'visible';
                    word.style.opacity = '1';
                }
                reveal.lastRevealedIndex = words.length - 1;
                return;
            }
            var start = Math.max(0, reveal.lastRevealedIndex + 1);
            var maximumLag = 80;
            if (words.length - start > maximumLag) {
                for (var skipped = start; skipped < words.length - maximumLag; skipped++) {
                    words[skipped].style.visibility = 'visible';
                    words[skipped].style.opacity = '1';
                }
                start = words.length - maximumLag;
                reveal.lastRevealedIndex = start - 1;
            }
            for (var index = start; index < words.length; index++) {
                var word = words[index];
                word.style.visibility = 'hidden';
                word.style.opacity = '0';
                word.style.filter = '';
                word.style.transition = 'opacity 0.22s ease-out';
                reveal.queue.push({ element: word, index: index });
            }
            if (!reveal.queue.length) return;
            // Retarget the existing frame loop without resetting elapsed time or credits.
            if (wasActive) return;
            reveal.active = true;
            reveal.lastTick = performance.now();
            reveal.credits = 1;
            var tick = function(now) {
                if (generation !== reveal.generation) return;
                var elapsed = Math.max(0, now - reveal.lastTick);
                reveal.lastTick = now;
                var wordsPerSecond = 40 * (1 + reveal.queue.length / 10);
                reveal.credits += wordsPerSecond * elapsed / 1000;
                var emitted = 0;
                while (reveal.credits >= 1 && reveal.queue.length && emitted < 64) {
                    var item = reveal.queue.shift();
                    if (item.element.isConnected) {
                        item.element.style.visibility = 'visible';
                        item.element.style.opacity = '1';
                    }
                    reveal.lastRevealedIndex = item.index;
                    reveal.credits -= 1;
                    emitted++;
                }
                if (reveal.queue.length) requestAnimationFrame(tick);
                else reveal.active = false;
            };
            requestAnimationFrame(tick);
        }

        function destroy() {
            reveal.generation++;
            reveal.queue = [];
            reveal.active = false;
        }

        return { update: update, destroy: destroy };
    }

    window.__SGT_CREATE_REVEAL_RUNTIME__ = createRuntime;
})();
