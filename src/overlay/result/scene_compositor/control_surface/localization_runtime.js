window.__SGT_SET_CONTROL_LOCALE__ = function(strings) {
    window.L10N = strings;
    const controls = document.getElementById('button-container');
    if (!controls) return;
    for (const element of controls.querySelectorAll('[data-l10n-title]')) {
        element.setAttribute('aria-label', strings[element.dataset.l10nTitle] || '');
    }
    for (const element of controls.querySelectorAll('[data-l10n-placeholder]')) {
        element.placeholder = strings[element.dataset.l10nPlaceholder] || '';
    }
    window.__SGT_CONTROL_TOOLTIP__?.refresh();
};
