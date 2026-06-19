import "./styles.css";
import type { HotkeyItem, TranslationGummyState } from "./types";
import { renderTranscripts, updateTranscriptScrollAffinity } from "./transcripts";

const BAR_WIDTH = 8;
const BAR_GAP = 6;
const BAR_SPACING = BAR_WIDTH + BAR_GAP;
const VISIBLE_BARS = 20;

const COLORS_DARK: Record<string, string[]> = {
  ready:        ['#a92a44', '#ff7387', '#ffb3bc'],
  reconnecting: ['#f9cc61', '#eabe55', '#ffd78f'],
  error:        ['#ff716c', '#ff9993', '#ffb3ba'],
  stopped:      ['#45475a', '#585b70', '#6c7086'],
  not_configured: ['#45475a', '#585b70', '#6c7086'],
  connecting:   ['#864958', '#ff7387', '#ffc2cd'],
};

const COLORS_LIGHT: Record<string, string[]> = {
  ready:        ['#a92a44', '#ff7387', '#ffb3bc'],
  reconnecting: ['#f59f00', '#f9cc61', '#ffe066'],
  error:        ['#b31b25', '#fb5151', '#ff9993'],
  stopped:      ['#b3abad', '#7c7577', '#605a5c'],
  not_configured: ['#b3abad', '#7c7577', '#605a5c'],
  connecting:   ['#864958', '#ff7387', '#ffc2cd'],
};

const state = {
  payload: window.__TG_INITIAL_STATE__ ?? null,
  hotkeyCaptureArmed: false,
  transcriptPinned: true,
  visualLevel: 0,
  barHeights: new Array(VISIBLE_BARS + 2).fill(6) as number[],
  scrollProgress: 0,
  lastTime: 0,
  lastTranscriptKey: "__init__",
  firstDetectedLang: "",
  dominantLang: "",       // the reliably detected ISO 639-3 code (e.g. "kor")
  dominantIsLeft: true,   // which side dominant goes to
  lastHotkeyKey: "",
  guideShown: false,
};

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("App root not found");

app.innerHTML = `
  <div class="app-shell">
    <header class="titlebar" id="dragRegion">
      <div class="header-waveform">
        <canvas class="header-canvas" id="visualizerCanvas"></canvas>
      </div>
      <div class="settings-btn-wrap" id="settingsWrap">
        <button class="icon-button" id="settingsBtn" type="button" aria-label="TTS Settings"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="m9.25 22l-.4-3.2q-.325-.125-.612-.3t-.563-.375L4.7 19.375l-2.75-4.75l2.575-1.95Q4.5 12.5 4.5 12.338v-.675q0-.163.025-.338L1.95 9.375l2.75-4.75l2.975 1.25q.275-.2.575-.375t.6-.3l.4-3.2h5.5l.4 3.2q.325.125.613.3t.562.375l2.975-1.25l2.75 4.75l-2.575 1.95q.025.175.025.338v.674q0 .163-.05.338l2.575 1.95l-2.75 4.75l-2.95-1.25q-.275.2-.575.375t-.6.3l-.4 3.2zm2.8-6.5q1.45 0 2.475-1.025T15.55 12t-1.025-2.475T12.05 8.5q-1.475 0-2.488 1.025T8.55 12t1.013 2.475T12.05 15.5"/></svg></button>
      </div>
      <div class="volume-btn-wrap" id="volumeWrap">
        <button class="icon-button" id="volumeBtn" type="button" aria-label="Volume"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M14 20.725v-2.05q2.25-.65 3.625-2.5t1.375-4.2t-1.375-4.2T14 5.275v-2.05q3.1.7 5.05 3.138T21 11.975t-1.95 5.613T14 20.725M3 15V9h4l5-5v16l-5-5zm11 1V7.95q1.175.55 1.838 1.65T16.5 12q0 1.275-.663 2.363T14 16"/></svg></button>
        <div class="volume-popup" id="volumePopup">
          <input type="range" class="volume-slider" id="volumeSlider" min="0" max="100" value="100" step="5">
          <span class="volume-label" id="volumeLabel">100%</span>
        </div>
      </div>
      <div class="status-pill">
        <span class="status-dot" id="statusDot"></span>
        <span id="statusText"></span>
      </div>
      <div class="header-hotkey" id="hotkeyArea"></div>
      <div class="window-actions">
        <button class="icon-button" id="minimizeBtn" type="button" aria-label="Minimize"><svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M5 13v-2h14v2z"/></svg></button>
        <button class="icon-button" id="closeBtn" type="button" aria-label="Close"><svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z"/></svg></button>
      </div>
    </header>

    <section class="body-grid">
      <aside class="rail">
        <section class="card lang-card">
          <div class="lang-top-row">
            <div class="card-title" id="firstTitle"></div>
            <input class="text-input" id="firstLanguage" />
          </div>
          <div class="lang-bottom-row">
            <input class="text-input" id="firstAccent" />
            <input class="text-input" id="firstTone" />
          </div>
        </section>

        <section class="card lang-card">
          <div class="lang-top-row">
            <div class="card-title" id="secondTitle"></div>
            <input class="text-input" id="secondLanguage" />
          </div>
          <div class="lang-bottom-row">
            <input class="text-input" id="secondAccent" />
            <input class="text-input" id="secondTone" />
          </div>
        </section>

        <section class="card">
          <div class="actions-row">
            <button class="button primary" id="applyBtn" type="button"></button>
            <button class="button secondary" id="toggleBtn" type="button"></button>
          </div>
          <div class="message" id="messageText"></div>
        </section>
      </aside>

      <section class="transcript-card">
        <div class="transcript-curtain">
          <div class="transcript-stage">
            <div class="message" id="transcriptMessage"></div>
            <div class="transcript-body" id="transcriptBody"></div>
          </div>
        </div>
      </section>
    </section>
    <div class="guide-overlay" id="guideOverlay" style="display:none">
      <div class="guide-popup">
        <div class="guide-title">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M11 17h2v-6h-2zm1.713-8.287Q13 8.425 13 8t-.288-.712T12 7t-.712.288T11 8t.288.713T12 9t.713-.288M12 22q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22"/></svg>
          <span id="guideTitle"></span>
        </div>
        <p class="guide-msg" id="guideMsg"></p>
        <button class="guide-btn" id="guideBtn" type="button"></button>
      </div>
    </div>
  </div>
`;

const el = {
  root: document.documentElement,
  dragRegion: document.querySelector<HTMLElement>("#dragRegion")!,
  statusDot: document.querySelector<HTMLElement>("#statusDot")!,
  statusText: document.querySelector<HTMLElement>("#statusText")!,
  hotkeyArea: document.querySelector<HTMLElement>("#hotkeyArea")!,
  firstTitle: document.querySelector<HTMLElement>("#firstTitle")!,
  secondTitle: document.querySelector<HTMLElement>("#secondTitle")!,
  firstLanguage: document.querySelector<HTMLInputElement>("#firstLanguage")!,
  firstAccent: document.querySelector<HTMLInputElement>("#firstAccent")!,
  firstTone: document.querySelector<HTMLInputElement>("#firstTone")!,
  secondLanguage: document.querySelector<HTMLInputElement>("#secondLanguage")!,
  secondAccent: document.querySelector<HTMLInputElement>("#secondAccent")!,
  secondTone: document.querySelector<HTMLInputElement>("#secondTone")!,
  applyBtn: document.querySelector<HTMLButtonElement>("#applyBtn")!,
  toggleBtn: document.querySelector<HTMLButtonElement>("#toggleBtn")!,
  minimizeBtn: document.querySelector<HTMLButtonElement>("#minimizeBtn")!,
  closeBtn: document.querySelector<HTMLButtonElement>("#closeBtn")!,
  messageText: document.querySelector<HTMLElement>("#messageText")!,
  transcriptMessage: document.querySelector<HTMLElement>("#transcriptMessage")!,
  transcriptBody: document.querySelector<HTMLElement>("#transcriptBody")!,
  visualizerCanvas: document.querySelector<HTMLCanvasElement>("#visualizerCanvas")!,
};

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  if (window.invoke) return window.invoke(cmd, args);
  return Promise.resolve(null);
}

function bindDraftInput(element: HTMLInputElement, profile: "first" | "second", field: "language" | "accent" | "tone") {
  element.addEventListener("input", () => {
    void invoke("set_draft", { profile, field, value: element.value });
  });
}

function hotkeyKey(hotkeys: HotkeyItem[]): string {
  return hotkeys.map((h) => `${h.code}:${h.modifiers}`).join(",");
}

function renderHotkeys(payload: TranslationGummyState) {
  const armed = state.hotkeyCaptureArmed ? "1" : "0";
  const key = hotkeyKey(payload.hotkeys ?? []) + "|" + armed + "|" + payload.strings.setHotkey;
  if (key === state.lastHotkeyKey) return;
  state.lastHotkeyKey = key;

  const hotkeys = payload.hotkeys ?? [];
  let html = "";

  // Hotkey badges with X
  for (let i = 0; i < hotkeys.length; i++) {
    html += `<button class="hotkey-badge" data-remove="${i}" type="button">${escapeHtml(hotkeys[i].name)}<svg class="badge-x" width="10" height="10" viewBox="0 0 24 24" fill="currentColor"><path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z"/></svg></button>`;
  }

  // Capture state or Add button
  if (state.hotkeyCaptureArmed) {
    html += `<span class="hotkey-capture">...</span><button class="hotkey-cancel" id="cancelHotkeyBtn" type="button"><svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor"><path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z"/></svg></button>`;
  } else {
    html += `<button class="text-btn" id="addHotkeyBtn" type="button">${escapeHtml(payload.strings.setHotkey)}</button>`;
  }

  el.hotkeyArea.innerHTML = html;

  // Bind events
  el.hotkeyArea.querySelectorAll<HTMLButtonElement>("[data-remove]").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const idx = parseInt(btn.dataset.remove!, 10);
      void invoke("remove_hotkey", { index: idx });
    });
  });

  const addBtn = el.hotkeyArea.querySelector<HTMLButtonElement>("#addHotkeyBtn");
  if (addBtn) {
    addBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      state.hotkeyCaptureArmed = true;
      if (state.payload) renderHotkeys(state.payload);
    });
  }

  const cancelBtn = el.hotkeyArea.querySelector<HTMLButtonElement>("#cancelHotkeyBtn");
  if (cancelBtn) {
    cancelBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      state.hotkeyCaptureArmed = false;
      if (state.payload) renderHotkeys(state.payload);
    });
  }
}

function connectionClass(cs: string) {
  if (cs === "ready") return "ready";
  if (cs === "reconnecting") return "reconnecting";
  if (cs === "error") return "error";
  return "";
}

// Helper: only touch DOM if value changed
function setText(el: HTMLElement, v: string) { if (el.textContent !== v) el.textContent = v; }
function setAttr(el: HTMLElement, k: string, v: string) { if (el.getAttribute(k) !== v) el.setAttribute(k, v); }

let lastRenderedLang = "";

function render(payload: TranslationGummyState) {
  state.payload = payload;
  el.root.dataset.theme = payload.darkMode ? "dark" : "light";

  // Static strings — only update on language change
  const lang = payload.darkMode + payload.strings.title;
  if (lang !== lastRenderedLang) {
    lastRenderedLang = lang;
    setText(el.firstTitle, payload.strings.firstProfile);
    setText(el.secondTitle, payload.strings.secondProfile);
    el.firstLanguage.placeholder = payload.strings.languageLabel;
    el.firstAccent.placeholder = payload.strings.accentLabel;
    el.firstTone.placeholder = payload.strings.toneLabel;
    el.secondLanguage.placeholder = payload.strings.languageLabel;
    el.secondAccent.placeholder = payload.strings.accentLabel;
    el.secondTone.placeholder = payload.strings.toneLabel;
    setText(el.applyBtn, payload.strings.apply);
  }

  setText(el.statusText, payload.statusLabel);
  setAttr(el.statusDot, "class", `status-dot ${connectionClass(payload.connectionState)}`.trim());

  const toggleLabel = payload.isRunning ? payload.strings.stop : payload.strings.start;
  setText(el.toggleBtn, toggleLabel);

  const errorText = payload.hotkeyError || payload.lastError || "";
  setText(el.messageText, errorText);
  setText(el.transcriptMessage, errorText);

  el.applyBtn.hidden = !payload.dirty;
  el.applyBtn.disabled = !payload.dirty || !payload.canApply;
  el.toggleBtn.disabled = !(payload.isRunning || payload.canToggle);

  if (document.activeElement !== el.firstLanguage) el.firstLanguage.value = payload.draft.first.language ?? "";
  if (document.activeElement !== el.firstAccent) el.firstAccent.value = payload.draft.first.accent ?? "";
  if (document.activeElement !== el.firstTone) el.firstTone.value = payload.draft.first.tone ?? "";
  if (document.activeElement !== el.secondLanguage) el.secondLanguage.value = payload.draft.second.language ?? "";
  if (document.activeElement !== el.secondAccent) el.secondAccent.value = payload.draft.second.accent ?? "";
  if (document.activeElement !== el.secondTone) el.secondTone.value = payload.draft.second.tone ?? "";

  // Guide dialog on first visit (once per session)
  if (!payload.guideSeen && !state.guideShown) {
    state.guideShown = true;
    document.querySelector<HTMLElement>("#guideTitle")!.textContent = payload.strings.title;
    document.querySelector<HTMLElement>("#guideMsg")!.textContent = payload.strings.guide;
    guideBtn.textContent = payload.strings.guideOk;
    guideOverlay.style.display = "";
  }

  renderHotkeys(payload);
  renderTranscripts(el.transcriptBody, payload, state);
}

/* ── Waveform visualizer ── */

function drawVisualizer(timestamp: number) {
  const canvas = el.visualizerCanvas;
  const ctx = canvas.getContext("2d");
  if (!ctx) { requestAnimationFrame(drawVisualizer); return; }

  if (canvas.width !== 200 || canvas.height !== 52) {
    canvas.width = 200;
    canvas.height = 52;
  }

  const payload = state.payload;
  const isActive = payload?.isRunning ?? false;
  const connectionState = payload?.connectionState ?? "stopped";
  const isDark = payload?.darkMode ?? true;
  const rms = isActive ? (payload?.audioLevel ?? 0) : 0;

  const dt = state.lastTime ? (timestamp - state.lastTime) / 1000 : 0.016;
  state.lastTime = timestamp;
  state.scrollProgress += dt / 0.15;

  while (state.scrollProgress >= 1) {
    state.scrollProgress -= 1;
    state.barHeights.shift();
    let displayRMS = rms;
    if (!isActive) displayRMS = 0.02;
    else if (connectionState === "connecting") displayRMS = 0.08 + 0.12 * Math.abs(Math.sin(timestamp / 300));
    else if (connectionState === "reconnecting") displayRMS = 0.06 + 0.1 * Math.abs(Math.sin(timestamp / 250));
    const h = canvas.height;
    state.barHeights.push(Math.max(6, Math.min(h - 4, displayRMS * 250 + 6)));
  }

  const w = canvas.width, h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const pixelOffset = state.scrollProgress * BAR_SPACING;
  const colorSet = isDark ? COLORS_DARK : COLORS_LIGHT;
  const currentColors = colorSet[connectionState] ?? colorSet.stopped;

  const grad = ctx.createLinearGradient(0, h, 0, 0);
  grad.addColorStop(0, currentColors[0]);
  grad.addColorStop(0.5, currentColors[1]);
  grad.addColorStop(1, currentColors[2]);
  ctx.fillStyle = grad;

  for (let i = 0; i < state.barHeights.length; i++) {
    const pillHeight = state.barHeights[i];
    const x = i * BAR_SPACING - pixelOffset;
    const y = (h - pillHeight) / 2;
    if (x > -BAR_WIDTH && x < w) {
      ctx.beginPath();
      if (ctx.roundRect) ctx.roundRect(x, y, BAR_WIDTH, pillHeight, BAR_WIDTH / 2);
      else ctx.rect(x, y, BAR_WIDTH, pillHeight);
      ctx.fill();
    }
  }

  const fadeWidth = 30;
  ctx.save();
  ctx.globalCompositeOperation = "destination-out";
  const leftGrad = ctx.createLinearGradient(0, 0, fadeWidth, 0);
  leftGrad.addColorStop(0, "rgba(0,0,0,1)");
  leftGrad.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = leftGrad;
  ctx.fillRect(0, 0, fadeWidth, h);
  const rightGrad = ctx.createLinearGradient(w - fadeWidth, 0, w, 0);
  rightGrad.addColorStop(0, "rgba(0,0,0,0)");
  rightGrad.addColorStop(1, "rgba(0,0,0,1)");
  ctx.fillStyle = rightGrad;
  ctx.fillRect(w - fadeWidth, 0, fadeWidth, h);
  ctx.restore();

  requestAnimationFrame(drawVisualizer);
}

/* ── Events ── */

el.dragRegion.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("button, .hotkey-badge, .text-btn, .hotkey-capture, .volume-popup, input")) return;
  void invoke("drag_window");
});

const settingsBtn = document.querySelector<HTMLButtonElement>("#settingsBtn")!;
const settingsWrap = document.querySelector<HTMLElement>("#settingsWrap")!;
settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  void invoke("open_tts_settings");
});

// Custom hover popup for settings
let settingsPopup: HTMLElement | null = null;
settingsWrap.addEventListener("mouseenter", () => {
  const p = state.payload;
  if (!p || settingsPopup) return;
  settingsPopup = document.createElement("div");
  settingsPopup.className = "settings-popup";
  settingsPopup.innerHTML = `<div class="settings-popup-line">${escapeHtml(p.strings.currentModel)}: <strong>${escapeHtml(p.ttsModel)}</strong></div><div class="settings-popup-line">${escapeHtml(p.strings.currentVoice)}: <strong>${escapeHtml(p.ttsVoice)}</strong></div>`;
  settingsWrap.appendChild(settingsPopup);
});
settingsWrap.addEventListener("mouseleave", () => {
  if (settingsPopup) { settingsPopup.remove(); settingsPopup = null; }
});

// Volume slider with drag-safe popup + mute toggle
const volumeWrap = document.querySelector<HTMLElement>("#volumeWrap")!;
const volumeBtn = document.querySelector<HTMLButtonElement>("#volumeBtn")!;
const volumeSlider = document.querySelector<HTMLInputElement>("#volumeSlider")!;
const volumeLabel = document.querySelector<HTMLElement>("#volumeLabel")!;
let volDragging = false;
let volBeforeMute = 100;
let volMuted = false;
const volIconOn = `<svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M14 20.725v-2.05q2.25-.65 3.625-2.5t1.375-4.2t-1.375-4.2T14 5.275v-2.05q3.1.7 5.05 3.138T21 11.975t-1.95 5.613T14 20.725M3 15V9h4l5-5v16l-5-5zm11 1V7.95q1.175.55 1.838 1.65T16.5 12q0 1.275-.663 2.363T14 16"/></svg>`;
const volIconOff = `<svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="m19.8 22.6l-3.025-3.025q-.625.4-1.325.688t-1.45.462v-2.05q.35-.125.688-.25t.637-.3L12 14.8V20l-5-5H3V9h3.2L1.4 4.2l1.4-1.4l18.4 18.4zm-.2-5.8l-1.45-1.45q.425-.775.638-1.625t.212-1.75q0-2.35-1.375-4.2T14 5.275v-2.05q3.1.7 5.05 3.138T21 11.975q0 1.325-.363 2.55T19.6 16.8m-3.35-3.35L14 11.2V7.95q1.175.55 1.838 1.65T16.5 12q0 .375-.062.738t-.188.712M12 9.2L9.4 6.6L12 4z"/></svg>`;

function updateVolIcon() {
  volumeBtn.innerHTML = volMuted ? volIconOff : volIconOn;
}

function applyVolume(vol: number) {
  volumeSlider.value = String(vol);
  volumeLabel.textContent = `${vol}%`;
  updateVolIcon();
  void invoke("set_tts_volume", { volume: vol });
}

volumeBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  if (volMuted) {
    volMuted = false;
    applyVolume(volBeforeMute || 100);
  } else {
    volBeforeMute = parseInt(volumeSlider.value, 10) || 100;
    volMuted = true;
    applyVolume(0);
  }
});

volumeWrap.addEventListener("mouseenter", () => { volumeWrap.classList.add("vol-open"); });
volumeWrap.addEventListener("mouseleave", () => { if (!volDragging) volumeWrap.classList.remove("vol-open"); });
volumeSlider.addEventListener("mousedown", () => { volDragging = true; });
window.addEventListener("mouseup", () => {
  if (!volDragging) return;
  volDragging = false;
  if (!volumeWrap.matches(":hover")) volumeWrap.classList.remove("vol-open");
});
volumeSlider.addEventListener("input", () => {
  const vol = parseInt(volumeSlider.value, 10);
  volMuted = vol === 0;
  volumeLabel.textContent = `${vol}%`;
  updateVolIcon();
  void invoke("set_tts_volume", { volume: vol });
});

// Guide dialog (first-time only)
const guideOverlay = document.querySelector<HTMLElement>("#guideOverlay")!;
const guideBtn = document.querySelector<HTMLButtonElement>("#guideBtn")!;
function dismissGuide() {
  guideOverlay.style.display = "none";
  void invoke("dismiss_guide");
}
guideOverlay.addEventListener("click", dismissGuide);
guideOverlay.querySelector(".guide-popup")!.addEventListener("click", (e) => e.stopPropagation());
guideBtn.addEventListener("click", dismissGuide);
el.minimizeBtn.addEventListener("click", () => void invoke("minimize_window"));
el.closeBtn.addEventListener("click", () => void invoke("close_window"));
el.applyBtn.addEventListener("click", () => void invoke("apply"));
el.toggleBtn.addEventListener("click", () => void invoke("toggle_run"));

document.addEventListener("keydown", (event) => {
  if (!state.hotkeyCaptureArmed) return;
  event.preventDefault();
  event.stopPropagation();
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return;
  state.hotkeyCaptureArmed = false;
  void invoke("add_hotkey", {
    key: event.key,
    code: event.code,
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    meta: event.metaKey,
  });
});

el.transcriptBody.addEventListener("scroll", () =>
  updateTranscriptScrollAffinity(el.transcriptBody, state),
);

bindDraftInput(el.firstLanguage, "first", "language");
bindDraftInput(el.firstAccent, "first", "accent");
bindDraftInput(el.firstTone, "first", "tone");
bindDraftInput(el.secondLanguage, "second", "language");
bindDraftInput(el.secondAccent, "second", "accent");
bindDraftInput(el.secondTone, "second", "tone");

window.__TG_SET_STATE = (payload: TranslationGummyState) => render(payload);
if (state.payload) render(state.payload);
requestAnimationFrame(drawVisualizer);
