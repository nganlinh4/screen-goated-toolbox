import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { JSDOM } from "jsdom";
import { renderTranscripts } from "./transcripts.ts";
import type { TranscriptRenderState } from "./transcripts.ts";
import type { TranslationGummyState } from "./types.ts";

const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/translation-gummy/transcript-rendering.json", import.meta.url), "utf8"));

function withRenderer(run: (body: HTMLElement, state: TranscriptRenderState) => void) {
  const dom = new JSDOM("<main></main>");
  const previous = Object.fromEntries(["window", "document", "requestAnimationFrame"].map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    requestAnimationFrame: (callback: FrameRequestCallback) => { callback(0); return 1; },
  });
  try {
    run(dom.window.document.querySelector("main")!, {
      transcriptPinned: false,
      lastTranscriptKey: "__init__",
      firstDetectedLang: "",
      deleteIcon: "<svg></svg>",
    });
  } finally {
    dom.window.close();
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
}

function payload(transcripts: TranslationGummyState["transcripts"], deleteItem = "Delete") {
  return { transcripts, strings: { chatHistory: "History", deleteItem } } as TranslationGummyState;
}

for (const entry of fixture.cases) test(`visible keyed rows: ${entry.name}`, () => {
  assert.equal(fixture.version, 1);
  withRenderer((body, state) => {
    let previousRows = new Map<string, Element>();
    for (const snapshot of entry.snapshots) {
      renderTranscripts(body, payload(snapshot.items), state);
      const rows = [...body.querySelectorAll<HTMLElement>(".transcript-pill")];
      assert.deepEqual(rows.map((row) => ({
        id: Number(row.dataset.eid!.slice(1)),
        input: row.querySelector(".pill-input")?.textContent ?? "",
        output: row.querySelector(".pill-output")?.textContent ?? "",
      })), snapshot.expectedPairs, entry.name);
      for (const row of rows) {
        const prior = previousRows.get(row.dataset.eid!);
        if (prior) assert.equal(row, prior, `${entry.name}: retained row identity`);
      }
      previousRows = new Map(rows.map((row) => [row.dataset.eid!, row]));
      assert.equal(body.querySelector("tag"), null, "transcripts remain literal text");
      if (!snapshot.items.length) assert.equal(body.querySelector(".empty-title")?.textContent, "History");
    }
  });
});

test("a changed transcript leaves other pill content untouched", () => withRenderer((body, state) => {
  const items: TranslationGummyState["transcripts"] = [
    { id: 7, role: "output", text: "retained", isFinal: true, lang: "" },
    { id: 8, role: "output", text: "growing", isFinal: false, lang: "" },
  ];
  renderTranscripts(body, payload(items), state);
  const retained = body.querySelector('[data-eid="e7"]')!;
  const observer = new window.MutationObserver(() => {});
  observer.observe(retained, { childList: true, subtree: true, attributes: true, characterData: true });
  const changed = structuredClone(items);
  changed[1].text += " text";
  renderTranscripts(body, payload(changed), state);
  assert.equal(body.querySelector('[data-eid="e8"] .pill-output')?.textContent, "growing text");
  assert.deepEqual(observer.takeRecords(), [], "unrelated text and controls retain their DOM");
  observer.disconnect();
}));

test("current locale updates existing transcript and separator delete controls", () => withRenderer((body, state) => {
  const items: TranslationGummyState["transcripts"] = [
    { id: 5, role: "separator", text: "01:02", isFinal: true, lang: "" },
    { id: 6, role: "output", text: "content", isFinal: true, lang: "" },
  ];
  renderTranscripts(body, payload(items), state);
  const buttons = [...body.querySelectorAll("button")];
  renderTranscripts(body, payload(structuredClone(items), "Remove"), state);
  assert.deepEqual([...body.querySelectorAll("button")], buttons, "controls keep identity and listeners");
  for (const button of buttons) {
    assert.equal(button.getAttribute("aria-label"), "Remove");
    assert.equal(button.title, "Remove");
  }
  const deleted: unknown[] = [];
  window.invoke = async (command, args) => { deleted.push([command, args]); };
  buttons[1].click();
  assert.deepEqual(deleted, [["delete_transcript", { id: 6 }]]);
}));

test("identical snapshots leave transcript DOM untouched", () => withRenderer((body, state) => {
  const items = fixture.cases[0].snapshots[0].items;
  renderTranscripts(body, payload(items), state);
  const observer = new window.MutationObserver(() => {});
  observer.observe(body, { childList: true, subtree: true, attributes: true, characterData: true });
  renderTranscripts(body, payload(structuredClone(items)), state);
  assert.deepEqual(observer.takeRecords(), []);
  observer.disconnect();
}));
