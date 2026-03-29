import "./styles.css";

type RelayProfile = {
  language: string;
  accent: string;
  tone: string;
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
  hotkeyLabel: string;
  hotkeyError?: string | null;
  lastError?: string | null;
  transcripts: Array<{
    id: number;
    role: "input" | "output";
    text: string;
    isFinal: boolean;
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
  ready:        ['#00a8e0', '#00c8ff', '#40e0ff'],
  reconnecting: ['#FFD700', '#FFA500', '#FFDEAD'],
  error:        ['#ff6b7a', '#ff8e96', '#ffb3ba'],
  stopped:      ['#888888', '#AAAAAA', '#CCCCCC'],
  not_configured: ['#888888', '#AAAAAA', '#CCCCCC'],
  connecting:   ['#9F7AEA', '#805AD5', '#B794F4'],
};

const COLORS_LIGHT: Record<string, string[]> = {
  ready:        ['#0066cc', '#0088dd', '#00aaee'],
  reconnecting: ['#cc6600', '#dd8800', '#ee9900'],
  error:        ['#cc3344', '#dd5566', '#ee7788'],
  stopped:      ['#666666', '#888888', '#aaaaaa'],
  not_configured: ['#666666', '#888888', '#aaaaaa'],
  connecting:   ['#6B46C1', '#553C9A', '#805AD5'],
};

const state = {
  payload: window.__BR_INITIAL_STATE__ ?? null,
  hotkeyCaptureArmed: false,
  transcriptPinned: true,
  visualLevel: 0,
  barHeights: new Array(VISIBLE_BARS + 2).fill(6) as number[],
  scrollProgress: 0,
  lastTime: 0,
};

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App root not found");
}

app.innerHTML = `
  <div class="app-shell">
    <header class="titlebar" id="dragRegion">
      <div class="titlebar-drag">
        <div class="title" id="title"></div>
      </div>
      <div class="status-pill">
        <span class="status-dot" id="statusDot"></span>
        <span id="statusText"></span>
      </div>
      <div class="header-hotkey">
        <div class="hotkey-value" id="hotkeyValue">\u2014</div>
        <button class="text-btn" id="setHotkeyBtn" type="button"></button>
        <button class="text-btn" id="clearHotkeyBtn" type="button"></button>
      </div>
      <div class="window-actions">
        <button class="icon-button" id="minimizeBtn" type="button" aria-label="Minimize"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14"/></svg></button>
        <button class="icon-button" id="closeBtn" type="button" aria-label="Close"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6L6 18M6 6l12 12"/></svg></button>
      </div>
    </header>

    <section class="body-grid">
      <aside class="rail">
        <section class="card">
          <div class="card-header">
            <div class="card-title" id="firstTitle"></div>
          </div>
          <div class="profile-grid">
            <div class="field-row">
              <label class="field">
                <span class="field-label" id="language1Label"></span>
                <input class="text-input" id="firstLanguage" />
              </label>
            </div>
            <div class="field-row">
              <label class="field">
                <span class="field-label" id="accent1Label"></span>
                <input class="text-input" id="firstAccent" />
              </label>
              <label class="field">
                <span class="field-label" id="tone1Label"></span>
                <input class="text-input" id="firstTone" />
              </label>
            </div>
          </div>
        </section>

        <section class="card">
          <div class="card-header">
            <div class="card-title" id="secondTitle"></div>
          </div>
          <div class="profile-grid">
            <div class="field-row">
              <label class="field">
                <span class="field-label" id="language2Label"></span>
                <input class="text-input" id="secondLanguage" />
              </label>
            </div>
            <div class="field-row">
              <label class="field">
                <span class="field-label" id="accent2Label"></span>
                <input class="text-input" id="secondAccent" />
              </label>
              <label class="field">
                <span class="field-label" id="tone2Label"></span>
                <input class="text-input" id="secondTone" />
              </label>
            </div>
          </div>
        </section>

        <section class="visualizer-card">
          <div class="visualizer-header">
            <div class="visualizer-status" id="visualizerStatus"></div>
          </div>
          <div class="visualizer-canvas-wrap">
            <canvas class="visualizer-canvas" id="visualizerCanvas"></canvas>
          </div>
          <div class="actions-row">
            <button class="button primary" id="applyBtn" type="button"></button>
            <button class="button secondary" id="toggleBtn" type="button"></button>
          </div>
          <div class="message" id="messageText"></div>
        </section>
      </aside>

      <section class="transcript-card">
        <div class="transcript-head">
          <div class="card-title" id="transcriptTitle"></div>
        </div>
        <div class="message" id="transcriptMessage"></div>
        <div class="transcript-body" id="transcriptBody"></div>
      </section>
    </section>
  </div>
`;

const elements = {
  root: document.documentElement,
  dragRegion: document.querySelector<HTMLElement>("#dragRegion")!,
  title: document.querySelector<HTMLElement>("#title")!,
  firstTitle: document.querySelector<HTMLElement>("#firstTitle")!,
  secondTitle: document.querySelector<HTMLElement>("#secondTitle")!,
  language1Label: document.querySelector<HTMLElement>("#language1Label")!,
  accent1Label: document.querySelector<HTMLElement>("#accent1Label")!,
  tone1Label: document.querySelector<HTMLElement>("#tone1Label")!,
  language2Label: document.querySelector<HTMLElement>("#language2Label")!,
  accent2Label: document.querySelector<HTMLElement>("#accent2Label")!,
  tone2Label: document.querySelector<HTMLElement>("#tone2Label")!,
  firstLanguage: document.querySelector<HTMLInputElement>("#firstLanguage")!,
  firstAccent: document.querySelector<HTMLInputElement>("#firstAccent")!,
  firstTone: document.querySelector<HTMLInputElement>("#firstTone")!,
  secondLanguage: document.querySelector<HTMLInputElement>("#secondLanguage")!,
  secondAccent: document.querySelector<HTMLInputElement>("#secondAccent")!,
  secondTone: document.querySelector<HTMLInputElement>("#secondTone")!,
  hotkeyValue: document.querySelector<HTMLElement>("#hotkeyValue")!,
  setHotkeyBtn: document.querySelector<HTMLButtonElement>("#setHotkeyBtn")!,
  clearHotkeyBtn: document.querySelector<HTMLButtonElement>("#clearHotkeyBtn")!,
  applyBtn: document.querySelector<HTMLButtonElement>("#applyBtn")!,
  toggleBtn: document.querySelector<HTMLButtonElement>("#toggleBtn")!,
  minimizeBtn: document.querySelector<HTMLButtonElement>("#minimizeBtn")!,
  closeBtn: document.querySelector<HTMLButtonElement>("#closeBtn")!,
  messageText: document.querySelector<HTMLElement>("#messageText")!,
  transcriptTitle: document.querySelector<HTMLElement>("#transcriptTitle")!,
  transcriptMessage: document.querySelector<HTMLElement>("#transcriptMessage")!,
  transcriptBody: document.querySelector<HTMLElement>("#transcriptBody")!,
  statusDot: document.querySelector<HTMLElement>("#statusDot")!,
  statusText: document.querySelector<HTMLElement>("#statusText")!,
  visualizerStatus: document.querySelector<HTMLElement>("#visualizerStatus")!,
  visualizerCanvas: document.querySelector<HTMLCanvasElement>("#visualizerCanvas")!,
};

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function currentPayload(): RelayState {
  if (!state.payload) {
    throw new Error("Bilingual relay state not available");
  }
  return state.payload;
}

function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  if (window.invoke) {
    return window.invoke(cmd, args);
  }
  return Promise.resolve(null);
}

function bindDraftInput(
  element: HTMLInputElement,
  profile: "first" | "second",
  field: "language" | "accent" | "tone",
) {
  element.addEventListener("input", () => {
    void invoke("set_draft", { profile, field, value: element.value });
  });
}

function updateTranscriptScrollAffinity() {
  const el = elements.transcriptBody;
  const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
  state.transcriptPinned = distance < 36;
}

function renderTranscripts(payload: RelayState) {
  const stick = state.transcriptPinned;
  const items = payload.transcripts ?? [];
  if (!items.length) {
    elements.transcriptBody.innerHTML = `<div class="transcript-empty"><span class="empty-title">${escapeHtml(payload.strings.noTranscript)}</span></div>`;
    return;
  }
  elements.transcriptBody.innerHTML = items
    .map((item) => {
      const chip = item.role === "input" ? payload.strings.inputChip : payload.strings.outputChip;
      return `
        <article class="transcript-item">
          <div class="transcript-line">
            <span class="chip ${item.role === "output" ? "output" : ""}">${escapeHtml(chip)}</span>
            <div class="transcript-text">${escapeHtml(item.text)}</div>
          </div>
        </article>
      `;
    })
    .join("");
  if (stick) {
    requestAnimationFrame(() => {
      elements.transcriptBody.scrollTop = elements.transcriptBody.scrollHeight;
      state.transcriptPinned = true;
    });
  }
}

function connectionClass(connectionState: string) {
  if (connectionState === "ready") return "ready";
  if (connectionState === "reconnecting") return "reconnecting";
  if (connectionState === "error") return "error";
  return "";
}

function render(payload: RelayState) {
  state.payload = payload;
  elements.root.dataset.theme = payload.darkMode ? "dark" : "light";
  elements.title.textContent = payload.strings.title;
  elements.firstTitle.textContent = payload.strings.firstProfile;
  elements.secondTitle.textContent = payload.strings.secondProfile;
  elements.language1Label.textContent = payload.strings.languageLabel;
  elements.accent1Label.textContent = payload.strings.accentLabel;
  elements.tone1Label.textContent = payload.strings.toneLabel;
  elements.language2Label.textContent = payload.strings.languageLabel;
  elements.accent2Label.textContent = payload.strings.accentLabel;
  elements.tone2Label.textContent = payload.strings.toneLabel;
  // hotkeyLabel removed — hotkey is now in header
  elements.setHotkeyBtn.textContent = payload.strings.setHotkey;
  elements.clearHotkeyBtn.textContent = payload.strings.clearHotkey;
  elements.applyBtn.textContent = payload.strings.apply;
  elements.toggleBtn.textContent = payload.isRunning ? payload.strings.stop : payload.strings.start;
  elements.transcriptTitle.textContent = payload.strings.transcriptTitle;
  elements.statusText.textContent = payload.statusLabel;
  elements.visualizerStatus.textContent = payload.statusLabel;
  elements.statusDot.className = `status-dot ${connectionClass(payload.connectionState)}`.trim();

  const currentMessage = payload.hotkeyError || payload.lastError || "";
  elements.messageText.textContent = currentMessage;
  elements.transcriptMessage.textContent = currentMessage;

  elements.hotkeyValue.textContent = state.hotkeyCaptureArmed ? "…" : payload.hotkeyLabel || "—";
  elements.applyBtn.hidden = !payload.dirty;
  elements.applyBtn.disabled = !payload.dirty || !payload.canApply;
  elements.toggleBtn.disabled = !(payload.isRunning || payload.canToggle);
  elements.clearHotkeyBtn.disabled = !payload.draft.hotkey;

  if (document.activeElement !== elements.firstLanguage) {
    elements.firstLanguage.value = payload.draft.first.language ?? "";
  }
  if (document.activeElement !== elements.firstAccent) {
    elements.firstAccent.value = payload.draft.first.accent ?? "";
  }
  if (document.activeElement !== elements.firstTone) {
    elements.firstTone.value = payload.draft.first.tone ?? "";
  }
  if (document.activeElement !== elements.secondLanguage) {
    elements.secondLanguage.value = payload.draft.second.language ?? "";
  }
  if (document.activeElement !== elements.secondAccent) {
    elements.secondAccent.value = payload.draft.second.accent ?? "";
  }
  if (document.activeElement !== elements.secondTone) {
    elements.secondTone.value = payload.draft.second.tone ?? "";
  }

  renderTranscripts(payload);
}

function drawVisualizer(timestamp: number) {
  const canvas = elements.visualizerCanvas;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    requestAnimationFrame(drawVisualizer);
    return;
  }

  // Use fixed backing size like the recording indicator — let CSS handle display scaling.
  // The recording indicator uses 200x60 for a 100x30 CSS display.
  // We use a proportionally larger backing for the wider visualizer-canvas-wrap.
  if (canvas.width !== 400 || canvas.height !== 60) {
    canvas.width = 400;
    canvas.height = 60;
  }

  const payload = state.payload;
  const isActive = payload?.isRunning ?? false;
  const connectionState = payload?.connectionState ?? "stopped";
  const isDark = payload?.darkMode ?? true;
  const rms = isActive ? (payload?.audioLevel ?? 0) : 0;

  // Scroll bars left-to-right
  const dt = state.lastTime ? (timestamp - state.lastTime) / 1000 : 0.016;
  state.lastTime = timestamp;
  state.scrollProgress += dt / 0.15;

  while (state.scrollProgress >= 1) {
    state.scrollProgress -= 1;
    state.barHeights.shift();

    let displayRMS = rms;
    if (!isActive) {
      displayRMS = 0.02;
    } else if (connectionState === "connecting") {
      displayRMS = 0.08 + 0.12 * Math.abs(Math.sin(timestamp / 300));
    } else if (connectionState === "reconnecting") {
      displayRMS = 0.06 + 0.1 * Math.abs(Math.sin(timestamp / 250));
    }

    const h = canvas.height;
    const v = Math.max(6, Math.min(h - 4, displayRMS * 250 + 6));
    state.barHeights.push(v);
  }

  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const pixelOffset = state.scrollProgress * BAR_SPACING;
  const colorSet = isDark ? COLORS_DARK : COLORS_LIGHT;
  const currentColors = colorSet[connectionState] ?? colorSet.stopped;

  // Vertical gradient (bottom -> top)
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
      if (ctx.roundRect) {
        ctx.roundRect(x, y, BAR_WIDTH, pillHeight, BAR_WIDTH / 2);
      } else {
        ctx.rect(x, y, BAR_WIDTH, pillHeight);
      }
      ctx.fill();
    }
  }

  // Fade edges (30px left + right)
  const fadeWidth = 30;
  ctx.save();
  ctx.globalCompositeOperation = "destination-out";

  const leftGrad = ctx.createLinearGradient(0, 0, fadeWidth, 0);
  leftGrad.addColorStop(0, "rgba(0, 0, 0, 1)");
  leftGrad.addColorStop(1, "rgba(0, 0, 0, 0)");
  ctx.fillStyle = leftGrad;
  ctx.fillRect(0, 0, fadeWidth, h);

  const rightGrad = ctx.createLinearGradient(w - fadeWidth, 0, w, 0);
  rightGrad.addColorStop(0, "rgba(0, 0, 0, 0)");
  rightGrad.addColorStop(1, "rgba(0, 0, 0, 1)");
  ctx.fillStyle = rightGrad;
  ctx.fillRect(w - fadeWidth, 0, fadeWidth, h);

  ctx.restore();

  requestAnimationFrame(drawVisualizer);
}

elements.dragRegion.addEventListener("mousedown", (event) => {
  const target = event.target as HTMLElement;
  if (target.closest("button")) {
    return;
  }
  void invoke("drag_window");
});

elements.minimizeBtn.addEventListener("click", () => {
  void invoke("minimize_window");
});

elements.closeBtn.addEventListener("click", () => {
  void invoke("close_window");
});

elements.applyBtn.addEventListener("click", () => {
  void invoke("apply");
});

elements.toggleBtn.addEventListener("click", () => {
  void invoke("toggle_run");
});

elements.clearHotkeyBtn.addEventListener("click", () => {
  state.hotkeyCaptureArmed = false;
  void invoke("clear_hotkey");
});

elements.setHotkeyBtn.addEventListener("click", () => {
  state.hotkeyCaptureArmed = true;
  if (state.payload) {
    render(state.payload);
  }
});

document.addEventListener("keydown", (event) => {
  if (!state.hotkeyCaptureArmed) {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) {
    return;
  }
  state.hotkeyCaptureArmed = false;
  void invoke("set_hotkey", {
    key: event.key,
    code: event.code,
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    meta: event.metaKey,
  });
  if (state.payload) {
    render(state.payload);
  }
});

elements.transcriptBody.addEventListener("scroll", updateTranscriptScrollAffinity);

bindDraftInput(elements.firstLanguage, "first", "language");
bindDraftInput(elements.firstAccent, "first", "accent");
bindDraftInput(elements.firstTone, "first", "tone");
bindDraftInput(elements.secondLanguage, "second", "language");
bindDraftInput(elements.secondAccent, "second", "accent");
bindDraftInput(elements.secondTone, "second", "tone");

window.__BR_SET_STATE = (payload: RelayState) => {
  render(payload);
};

if (state.payload) {
  render(state.payload);
}

requestAnimationFrame(drawVisualizer);
