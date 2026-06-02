import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { NarrationPanel } from "@/components/sidepanel/NarrationPanel";
import en from "@/i18n/en";

const mocks = vi.hoisted(() => ({
  update: vi.fn(),
}));

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en }),
}));

vi.mock("@/hooks/useSubtitleNarration", () => ({
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET: 25,
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
      method: "StepAudioEditX",
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
      kokoroVoice: "af_heart",
      kokoroSpeed: 1,
      kokoroNumThreads: 2,
      kokoroVoiceConfigs: [],
      stepAudioVoice: "default_en",
      stepAudioReferenceVoiceId: "",
      stepAudioPromptText: "Use a calm narration delivery.",
      magpieVoice: "",
      magpieVoiceConfigs: [],
    },
    update: mocks.update,
    profile: { method: "StepAudioEditX" },
    metadata: {
      providers: [{ method: "StepAudioEditX", label: "Step Audio EditX" }],
      geminiVoices: [],
      geminiModels: [],
      geminiInstructionLanguages: [],
      geminiSpeedOptions: ["Slow", "Normal", "Fast"],
      googleSpeedOptions: ["Slow", "Normal"],
      kokoroVoices: [],
      kokoroVoiceLanguages: [],
      magpieVoices: [],
      magpieVoiceLanguages: [],
      stepAudioVoices: [
        { id: "ref-demo", label: "Demo reference" },
      ],
      defaults: {},
    },
  }),
}));

describe("NarrationPanel Step Audio controls", () => {
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

  it("renders reference selector for Step Audio EditX", () => {
    renderPanel();

    expect(screen.getByText("Step Audio EditX")).toBeInTheDocument();
    expect(screen.getByText("Reference voice")).toBeInTheDocument();
    expect(screen.getByText("Bundled default reference")).toBeInTheDocument();
    expect(screen.queryByText(en.narrationTtsStepAudioPromptText)).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue("Use a calm narration delivery.")).not.toBeInTheDocument();
  });

  it("selects a reference voice", () => {
    renderPanel();

    fireEvent.click(screen.getByText("Bundled default reference"));
    fireEvent.click(screen.getByRole("button", { name: /demo reference/i }));

    expect(mocks.update).toHaveBeenCalledWith("stepAudioReferenceVoiceId", "ref-demo");
  });
});
