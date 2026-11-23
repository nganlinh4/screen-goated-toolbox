import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { NarrationGeminiSettings } from "@/components/sidepanel/NarrationGeminiSettings";
import { LanguageConfigList } from "@/components/sidepanel/LanguageConfigList";
import { PanelSelect } from "@/components/ui/PanelSelect";
import en from "@/i18n/en";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

describe("narration select labels", () => {
  it("names the Gemini model, voice and speed selects by their visible labels", () => {
    const { container } = render(
      <NarrationGeminiSettings
        availableConditionLanguages={[]}
        geminiLanguageConditions={[]}
        geminiModels={[{ apiModel: "model-a", label: "Model A" }]}
        geminiSpeedOptions={["Normal"]}
        geminiVoices={[{ name: "Voice", gender: "Female" }]}
        narrationMode="subtitles"
        settings={{
          geminiModel: "model-a", geminiVoice: "Voice", geminiSpeed: "Normal",
          geminiParallelRequests: 2, geminiS2sParallelRequests: 2, geminiInstruction: "",
        } as never}
        update={vi.fn()}
        addLanguageCondition={vi.fn()}
        removeLanguageCondition={vi.fn()}
        updateLanguageCondition={vi.fn()}
      />,
    );

    expect(container.querySelector(".narration-model-select")).toHaveAccessibleName(en.narrationTtsModel);
    expect(container.querySelector(".narration-voice-select")).toHaveAccessibleName(en.narrationTtsVoice);
    expect(container.querySelector(".narration-speed-select")).toHaveAccessibleName(en.narrationTtsSpeed);
  });

  it("names each per-language voice select by the list title and its language", () => {
    const { container } = render(
      <LanguageConfigList
        title="Voices"
        items={[{ languageCode: "ko", languageName: "Korean" }]}
        removeTitle="Remove"
        availableLanguages={[{ languageCode: "en", languageName: "English" }]}
        addLabel="Add language"
        onAdd={vi.fn()}
        onRemove={vi.fn()}
        renderControl={() => (
          <PanelSelect value="a" options={[{ value: "a", label: "Voice A" }]} onChange={vi.fn()}
            triggerClassName="row-voice-select" />
        )}
        addTriggerClassName="row-add-select"
      />,
    );

    expect(container.querySelector(".row-voice-select")).toHaveAccessibleName("Voices Korean");
    expect(container.querySelector(".row-add-select")).not.toHaveAccessibleName("Voices");
  });
});
