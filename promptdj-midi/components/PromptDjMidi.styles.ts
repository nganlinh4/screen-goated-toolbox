import { css } from 'lit';

export const promptDjMidiStyles = css`
  :host {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    box-sizing: border-box;
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
    font-variation-settings: 'ROND' 100;
    container-type: size;
    container-name: dj-host;
  }
  button { font-family: inherit; }

  #background {
    will-change: background-image;
    position: absolute;
    height: 100%;
    width: 100%;
    z-index: -1;
    background: var(--md-surface);
  }

  /* Main layout */
  #content {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: clamp(8px, 3cqi, 40px);
    position: relative;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
  }
  @media (orientation: portrait) {
    #content {
      flex-direction: column;
      justify-content: space-evenly;
      overflow: hidden;
      padding: 2% 6%;
      height: 100%;
    }
  }
  @media (orientation: landscape) {
    #content {
      flex-direction: row;
      overflow: hidden;
      padding: 4vh 8%;
    }
  }

  /* Grid wrapper */
  #gridWrap {
    display: flex;
    align-items: center;
    justify-content: center;
    container-type: inline-size;
    container-name: grid-wrap;
  }
  @media (orientation: portrait) {
    #gridWrap { width: 100%; flex: 1; }
  }
  @media (orientation: landscape) {
    #gridWrap { flex: 1; height: 100%; }
  }

  /* Grid */
  #grid {
    width: 100%;
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 12vh clamp(4px, 1cqi, 12px);
  }
  @media (orientation: portrait) {
    #grid {
      grid-template-columns: repeat(3, 1fr);
      row-gap: 40px;
      column-gap: 6px;
      flex: 1;
    }
    prompt-controller {
      --knob-scale: 45%;
      --text-max: 11px;
    }
  }
  @media (orientation: portrait) and (min-width: 600px) {
    #grid {
      grid-template-columns: repeat(4, 1fr);
      row-gap: 60px;
    }
    prompt-controller {
      --knob-scale: 38%;
    }
  }
  @media (orientation: landscape) {
    @container grid-wrap (min-width: 700px) {
      #grid { grid-template-columns: repeat(8, 1fr); }
    }
  }

  .add-slot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    cursor: pointer;
    width: 100%;
    height: 100%;
  }
  .add-slot .add-icon {
    width: 60%;
    height: 60%;
    color: #fff;
    filter: drop-shadow(0 12px 22px rgba(0,0,0,0.25)) drop-shadow(0 4px 10px rgba(0,0,0,0.18));
    transition: transform var(--md-duration-short3) var(--md-easing-emphasized);
  }
  :host([data-theme="light"]) .add-slot .add-icon { color: #fff; }
  .add-slot:hover .add-icon { transform: scale(1.05); }

  /* Clear button on knobs */
  .pc-wrap { position: relative; overflow: visible; }
  .pc-clear {
    position: absolute;
    top: -6px;
    right: -6px;
    width: clamp(16px, 3cqi, 28px);
    height: clamp(16px, 3cqi, 28px);
    border-radius: 9999px;
    border: 1px solid var(--md-outline-variant);
    background: var(--md-surface);
    color: var(--md-on-surface);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    line-height: 0;
    box-sizing: border-box;
    cursor: pointer;
    box-shadow: var(--md-elevation-level1);
    opacity: 0;
    z-index: 20;
    pointer-events: auto;
    transition: opacity var(--md-duration-short3) var(--md-easing-standard),
                transform var(--md-duration-short3) var(--md-easing-standard),
                box-shadow var(--md-duration-short3) var(--md-easing-standard),
                background-color var(--md-duration-short3) var(--md-easing-standard),
                border-color var(--md-duration-short3) var(--md-easing-standard);
    transform: scale(0.9);
  }
  .pc-clear svg { width: 100%; height: 100%; display: block; }
  .pc-wrap:hover .pc-clear { opacity: 1; transform: scale(1); }
  .pc-clear:hover {
    background: var(--md-surface-variant);
    border-color: var(--md-outline);
    box-shadow: var(--md-elevation-level2);
    transform: scale(1.06);
  }
  .pc-clear:active { transform: scale(0.96); box-shadow: var(--md-elevation-level1); }
  .pc-clear:focus-visible { outline: none; box-shadow: 0 0 0 2px rgba(0,0,0,0.3), var(--md-elevation-level2); }

  /* Modal */
  .modal-overlay {
    position: fixed;
    top: 0; left: 0; width: 100%; height: 100%;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(5px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fadeIn 0.3s ease;
  }
  .modal-content {
    background: var(--md-surface, #222);
    padding: 16px;
    border-radius: 12px;
    box-shadow: 0 10px 30px rgba(0,0,0,0.5);
    border: 1px solid rgba(255,255,255,0.1);
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 240px;
    max-width: 90%;
    animation: slideIn 0.3s ease;
  }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes slideIn { from { transform: translateY(20px); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: clamp(12px, 1.8cqi, 18px);
    font-weight: bold;
    color: var(--md-on-surface, #fff);
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
  }
  .close-modal {
    background: transparent;
    border: none;
    color: rgba(255,255,255,0.6);
    cursor: pointer;
    font-size: clamp(16px, 2.5cqi, 24px);
    line-height: 1;
    padding: 0;
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
  }
  .close-modal:hover { color: #fff; }
  .audio-player { width: 100%; height: 36px; border-radius: 999px; margin-top: 4px; }
  .download-btn {
    background: var(--md-primary, #6200ea);
    color: var(--md-on-primary, #fff);
    border: none;
    padding: 8px 16px;
    border-radius: 20px;
    font-size: clamp(11px, 1.5cqi, 14px);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-weight: 500;
    font-family: inherit;
    transition: background 0.2s;
  }
  .download-btn:hover { filter: brightness(1.2); }

  /* Side controls */
  #sideControls {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  @media (orientation: portrait) {
    #sideControls {
      flex-direction: row;
      width: 100%;
      padding: 16px 6%;
      gap: 12px;
      justify-content: center;
      align-items: center;
    }
    .volume-container {
      flex: 1;
      order: 2;
      margin: 0;
      max-width: 140px;
      transform: translateY(7px);
    }
    play-pause-morph { order: 1; }
    .mini-controls { order: 3; margin-top: 0; margin-left: 0; }
  }
  @media (orientation: landscape) {
    #sideControls {
      flex-direction: column;
      height: 100%;
      width: auto;
      min-width: 80px;
      max-width: 140px;
    }
  }

  play-pause-morph {
    display: inline-block;
    width: clamp(48px, 15cqi, 140px);
    height: clamp(48px, 15cqi, 140px);
  }
  @media (orientation: portrait) {
    play-pause-morph {
      width: clamp(64px, 18vw, 120px);
      height: clamp(64px, 18vw, 120px);
    }
  }

  .mini-controls {
    display: flex;
    gap: clamp(4px, 1cqi, 10px);
    margin-top: clamp(4px, 1.5cqi, 20px);
  }
  @media (orientation: portrait) {
    .mini-controls {
      margin-top: 0;
      margin-left: auto;
      gap: clamp(8px, 4vw, 20px);
    }
    .mini-btn {
      width: clamp(36px, 8vw, 56px);
      height: clamp(36px, 8vw, 56px);
    }
    .mini-btn svg {
      width: clamp(18px, 5vw, 32px);
      height: clamp(18px, 5vw, 32px);
    }
  }

  .mini-btn {
    width: clamp(28px, 5cqi, 48px);
    height: clamp(28px, 5cqi, 48px);
    background: transparent;
    border: none;
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .mini-btn:hover { transform: scale(1.15); }
  .mini-btn.active { color: #ff3c3c; filter: drop-shadow(0 0 6px #ff3c3c); }
  .mini-btn.toggled { color: var(--md-primary); filter: drop-shadow(0 0 6px var(--md-primary)); }

  .mini-btn svg, .pc-clear svg, .add-icon svg, .volume-icon svg, .download-btn svg {
    display: inline-block;
    filter: drop-shadow(0 2px 4px rgba(0,0,0,0.5));
  }
  :host([data-theme="light"]) .mini-btn svg,
  :host([data-theme="light"]) .pc-clear svg,
  :host([data-theme="light"]) .add-icon svg {
    filter: drop-shadow(0 1px 2px rgba(0,0,0,0.3));
  }
  .mini-btn svg { width: clamp(14px, 2.5cqi, 24px); height: clamp(14px, 2.5cqi, 24px); }
  .pc-clear svg { width: clamp(10px, 2cqi, 18px); height: clamp(10px, 2cqi, 18px); }
  .add-icon svg { width: clamp(24px, 8cqi, 64px); height: clamp(24px, 8cqi, 64px); }
  .volume-icon svg { width: clamp(14px, 2cqi, 20px); height: clamp(14px, 2cqi, 20px); }
  .download-btn svg { width: 16px; height: 16px; }

  .rec-timer-container {
    min-height: 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    font-variant-numeric: tabular-nums;
    margin-top: 4px;
    pointer-events: none;
  }
  .rec-timer-elapsed {
    font-size: 1.15em;
    font-weight: 700;
    color: var(--accent-color, #ff4444);
    line-height: 1;
    font-stretch: 125%;
    font-variation-settings: 'wdth' 125;
  }
  .rec-timer-audio { font-size: 0.75em; opacity: 0.7; margin-top: 2px; }

  .volume-container {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    margin-bottom: 16px;
    box-sizing: border-box;
    gap: 6px;
    opacity: 1;
    transition: opacity 0.3s ease;
  }
  @media (hover: hover) {
    .volume-container { opacity: 0; }
    #sideControls:hover .volume-container { opacity: 1; }
  }
  .volume-icon { color: #fff; display: flex; align-items: center; }
  .volume-icon svg { width: 18px; height: 18px; }
  .volume-slider {
    flex: 1;
    -webkit-appearance: none;
    height: 3px;
    border-radius: 4px;
    outline: none;
    cursor: pointer;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    max-width: 120px;
  }
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ffffff;
    cursor: pointer;
    transition: transform 0.1s;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  }
  .volume-slider::-webkit-slider-thumb:hover { transform: scale(1.2); }
`;
