//! CSS styles for the result compositor's control layer.

pub fn get_base_css() -> &'static str {
    r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: transparent;
    pointer-events: none;
    font-family: 'Google Sans Flex';
    user-select: none;
}

#button-container {
    position: fixed;
    inset: 0;
    z-index: 2147480000;
    pointer-events: none;
}

.button-group {
    --control-scale: 1;
    position: absolute;
    display: flex;
    gap: calc(4px * var(--control-scale));
    padding: calc(2px * var(--control-scale));
    pointer-events: auto;
    transition: opacity 0.15s ease-out;
}

.button-group.local-control-surface-light {
    --btn-bg: rgba(250, 250, 250, 0.94);
    --btn-border: rgba(0, 0, 0, 0.24);
    --btn-hover-bg: rgba(255, 255, 255, 0.98);
    --btn-active-bg: rgba(255, 255, 255, 0.98);
}

.button-group.local-control-surface-dark {
    --btn-bg: rgba(24, 24, 27, 0.92);
    --btn-border: rgba(255, 255, 255, 0.22);
    --btn-hover-bg: rgba(45, 45, 48, 0.98);
    --btn-active-bg: rgba(45, 45, 48, 0.98);
}

.btn {
    width: calc(24px * var(--control-scale));
    height: calc(24px * var(--control-scale));
    border-radius: calc(6px * var(--control-scale));
    background: var(--btn-bg);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border: 1px solid var(--btn-border);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: opacity 0.15s ease-out, background-color 0.15s ease-out, color 0.15s ease-out;
    color: var(--chain-control-color, var(--btn-color));
}
.btn svg {
    width: calc(16px * var(--control-scale));
    height: calc(16px * var(--control-scale));
}

.button-group.vertical {
    flex-direction: column;
    padding: calc(6px * var(--control-scale)) calc(3px * var(--control-scale));
    height: auto;
    width: calc(32px * var(--control-scale));
}
.button-group.vertical .btn {
    margin: calc(3px * var(--control-scale)) 0;
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

.btn.result-handle {
    cursor: grab;
}
.btn.result-handle:active {
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
    font-family: 'Google Sans Flex';
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

.model-badge {
    display: flex;
    align-items: center;
    height: calc(24px * var(--control-scale));
    max-width: calc(120px * var(--control-scale));
    padding: 0 calc(4px * var(--control-scale));
    background: none;
    border: none;
    cursor: default;
    pointer-events: none;
    user-select: none;
    font-size: calc(9px * var(--control-scale));
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--chain-control-color, var(--btn-color));
    opacity: 0.6;
}
.model-badge-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.button-group.vertical .model-badge {
    max-width: calc(30px * var(--control-scale));
    padding: calc(2px * var(--control-scale)) 0;
    height: auto;
    justify-content: center;
    font-size: calc(7px * var(--control-scale));
}

.opacity-btn-expandable {
    width: calc(24px * var(--control-scale));
    height: calc(24px * var(--control-scale));
    transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1), height 0.3s cubic-bezier(0.4, 0, 0.2, 1), background-color 0.15s, color 0.15s !important;
    overflow: hidden;
    padding: 0 calc(4px * var(--control-scale)) !important;
    display: flex !important;
    align-items: center;
    justify-content: flex-end !important;
    white-space: nowrap;
    border-radius: calc(6px * var(--control-scale));
}

.opacity-btn-expandable:not(.vertical-slider):hover {
    width: calc(110px * var(--control-scale)) !important;
    background: var(--btn-hover-bg) !important;
    transform: none !important;
}

.opacity-btn-expandable.vertical-slider {
    flex-direction: column !important;
    justify-content: flex-end !important;
    padding: calc(4px * var(--control-scale)) 0 !important;
}

.opacity-btn-expandable.vertical-slider:hover {
    height: calc(110px * var(--control-scale)) !important;
    background: var(--btn-hover-bg) !important;
    transform: none !important;
}

.opacity-icon-wrapper {
    width: calc(16px * var(--control-scale));
    min-width: calc(16px * var(--control-scale));
    height: calc(24px * var(--control-scale));
    display: flex;
    align-items: center;
    justify-content: center;
    order: 2;
    flex-shrink: 0;
}

.opacity-btn-expandable.vertical-slider .opacity-icon-wrapper {
    height: calc(16px * var(--control-scale));
    width: calc(24px * var(--control-scale));
}

.opacity-slider-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    gap: calc(4px * var(--control-scale));
    opacity: 0;
    transition: opacity 0.2s ease;
    pointer-events: none;
    order: 1;
    padding-right: calc(4px * var(--control-scale));
    min-width: 0;
}

.opacity-btn-expandable.vertical-slider .opacity-slider-wrapper {
    flex-direction: column;
    padding-right: 0;
    padding-bottom: calc(2px * var(--control-scale));
    gap: calc(2px * var(--control-scale));
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
    height: calc(3px * var(--control-scale));
    background: var(--btn-border);
    border-radius: 2px;
    cursor: grab;
    outline: none;
}

.opacity-slider-inline:active {
    cursor: grabbing;
}

.opacity-slider-inline::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: calc(12px * var(--control-scale));
    height: calc(12px * var(--control-scale));
    background: var(--btn-active-color);
    border-radius: 50%;
    cursor: grab;
    border: none;
}

.opacity-slider-inline:active::-webkit-slider-thumb {
    cursor: grabbing;
}

.opacity-btn-expandable.vertical-slider .opacity-slider-inline {
    -webkit-appearance: none;
    appearance: none;
    width: calc(3px * var(--control-scale)) !important;
    min-width: calc(3px * var(--control-scale)) !important;
    height: calc(55px * var(--control-scale)) !important;
    flex: none;
    margin: calc(5px * var(--control-scale)) auto;
    writing-mode: vertical-lr;
    direction: rtl;
}

.opacity-value-inline {
    font-size: calc(9px * var(--control-scale));
    color: var(--chain-control-color, var(--btn-color));
    min-width: calc(25px * var(--control-scale));
    text-align: center;
}
"#
}
