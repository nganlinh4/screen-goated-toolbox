import type { TranslationGummyState } from "./types";

type TranscriptPair = {
  id: number;
  input: string;
  output: string;
  lang: string;
  isFinal: boolean;
};

type TranscriptEntry =
  | TranscriptPair
  | { type: "separator"; time: string; id: number };

export type TranscriptRenderState = {
  transcriptPinned: boolean;
  lastTranscriptKey: string;
  firstDetectedLang: string;
};

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function transcriptKey(items: TranslationGummyState["transcripts"], chatHistory: string): string {
  if (!items.length) return `empty:${chatHistory}`;
  const last = items[items.length - 1];
  const langs = items.map((item) => item.lang || "_").join("");
  return `${items.length}:${last.id}:${last.text.length}:${last.isFinal ? 1 : 0}:${langs}:${chatHistory}`;
}

function groupTranscripts(items: TranslationGummyState["transcripts"]): TranscriptEntry[] {
  const entries: TranscriptEntry[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item.role === "separator") {
      entries.push({ type: "separator", time: item.text, id: item.id });
      continue;
    }
    if (item.role === "input") {
      const next = items[i + 1];
      if (next && next.role === "output") {
        entries.push({ id: item.id, input: item.text, output: next.text, lang: next.lang, isFinal: next.isFinal });
        i++;
      } else {
        entries.push({ id: item.id, input: item.text, output: "", lang: "", isFinal: item.isFinal });
      }
    } else {
      entries.push({ id: item.id, input: "", output: item.text, lang: item.lang, isFinal: item.isFinal });
    }
  }
  return entries;
}

function buildPillInner(pair: TranscriptPair): string {
  let html = `<div class="pill-content pill-appear">`;
  if (pair.input) html += `<span class="pill-input">${escapeHtml(pair.input)}</span>`;
  if (pair.output) html += `<span class="pill-output">${escapeHtml(pair.output)}</span>`;
  html += `</div>`;
  return html;
}

function insertAfter(parent: HTMLElement, node: Element, after: Element | null) {
  if (after && after.nextSibling) {
    parent.insertBefore(node, after.nextSibling);
  } else if (!after) {
    parent.insertBefore(node, parent.firstChild);
  } else {
    parent.appendChild(node);
  }
}

function brLog(message: string, data?: Record<string, unknown>) {
  if (data) {
    const flat = Object.entries(data)
      .map(([key, value]) => `${key}=${formatDebugValue(value)}`)
      .join(" ");
    console.log(`[BR] ${message}${flat ? ` ${flat}` : ""}`);
  } else {
    console.log(`[BR] ${message}`);
  }
}

function formatDebugValue(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) return `[${value.map(formatDebugValue).join(",")}]`;
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function animateClassChange(node: HTMLElement, wantClass: string) {
  if (node.className === wantClass) return;

  const beforeClass = node.className;
  const from = node.getBoundingClientRect();
  node.className = wantClass;
  const to = node.getBoundingClientRect();

  const dx = from.left - to.left;
  const dy = from.top - to.top;
  if (dx === 0 && dy === 0) return;

  brLog("slide", {
    eid: node.dataset.eid ?? "",
    from: beforeClass,
    to: wantClass,
    delta: `${dx.toFixed(1)},${dy.toFixed(1)}`,
  });

  const anim = node.animate(
    [
      { transform: `translate(${dx}px, ${dy}px)` },
      { transform: "translate(0px, 0px)" },
    ],
    {
      duration: 400,
      easing: "cubic-bezier(0.4, 0, 0.2, 1)",
      fill: "both",
    },
  );
  anim.onfinish = () => anim.cancel();
}

export function updateTranscriptScrollAffinity(
  body: HTMLElement,
  state: TranscriptRenderState,
) {
  state.transcriptPinned = body.scrollHeight - body.scrollTop - body.clientHeight < 36;
}

export function renderTranscripts(
  body: HTMLElement,
  payload: TranslationGummyState,
  state: TranscriptRenderState,
) {
  const items = payload.transcripts ?? [];
  const key = transcriptKey(items, payload.strings.chatHistory);
  if (key === state.lastTranscriptKey) return;
  state.lastTranscriptKey = key;

  const stick = state.transcriptPinned;
  if (!items.length) {
    body.innerHTML = `<div class="transcript-empty"><span class="empty-title">${escapeHtml(payload.strings.chatHistory)}</span></div>`;
    state.firstDetectedLang = "";
    return;
  }

  const entries = groupTranscripts(items);

  if (!state.firstDetectedLang) {
    for (const entry of entries) {
      if ("type" in entry) continue;
      if (entry.lang && entry.output) {
        state.firstDetectedLang = entry.lang;
        break;
      }
    }
  }
  const firstLang = state.firstDetectedLang;
  const currentIds = new Set(entries.map((entry) => `e${entry.id}`));

  for (const child of Array.from(body.children)) {
    const domId = (child as HTMLElement).dataset.eid;
    if (!domId || !currentIds.has(domId)) child.remove();
  }

  let prevNode: Element | null = null;
  let addedPill = false;
  for (const entry of entries) {
    const eid = `e${entry.id}`;
    let node = body.querySelector(`[data-eid="${eid}"]`);

    if ("type" in entry && entry.type === "separator") {
      if (!node) {
        node = document.createElement("div");
        (node as HTMLElement).dataset.eid = eid;
        node.className = "session-separator";
        node.innerHTML = `<span class="separator-time">${escapeHtml(entry.time)}</span>`;
        insertAfter(body, node, prevNode);
      }
      prevNode = node;
      continue;
    }

    const pair = entry as TranscriptPair;
    const decided = pair.lang && firstLang;
    const align = !decided ? "msg-center" : pair.lang === firstLang ? "msg-left" : "msg-right";
    const wantClass = `transcript-pill ${align}`;

    if (!node) {
      node = document.createElement("article");
      (node as HTMLElement).dataset.eid = eid;
      node.className = "transcript-pill msg-center";
      node.innerHTML = buildPillInner(pair);
      insertAfter(body, node, prevNode);
      addedPill = true;

      if (wantClass !== "transcript-pill msg-center") {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            if (node && node.isConnected) animateClassChange(node as HTMLElement, wantClass);
          });
        });
      }
    } else {
      const existing = node as HTMLElement;
      if (existing.className !== wantClass) {
        if (existing.classList.contains("msg-center") && align !== "msg-center") {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (existing.isConnected) animateClassChange(existing, wantClass);
            });
          });
        } else {
          animateClassChange(existing, wantClass);
        }
      }
      const content = existing.querySelector<HTMLElement>(".pill-content");
      if (content) {
        content.innerHTML = "";
        if (pair.input) content.insertAdjacentHTML("beforeend", `<span class="pill-input">${escapeHtml(pair.input)}</span>`);
        if (pair.output) content.insertAdjacentHTML("beforeend", `<span class="pill-output">${escapeHtml(pair.output)}</span>`);
      } else {
        existing.innerHTML = buildPillInner(pair);
      }
    }
    prevNode = node;
  }

  if (stick || addedPill) {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        body.scrollTop = body.scrollHeight;
        state.transcriptPinned = true;
      });
    });
  }
}
