import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { JSDOM } from "jsdom";
import { build } from "vite";
import type { TranslationGummyState } from "./types.ts";

function fixture(name: string) {
  return JSON.parse(readFileSync(new URL(`../../parity-fixtures/translation-gummy/${name}.json`, import.meta.url), "utf8"));
}

const volume = fixture("volume-control");
const onboarding = fixture("onboarding-contract");
const output = await build({
  root: fileURLToPath(new URL("..", import.meta.url)),
  configFile: false,
  logLevel: "silent",
  build: {
    write: false,
    minify: false,
    lib: { entry: fileURLToPath(new URL("./main.ts", import.meta.url)), formats: ["iife"], name: "Gummy" },
  },
});
assert.ok(!Array.isArray(output) || output.length === 1);
const bundle = (Array.isArray(output) ? output[0] : output).output.find((item) => item.type === "chunk")!;
assert.equal(bundle.type, "chunk");

function payload(guideSeen = true): TranslationGummyState {
  return {
    darkMode: true, statusLabel: "Stopped", connectionState: "stopped", isRunning: false,
    dirty: false, canApply: true, canToggle: true, audioLevel: 0, hotkeys: [], guideSeen, volumePercent: volume.defaultVolumePercent, volumeRestorePercent: volume.defaultVolumePercent,
    draft: { first: { language: "Korean", accent: "", tone: "" }, second: { language: "English", accent: "", tone: "" } },
    transcripts: [], transcriptLimit: 200, ttsModel: "", ttsVoice: "",
    strings: {
      title: onboarding.titleSource, guide: onboarding.messageSource, guideOk: onboarding.confirmLabelSource,
      firstProfile: "First", secondProfile: "Second", languageLabel: "Language", accentLabel: "Accent",
      toneLabel: "Tone", hotkeyLabel: "Hotkey", setHotkey: "Set", clearHotkey: "Clear", apply: "Apply",
      start: "Start", stop: "Stop", transcriptTitle: "Transcript", inputChip: "Input", outputChip: "Output",
      noTranscript: "Empty", chatHistory: "History", maxItems: "Limit", clearAll: "Clear", deleteItem: "Delete",
      currentModel: "Model", currentVoice: "Voice",
    },
  } as TranslationGummyState;
}

function withSurface(initial: TranslationGummyState, run: (window: Window, calls: Array<[string, Record<string, unknown>]>) => void) {
  const dom = new JSDOM('<div id="app"></div>', { runScripts: "outside-only" });
  const calls: Array<[string, Record<string, unknown>]> = [];
  dom.window.requestAnimationFrame = () => 1;
  dom.window.__TG_INITIAL_STATE__ = initial;
  dom.window.invoke = async (command, args = {}) => { calls.push([command, JSON.parse(JSON.stringify(args))]); };
  try {
    dom.window.eval(bundle.code);
    run(dom.window as unknown as Window, calls);
  } finally {
    dom.window.close();
  }
}

for (const entry of volume.cases) test(`actual volume controls: ${entry.name}`, () => {
  withSurface({ ...payload(), volumePercent: entry.initial.percent, volumeRestorePercent: entry.initial.restorePercent }, (window, calls) => {
    const slider = window.document.querySelector<HTMLInputElement>("#volumeSlider")!;
    const button = window.document.querySelector<HTMLButtonElement>("#volumeBtn")!;
    assert.equal(Number(slider.min), volume.range.min);
    assert.equal(Number(slider.max), volume.range.max);
    assert.equal(Number(slider.step), volume.range.step);
    assert.equal(Number(slider.defaultValue), volume.defaultVolumePercent);
    assert.equal(Number(slider.value), entry.initial.percent);
    const set = (value: number) => {
      slider.value = String(value);
      slider.dispatchEvent(new window.Event("input", { bubbles: true }));
    };
    for (const step of entry.steps) {
      if (step.type === "setVolume") set(step.value);
      else { assert.equal(step.type, "toggleMute"); button.click(); }
    }
    assert.equal(Number(slider.value), entry.expected.percent);
    assert.equal(window.document.querySelector("#volumeLabel")!.textContent, `${entry.expected.percent}%`);
    assert.ok(calls.length > 0);
    assert.ok(calls.every(([command, args]) => command === "toggle_tts_mute" && Object.keys(args).length === 0 || command === "set_tts_volume" && Number(args.volume) >= volume.range.min && Number(args.volume) <= volume.range.max));
    // Observe the restore target through the real mute control,without reading private state.
    if (entry.expected.percent > 0) button.click();
    button.click();
    assert.equal(Number(slider.value), entry.expected.restorePercent || volume.defaultVolumePercent);
  });
});

test("host volume snapshots update controls without restarting or sending volume commands", () => {
  const initial = payload();
  initial.volumePercent = 65;
  initial.volumeRestorePercent = 65;
  withSurface(initial, (window, calls) => {
    const slider = window.document.querySelector<HTMLInputElement>("#volumeSlider")!;
    assert.equal(slider.value, "65");
    assert.deepEqual(calls, []);
    const icon = window.document.querySelector("#volumeBtn svg")!;
    window.__TG_SET_STATE!({ ...initial, connectionState: "reconnecting" });
    assert.equal(window.document.querySelector("#volumeBtn svg"), icon, "unchanged snapshot retains icon content");
    assert.equal(slider.value, "65");
    assert.deepEqual(calls, []);
    const button = window.document.querySelector<HTMLButtonElement>("#volumeBtn")!;
    button.click();
    assert.equal(slider.value, "0");
    window.__TG_SET_STATE!({ ...initial, volumePercent: 0 });
    button.click();
    assert.equal(slider.value, "65");
    assert.deepEqual(calls, [["toggle_tts_mute", {}], ["toggle_tts_mute", {}]]);
  });
});

test("onboarding uses shared label sources and dismisses only through its controls", () => {
  assert.equal(onboarding.persistedFlag, "guide_seen");
  withSurface(payload(onboarding.showWhen.guide_seen), (window, calls) => {
    const overlay = window.document.querySelector<HTMLElement>("#guideOverlay")!;
    assert.notEqual(overlay.style.display, "none");
    assert.equal(window.document.querySelector("#guideTitle")!.textContent, onboarding.titleSource);
    assert.equal(window.document.querySelector("#guideMsg")!.textContent, onboarding.messageSource);
    assert.equal(window.document.querySelector("#guideBtn")!.textContent, onboarding.confirmLabelSource);
    window.document.querySelector<HTMLElement>(".guide-popup")!.click();
    assert.deepEqual(calls, []);
    window.document.querySelector<HTMLButtonElement>("#guideBtn")!.click();
    assert.deepEqual(calls, [["dismiss_guide", {}]]);
    assert.equal(overlay.style.display, "none");
    window.__TG_SET_STATE!(payload(false));
    assert.equal(overlay.style.display, "none", "an older host snapshot cannot reopen a dismissed guide");
  });
});

test("persisted dismissal suppresses onboarding in a new surface", () => {
  assert.equal(onboarding.dismissBehavior.persistGuideSeen, true);
  assert.equal(onboarding.dismissBehavior.suppressFutureAutoShow, true);
  withSurface(payload(true), (window, calls) => {
    assert.equal(window.document.querySelector<HTMLElement>("#guideOverlay")!.style.display, "none");
    assert.deepEqual(calls, []);
  });
});

test("onboarding backdrop dismissal sends the persistence command", () => {
  withSurface(payload(false), (window, calls) => {
    window.document.querySelector<HTMLElement>("#guideOverlay")!.click();
    assert.deepEqual(calls, [["dismiss_guide", {}]]);
  });
});

test("host snapshots do not move the transcript limit slider while it is being dragged", () => {
  const initial = payload();
  withSurface(initial, (window, calls) => {
    const slider = window.document.querySelector<HTMLInputElement>("#transcriptLimit")!;
    const label = window.document.querySelector<HTMLOutputElement>("#transcriptLimitValue")!;
    assert.equal(slider.value, "200");
    slider.value = "50";
    slider.dispatchEvent(new window.Event("input", { bubbles: true }));
    // A live session pushes a snapshot (e.g. a new transcript) mid-drag.
    window.__TG_SET_STATE!({ ...initial, statusLabel: "Listening" });
    assert.equal(slider.value, "50");
    assert.equal(label.value, "50");
    slider.dispatchEvent(new window.Event("change", { bubbles: true }));
    assert.deepEqual(calls, [["set_transcript_limit", { limit: 50 }]]);
    window.__TG_SET_STATE!({ ...initial, transcriptLimit: 50 });
    assert.equal(slider.value, "50");
    window.__TG_SET_STATE!({ ...initial, transcriptLimit: 120 });
    assert.equal(slider.value, "120", "a committed edit lets later host snapshots through again");
    assert.equal(label.value, "120");
  });
});

test("an abandoned transcript limit drag stops holding back host snapshots", () => {
  const initial = payload();
  withSurface(initial, (window) => {
    const slider = window.document.querySelector<HTMLInputElement>("#transcriptLimit")!;
    slider.value = "50";
    slider.dispatchEvent(new window.Event("input", { bubbles: true }));
    slider.value = "200";
    slider.dispatchEvent(new window.Event("input", { bubbles: true }));
    slider.dispatchEvent(new window.Event("blur"));
    window.__TG_SET_STATE!({ ...initial, transcriptLimit: 80 });
    assert.equal(slider.value, "80");
  });
});

const SHARED_SYMBOL_DIR = new URL("../../ui-shared/material-symbols/", import.meta.url);
function sharedSymbolPath(name: string): string {
  const svg = readFileSync(new URL(`${name}.svg`, SHARED_SYMBOL_DIR), "utf8");
  return /\sd="([^"]+)"/.exec(svg)![1];
}

test("every icon comes from the shared Material Symbols catalog", () => {
  for (const file of ["main.ts", "transcripts.ts"]) {
    const source = readFileSync(new URL(`./${file}`, import.meta.url), "utf8");
    assert.doesNotMatch(source, /<path\b|\sd="/, `${file} pastes SVG path data`);
  }
  const initial = payload(false);
  initial.hotkeys = [{ code: 112, name: "F1", modifiers: 0 }];
  initial.transcripts = [{ id: 1, role: "input", text: "hello", isFinal: true, lang: "" }];
  withSurface(initial, (window) => {
    const pathOf = (selector: string) => window.document.querySelector(`${selector} path`)?.getAttribute("d");
    assert.equal(pathOf("#settingsBtn"), sharedSymbolPath("settings"));
    assert.equal(pathOf("#volumeBtn"), sharedSymbolPath("volume_up"));
    assert.equal(pathOf("#minimizeBtn"), sharedSymbolPath("remove"));
    assert.equal(pathOf("#closeBtn"), sharedSymbolPath("close"));
    assert.equal(pathOf(".guide-title"), sharedSymbolPath("info"));
    assert.equal(pathOf(".hotkey-badge"), sharedSymbolPath("close"));
    assert.equal(pathOf(".transcript-delete"), sharedSymbolPath("close"));
    window.document.querySelector<HTMLButtonElement>("#volumeBtn")!.click();
    assert.equal(pathOf("#volumeBtn"), sharedSymbolPath("volume_off"));
    window.document.querySelector<HTMLButtonElement>("#addHotkeyBtn")!.click();
    assert.equal(pathOf("#cancelHotkeyBtn"), sharedSymbolPath("close"));
  });
});
