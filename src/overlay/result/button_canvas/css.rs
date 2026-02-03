//! CSS styles for button canvas

pub fn get_base_css() -> &'static str {
    r#"
.icons {
    font-family: 'Material Symbols Rounded';
    font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 20;
    font-size: 16px;
    line-height: 1;
}

* { margin: 0; padding: 0; box-sizing: border-box; }
html, body {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: transparent;
    pointer-events: none;
    font-family: 'Google Sans Flex', 'Segoe UI', sans-serif;
    user-select: none;
}

.button-group {
    position: absolute;
    display: flex;
    gap: 4px;
    padding: 2px;
    pointer-events: auto;
    transition: opacity 0.15s ease-out;
}

.btn {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: var(--btn-bg);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border: 1px solid var(--btn-border);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: opacity 0.15s ease-out, background-color 0.15s ease-out, color 0.15s ease-out;
    color: var(--btn-color);
}

.button-group.vertical {
    flex-direction: column;
    padding: 6px 3px;
    height: auto;
    width: 32px;
}
.button-group.vertical .btn {
    margin: 3px 0;
}

.btn:hover {
    background: var(--btn-hover-bg);
    color: var(--btn-hover-color);
    transform: scale(1.05);
    box-shadow:
        -5px 0 6px -3px var(--shadow-color),
        5px 0 6px -3px var(--shadow-color),
        0 5px 6px -3px var(--shadow-color);
}

.btn:active {
    transform: scale(0.95);
}

.btn.disabled {
    opacity: 0.3;
    pointer-events: none;
}

.btn.active {
    background: var(--btn-active-bg);
    border-color: var(--btn-active-color);
    color: var(--btn-active-color);
}

.btn.success {
    background: var(--btn-active-bg);
    border-color: var(--btn-success-color);
    color: var(--btn-success-color);
}

.btn.loading {
    animation: pulse 1s infinite;
}

@keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
}

.btn.broom {
    cursor: grab;
}
.btn.broom:active {
    cursor: grabbing;
}

.btn.hidden {
    visibility: hidden;
    pointer-events: none;
}

.refine-bar {
    display: flex;
    align-items: center;
    background: var(--refine-bg);
    border: 1px solid var(--refine-border);
    border-radius: 8px;
    padding: 2px 4px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.15);
    pointer-events: auto;
    min-width: 250px;
    gap: 4px;
    animation: fadeIn 0.15s ease-out;
}

@keyframes fadeIn {
    from { opacity: 0; transform: scale(0.98); }
    to { opacity: 1; transform: scale(1); }
}

.refine-input {
    flex: 1;
    background: var(--refine-input-bg);
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 6px 10px;
    color: var(--refine-text);
    font-family: 'Google Sans Flex', sans-serif;
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
    min-width: 0;
}

.refine-input:focus {
    border-color: var(--btn-active-color);
}

.refine-input::placeholder {
    color: var(--refine-placeholder);
}

.refine-action-btn {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.15s;
    background: transparent;
    border: none;
    color: var(--mic-fill);
}

.refine-action-btn:hover {
    background: var(--mic-bg);
    transform: scale(1.05);
}

.refine-action-btn.send {
    color: var(--btn-active-color);
}

.opacity-btn-expandable {
    width: 24px;
    height: 24px;
    transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1), height 0.3s cubic-bezier(0.4, 0, 0.2, 1), background-color 0.15s, color 0.15s !important;
    overflow: hidden;
    padding: 0 4px !important;
    display: flex !important;
    align-items: center;
    justify-content: flex-end !important;
    white-space: nowrap;
    border-radius: 6px;
}

.opacity-btn-expandable:not(.vertical-slider):hover {
    width: 110px !important;
    background: var(--btn-hover-bg) !important;
    transform: none !important;
}

.opacity-btn-expandable.vertical-slider {
    flex-direction: column !important;
    justify-content: flex-end !important;
    padding: 4px 0 !important;
}

.opacity-btn-expandable.vertical-slider:hover {
    height: 110px !important;
    background: var(--btn-hover-bg) !important;
    transform: none !important;
}

.opacity-icon-wrapper {
    width: 16px;
    min-width: 16px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    order: 2;
    flex-shrink: 0;
}

.opacity-btn-expandable.vertical-slider .opacity-icon-wrapper {
    height: 16px;
    width: 24px;
}

.opacity-slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.2s ease;
    pointer-events: none;
    order: 1;
    padding-right: 4px;
    min-width: 0;
}

.opacity-btn-expandable.vertical-slider .opacity-slider-wrapper {
    flex-direction: column;
    padding-right: 0;
    padding-bottom: 2px;
    gap: 2px;
    justify-content: center;
}

.opacity-btn-expandable:hover .opacity-slider-wrapper {
    opacity: 1;
    pointer-events: auto;
    transition: opacity 0.3s ease 0.1s;
}

.opacity-slider-inline {
    -webkit-appearance: none;
    appearance: none;
    flex: 1;
    min-width: 0;
    height: 3px;
    background: var(--btn-border);
    border-radius: 2px;
    cursor: pointer;
    outline: none;
}

.opacity-slider-inline::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    background: var(--btn-active-color);
    border-radius: 50%;
    cursor: pointer;
    border: none;
}

.opacity-btn-expandable.vertical-slider .opacity-slider-inline {
    -webkit-appearance: none;
    appearance: none;
    width: 3px !important;
    min-width: 3px !important;
    height: 55px !important;
    flex: none;
    margin: 5px auto;
    writing-mode: vertical-lr;
    direction: rtl;
}

.opacity-value-inline {
    font-size: 9px;
    color: var(--btn-color);
    min-width: 25px;
    text-align: center;
}
"#
}
