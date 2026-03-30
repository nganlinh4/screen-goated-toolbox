import "./styles.css";

type RelayProfile = {
  language: string;
  accent: string;
  tone: string;
};

type HotkeyItem = {
  code: number;
  name: string;
  modifiers: number;
};

type RelayState = {
  darkMode: boolean;
  statusLabel: string;
  connectionState: string;
  isRunning: boolean;
  dirty: boolean;
  canApply: boolean;
  canToggle: boolean;
  audioLevel: number;
  draft: {
    first: RelayProfile;
    second: RelayProfile;
  };
  hotkeys: HotkeyItem[];
  guideSeen: boolean;
  ttsModel: string;
  ttsVoice: string;
  hotkeyError?: string | null;
  lastError?: string | null;
  transcripts: Array<{
    id: number;
    role: "input" | "output";
    text: string;
    isFinal: boolean;
    lang: string;
  }>;
  strings: {
    title: string;
    firstProfile: string;
    secondProfile: string;
    languageLabel: string;
    accentLabel: string;
    toneLabel: string;
    hotkeyLabel: string;
    setHotkey: string;
    clearHotkey: string;
    apply: string;
    start: string;
    stop: string;
    transcriptTitle: string;
    inputChip: string;
    outputChip: string;
    noTranscript: string;
    guide: string;
    guideOk: string;
    chatHistory: string;
    currentModel: string;
    currentVoice: string;
  };
};

declare global {
  interface Window {
    __BR_INITIAL_STATE__?: RelayState;
    __BR_SET_STATE?: (payload: RelayState) => void;
    invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  }
}

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
  payload: window.__BR_INITIAL_STATE__ ?? null,
  hotkeyCaptureArmed: false,
  transcriptPinned: true,
  visualLevel: 0,
  barHeights: new Array(VISIBLE_BARS + 2).fill(6) as number[],
  scrollProgress: 0,
  lastTime: 0,
  lastTranscriptKey: "__init__",
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
        <button class="icon-button" id="settingsBtn" type="button" aria-label="TTS Settings"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg></button>
      </div>
      <div class="status-pill">
        <span class="status-dot" id="statusDot"></span>
        <span id="statusText"></span>
      </div>
      <div class="header-hotkey" id="hotkeyArea"></div>
      <div class="window-actions">
        <button class="icon-button" id="minimizeBtn" type="button" aria-label="Minimize"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14"/></svg></button>
        <button class="icon-button" id="closeBtn" type="button" aria-label="Close"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6L6 18M6 6l12 12"/></svg></button>
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
        <div class="message" id="transcriptMessage"></div>
        <div class="transcript-body" id="transcriptBody"></div>
      </section>
    </section>
    <div class="guide-overlay" id="guideOverlay" style="display:none">
      <div class="guide-popup">
        <div class="guide-title">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/></svg>
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
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
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

function updateTranscriptScrollAffinity() {
  const body = el.transcriptBody;
  state.transcriptPinned = body.scrollHeight - body.scrollTop - body.clientHeight < 36;
}

function transcriptKey(items: RelayState["transcripts"]): string {
  if (!items.length) return `empty:${state.payload?.strings.chatHistory}`;
  const last = items[items.length - 1];
  const langs = items.map(i => i.lang || "_").join("");
  return `${items.length}:${last.id}:${last.text.length}:${last.isFinal ? 1 : 0}:${langs}:${state.payload?.strings.chatHistory}`;
}

type TranscriptPair = {
  input: string;
  output: string;
  lang: string; // detected language of the OUTPUT text
  isFinal: boolean;
};

type TranscriptEntry = TranscriptPair | { type: "separator"; time: string };

function groupTranscripts(items: RelayState["transcripts"]): TranscriptEntry[] {
  const entries: TranscriptEntry[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item.role === "separator") {
      entries.push({ type: "separator", time: item.text });
      continue;
    }
    if (item.role === "input") {
      const next = items[i + 1];
      if (next && next.role === "output") {
        entries.push({ input: item.text, output: next.text, lang: next.lang, isFinal: next.isFinal });
        i++;
      } else {
        entries.push({ input: item.text, output: "", lang: item.lang, isFinal: item.isFinal });
      }
    } else {
      entries.push({ input: "", output: item.text, lang: item.lang, isFinal: item.isFinal });
    }
  }
  return entries;
}

function renderTranscripts(payload: RelayState) {
  const items = payload.transcripts ?? [];
  const key = transcriptKey(items);
  if (key === state.lastTranscriptKey) return;
  state.lastTranscriptKey = key;

  const stick = state.transcriptPinned;
  if (!items.length) {
    el.transcriptBody.innerHTML = `<div class="transcript-empty"><span class="empty-title">${escapeHtml(payload.strings.chatHistory)}</span></div>`;
    return;
  }

  const entries = groupTranscripts(items);
  const firstLang = (entries.find((e): e is TranscriptPair => "lang" in e && !!e.lang) as TranscriptPair | undefined)?.lang ?? "";

  el.transcriptBody.innerHTML = entries
    .map((entry) => {
      if ("type" in entry && entry.type === "separator") {
        return `<div class="session-separator"><span class="separator-time">${escapeHtml(entry.time)}</span></div>`;
      }
      const pair = entry as TranscriptPair;
      const isLeft = !pair.lang || !firstLang || pair.lang === firstLang;
      const align = isLeft ? "msg-left" : "msg-right";

      let html = `<article class="transcript-pill ${align}"><div class="pill-content">`;
      if (pair.input) {
        html += `<span class="pill-input">${escapeHtml(pair.input)}</span>`;
      }
      if (pair.output) {
        html += `<span class="pill-output">${escapeHtml(pair.output)}</span>`;
      }
      html += `</div></article>`;
      return html;
    })
    .join("");

  if (stick) {
    requestAnimationFrame(() => {
      el.transcriptBody.scrollTop = el.transcriptBody.scrollHeight;
      state.transcriptPinned = true;
    });
  }
}

function hotkeyKey(hotkeys: HotkeyItem[]): string {
  return hotkeys.map((h) => `${h.code}:${h.modifiers}`).join(",");
}

function renderHotkeys(payload: RelayState) {
  const armed = state.hotkeyCaptureArmed ? "1" : "0";
  const key = hotkeyKey(payload.hotkeys ?? []) + "|" + armed + "|" + payload.strings.setHotkey;
  if (key === state.lastHotkeyKey) return;
  state.lastHotkeyKey = key;

  const hotkeys = payload.hotkeys ?? [];
  let html = "";

  // Hotkey badges with X
  for (let i = 0; i < hotkeys.length; i++) {
    html += `<button class="hotkey-badge" data-remove="${i}" type="button">${escapeHtml(hotkeys[i].name)}<svg class="badge-x" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M18 6L6 18M6 6l12 12"/></svg></button>`;
  }

  // Capture state or Add button
  if (state.hotkeyCaptureArmed) {
    html += `<span class="hotkey-capture">...</span><button class="hotkey-cancel" id="cancelHotkeyBtn" type="button"><svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M18 6L6 18M6 6l12 12"/></svg></button>`;
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

function render(payload: RelayState) {
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
  renderTranscripts(payload);
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
  if ((e.target as HTMLElement).closest("button, .hotkey-badge, .text-btn, .hotkey-capture")) return;
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

el.transcriptBody.addEventListener("scroll", updateTranscriptScrollAffinity);

bindDraftInput(el.firstLanguage, "first", "language");
bindDraftInput(el.firstAccent, "first", "accent");
bindDraftInput(el.firstTone, "first", "tone");
bindDraftInput(el.secondLanguage, "second", "language");
bindDraftInput(el.secondAccent, "second", "accent");
bindDraftInput(el.secondTone, "second", "tone");

window.__BR_SET_STATE = (payload: RelayState) => render(payload);
if (state.payload) render(state.payload);
requestAnimationFrame(drawVisualizer);
