function installCustomVocabularyEditor(options) {
    const button = options.button;
    const modal = options.modal;
    const overlay = options.overlay;
    const input = options.input;
    const pills = options.pills;
    const pillClose = options.pillClose;
    const add = options.add;
    let current = Array.isArray(options.initialEntries) ? options.initialEntries.slice() : [];

    function normalize(value) {
        const seen = new Set();
        return value.split(/\r?\n/)
            .map(entry => entry.trim())
            .filter(entry => entry && !seen.has(entry) && seen.add(entry))
            .slice(0, 1000);
    }

    function close() {
        if (!modal.classList.contains('show')) return;
        modal.classList.remove('show');
        overlay.classList.remove('show');
        window.realtimePostMessage('textInputEnd');
    }

    function render() {
        pills.replaceChildren();
        current.forEach(function(entry, index) {
            const pill = document.createElement('span');
            pill.className = 'custom-vocabulary-pill';
            const label = document.createElement('span');
            label.textContent = entry;
            const remove = document.createElement('button');
            remove.type = 'button';
            remove.className = 'custom-vocabulary-pill-remove inline-svg-icon';
            remove.setAttribute('aria-label', entry);
            if (pillClose) remove.innerHTML = pillClose.innerHTML;
            pill.append(label, remove);
            remove.addEventListener('click', function() {
                current.splice(index, 1);
                render();
                options.onChange(current.slice());
                input.focus();
            });
            pills.appendChild(pill);
        });
        pills.scrollLeft = pills.scrollWidth;
    }

    function addTerms(value) {
        const next = normalize(current.concat(value.split(/[\r\n,]+/)).join('\n'));
        input.value = '';
        if (next.length === current.length && next.every((entry, index) => entry === current[index])) return;
        current = next;
        render();
        options.onChange(current.slice());
    }

    button.addEventListener('click', function(event) {
        event.stopPropagation();
        input.value = '';
        render();
        modal.classList.add('show');
        overlay.classList.add('show');
        window.realtimePostMessage('textInputStart');
        requestAnimationFrame(() => input.focus());
    });
    window.focusCustomVocabularyInput = function() { input.focus(); };
    overlay.addEventListener('click', close);
    if (add) add.addEventListener('click', function() { addTerms(input.value); });
    input.addEventListener('keydown', function(event) {
        if (event.key === 'Escape') {
            event.preventDefault();
            close();
        } else if (event.key === 'Enter' || event.key === ',') {
            event.preventDefault();
            addTerms(input.value);
        } else if (event.key === 'Backspace' && !input.value && current.length) {
            current.pop();
            render();
            options.onChange(current.slice());
        }
    });
    input.addEventListener('paste', function(event) {
        const pasted = event.clipboardData && event.clipboardData.getData('text');
        if (pasted && /[\r\n,]/.test(pasted)) {
            event.preventDefault();
            addTerms(pasted);
        }
    });
}
