import { fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { NarrationPanel } from "@/components/sidepanel/NarrationPanel";
import en from "@/i18n/en";

const mocks = vi.hoisted(() => ({
  update: vi.fn(),
}));

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "ko" }),
}));

vi.mock("@/hooks/useSubtitleNarration", () => ({
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET: 25,
  DEFAULT_NARRATION_TARGET_SEGMENT_SEC: 4,
  MIN_NARRATION_GROUP_TEXT_BUDGET: 5,
  MAX_NARRATION_GROUP_TEXT_BUDGET: 120,
  useSubtitleNarration: () => ({
    narrationStatus: null,
    narrationTargetCount: 0,
    canGenerateNarration: false,
    isGeneratingNarration: false,
    handleGenerateNarration: vi.fn(),
    handleCancelNarration: vi.fn(),
  }),
}));

vi.mock("@/hooks/useNarrationSettings", () => ({
  useNarrationSettings: () => ({
    settings: {
      method: "EdgeTTS",
      geminiModel: "",
      geminiVoice: "",
      geminiSpeed: "",
      geminiInstruction: "",
      geminiLanguageConditions: [],
      googleSpeed: "Normal",
      edgeVoice: "",
      edgePitch: 0,
      edgeRate: 0,
      edgeVoiceConfigs: [],
    },
    update: mocks.update,
    profile: { method: "EdgeTTS" },
    metadata: {
      providers: [],
      geminiVoices: [],
      geminiModels: [],
      geminiInstructionLanguages: [],
      geminiSpeedOptions: ["Slow", "Normal", "Fast"],
      googleSpeedOptions: ["Slow", "Normal"],
      defaults: {},
    },
  }),
}));

describe("NarrationPanel provider controls", () => {
  beforeEach(() => {
    mocks.update.mockClear();
  });

  function renderPanel() {
    return render(
      <NarrationPanel
        segment={null}
        composition={null}
        currentRawVideoPath=""
        currentRawMicAudioPath=""
        duration={0}
        visibleSubtitles={[
          {
            id: "subtitle-1",
            startTime: 0,
            endTime: 1,
            text: "Hello",
            style: { fontSize: 48, color: "#ffffff", x: 50, y: 80 },
          },
        ]}
        onApplyNarrationSegments={() => {}}
        onFinalizeNarrationSegments={() => {}}
        selectedSource="video"
        onSourceChange={() => {}}
        canUseVideoSource
        canUseMicSource={false}
        canUseAudioSource={false}
        onUpdateSegment={() => {}}
      />,
    );
  }

  it("offers only the online providers and renders the selected provider's controls", () => {
    const { container } = renderPanel();

    expect(container.querySelector(".narration-panel-pitch-slider")).not.toBeNull();
    const trigger = container.querySelector<HTMLElement>(".narration-method-select");
    expect(trigger).not.toBeNull();
    fireEvent.click(trigger!);
    const labels = Array.from(document.querySelectorAll(".panel-select-option"))
      .map((option) => option.textContent?.trim());
    expect(labels).toEqual([
      en.narrationTtsMethodGemini,
      en.narrationTtsMethodEdge,
      en.narrationTtsMethodGoogle,
    ]);
  });

  it("names the TTS method select by its visible label", () => {
    renderPanel();

    const trigger = document.querySelector(".narration-method-select");
    expect(trigger).not.toBeNull();
    expect(trigger).toHaveAccessibleName(en.narrationTtsMethod);
  });

  it("names the source track select by its visible label", () => {
    localStorage.setItem("screen-record-narration-mode-v1", "subtitles");
    const { container } = renderPanel();

    expect(container.querySelector(".narration-source-select"))
      .toHaveAccessibleName(en.narrationSourceTrack);
  });

  it("names every speech-to-speech select by its visible label", () => {
    localStorage.setItem("screen-record-narration-mode-v1", "s2s");
    const { container } = renderPanel();

    expect(container.querySelector(".narration-direct-method-select"))
      .toHaveAccessibleName(en.narrationTtsModel);
    expect(container.querySelector(".narration-s2s-source-select"))
      .toHaveAccessibleName(en.subtitleSource);
    expect(container.querySelector(".narration-s2s-language-select"))
      .toHaveAccessibleName(en.narrationS2sTarget);
    localStorage.removeItem("screen-record-narration-mode-v1");
  });

  it("keeps the speech-to-speech target across panel remounts and defaults to the UI language", () => {
    localStorage.removeItem("screen-record-s2s-target-language-v1");
    localStorage.setItem("screen-record-narration-mode-v1", "s2s");
    const first = renderPanel();
    expect(first.container.querySelector(".narration-s2s-language-select")).toHaveTextContent("Korean");
    first.unmount();

    localStorage.setItem("screen-record-s2s-target-language-v1", "ja");
    const second = renderPanel();
    expect(second.container.querySelector(".narration-s2s-language-select")).toHaveTextContent("Japanese");
    second.unmount();
    localStorage.removeItem("screen-record-narration-mode-v1");
    localStorage.removeItem("screen-record-s2s-target-language-v1");
  });
});
