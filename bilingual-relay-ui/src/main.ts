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

const state = {
  payload: window.__BR_INITIAL_STATE__ ?? null,
  hotkeyCaptureArmed: false,
  transcriptPinned: true,
  visualLevel: 0,
};

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App root not found");
}

app.innerHTML = `
  <div class="app-shell">
    <header class="titlebar">
      <div class="titlebar-drag" id="dragRegion">
        <div class="title-block">
          <div class="title" id="title"></div>
          <div class="subtitle" id="subtitle"></div>
        </div>
        <div class="status-pill">
          <span class="status-dot" id="statusDot"></span>
          <span id="statusText"></span>
        </div>
      </div>
      <div class="window-actions">
        <button class="icon-button" id="minimizeBtn" type="button" aria-label="Minimize">−</button>
        <button class="icon-button" id="closeBtn" type="button" aria-label="Close">×</button>
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

        <section class="card">
          <div class="card-header">
            <div class="card-title" id="hotkeyLabel"></div>
          </div>
          <div class="hotkey-row">
            <div class="hotkey-value" id="hotkeyValue">—</div>
            <button class="button secondary" id="setHotkeyBtn" type="button"></button>
            <button class="button secondary" id="clearHotkeyBtn" type="button"></button>
          </div>
          <div class="actions-row">
            <button class="button primary" id="applyBtn" type="button"></button>
            <button class="button secondary" id="toggleBtn" type="button"></button>
          </div>
          <div class="message" id="messageText"></div>
        </section>
      </aside>

      <section class="card transcript-card">
        <div class="transcript-head">
          <div class="card-title" id="transcriptTitle"></div>
        </div>
        <div class="message" id="transcriptMessage"></div>
        <div class="transcript-body" id="transcriptBody"></div>
      </section>
    </section>

    <section class="visualizer-dock">
      <div class="visualizer-copy">
        <div class="visualizer-status" id="visualizerStatus"></div>
      </div>
      <div class="visualizer-canvas-wrap">
        <canvas class="visualizer-canvas" id="visualizerCanvas"></canvas>
      </div>
    </section>
  </div>
`;

const elements = {
  root: document.documentElement,
  dragRegion: document.querySelector<HTMLElement>("#dragRegion")!,
  title: document.querySelector<HTMLElement>("#title")!,
  subtitle: document.querySelector<HTMLElement>("#subtitle")!,
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
  hotkeyLabel: document.querySelector<HTMLElement>("#hotkeyLabel")!,
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
    elements.transcriptBody.innerHTML = `<div class="transcript-empty">${escapeHtml(payload.strings.noTranscript)}</div>`;
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
  elements.subtitle.textContent = payload.isRunning
    ? payload.strings.transcriptTitle
    : payload.statusLabel;
  elements.firstTitle.textContent = payload.strings.firstProfile;
  elements.secondTitle.textContent = payload.strings.secondProfile;
  elements.language1Label.textContent = payload.strings.languageLabel;
  elements.accent1Label.textContent = payload.strings.accentLabel;
  elements.tone1Label.textContent = payload.strings.toneLabel;
  elements.language2Label.textContent = payload.strings.languageLabel;
  elements.accent2Label.textContent = payload.strings.accentLabel;
  elements.tone2Label.textContent = payload.strings.toneLabel;
  elements.hotkeyLabel.textContent = payload.strings.hotkeyLabel;
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

function drawVisualizer() {
  const canvas = elements.visualizerCanvas;
  const rect = canvas.getBoundingClientRect();
  const ratio = window.devicePixelRatio || 1;
  if (canvas.width !== Math.round(rect.width * ratio) || canvas.height !== Math.round(rect.height * ratio)) {
    canvas.width = Math.max(1, Math.round(rect.width * ratio));
    canvas.height = Math.max(1, Math.round(rect.height * ratio));
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) {
    requestAnimationFrame(drawVisualizer);
    return;
  }

  const payload = state.payload;
  const isActive = payload?.isRunning ?? false;
  const baseLevel = payload?.audioLevel ?? 0;
  const connectionState = payload?.connectionState ?? "stopped";
  const idleLevel = connectionState === "ready" ? 0.08 : connectionState === "reconnecting" ? 0.12 : 0.02;
  const target = isActive ? Math.max(baseLevel, idleLevel) : 0;
  state.visualLevel += (target - state.visualLevel) * 0.12;
  if (!isActive) {
    state.visualLevel *= 0.92;
  }

  const width = canvas.width;
  const height = canvas.height;
  ctx.clearRect(0, 0, width, height);

  const barCount = 34;
  const gap = width * 0.007;
  const totalGap = gap * (barCount - 1);
  const barWidth = (width - totalGap) / barCount;
  const centerY = height / 2;
  const time = performance.now() / 560;
  const color = connectionState === "error"
    ? "255, 142, 150"
    : connectionState === "reconnecting"
      ? "255, 215, 143"
      : "164, 184, 255";

  for (let index = 0; index < barCount; index += 1) {
    const x = index * (barWidth + gap);
    const harmonic = Math.sin(time + index * 0.28) * 0.22 + Math.sin(time * 1.8 + index * 0.12) * 0.16;
    const envelope = 0.35 + Math.sin((index / barCount) * Math.PI) * 0.65;
    const amplitude = Math.max(0.08, state.visualLevel + harmonic * 0.26) * envelope;
    const barHeight = Math.max(height * 0.16, amplitude * height * 0.92);
    const y = centerY - barHeight / 2;
    ctx.fillStyle = `rgba(${color}, ${0.18 + amplitude * 0.78})`;
    roundRect(ctx, x, y, barWidth, barHeight, barWidth / 2);
  }

  requestAnimationFrame(drawVisualizer);
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const r = Math.min(radius, width / 2, height / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + width, y, x + width, y + height, r);
  ctx.arcTo(x + width, y + height, x, y + height, r);
  ctx.arcTo(x, y + height, x, y, r);
  ctx.arcTo(x, y, x + width, y, r);
  ctx.closePath();
  ctx.fill();
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

drawVisualizer();
