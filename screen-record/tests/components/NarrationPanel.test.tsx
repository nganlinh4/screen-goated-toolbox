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

  it("renders reference selector for Step Audio EditX", () => {
    render(
      <NarrationPanel
        visibleSubtitles={[]}
        onApplyNarrationSegments={() => {}}
        onFinalizeNarrationSegments={() => {}}
      />,
    );

    expect(screen.getByText("Step Audio EditX")).toBeInTheDocument();
    expect(screen.getByText("Reference voice")).toBeInTheDocument();
    expect(screen.getByText("Bundled default reference")).toBeInTheDocument();
    expect(screen.queryByText(en.narrationTtsStepAudioPromptText)).not.toBeInTheDocument();
    expect(screen.queryByDisplayValue("Use a calm narration delivery.")).not.toBeInTheDocument();
  });

  it("selects a reference voice", () => {
    render(
      <NarrationPanel
        visibleSubtitles={[]}
        onApplyNarrationSegments={() => {}}
        onFinalizeNarrationSegments={() => {}}
      />,
    );

    fireEvent.click(screen.getByText("Bundled default reference"));
    fireEvent.click(screen.getByRole("button", { name: /demo reference/i }));

    expect(mocks.update).toHaveBeenCalledWith("stepAudioReferenceVoiceId", "ref-demo");
  });
});
