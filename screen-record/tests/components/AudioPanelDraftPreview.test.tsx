import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AudioPanel } from "@/components/sidepanel/AudioPanel";
import { ImportedAudioTrack } from "@/components/timeline/ImportedAudioTrack";
import { clearImportedAudioDrafts, getImportedAudioDrafts } from "@/lib/importedAudioDraftStore";
import en from "@/i18n/en";
import type { ImportedAudioSegment } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

function clip(id: string, startTime: number): ImportedAudioSegment {
  return {
    id,
    rawAudioPath: `C:/audio/${id}.wav`,
    name: `Clip ${id}`,
    duration: 4,
    startTime,
    inPoint: 0,
    outPoint: 2,
    addedAt: startTime,
  };
}

function Harness({ initial }: { initial: ImportedAudioSegment[] }) {
  const [segments, setSegments] = useState(initial);
  const selected = new Set(initial.map((segment) => segment.id));
  const update = (id: string, patch: Partial<ImportedAudioSegment>) =>
    setSegments((prev) => prev.map((segment) => (segment.id === id ? { ...segment, ...patch } : segment)));
  return (
    <>
      <AudioPanel
        importedSegments={segments}
        narrationSegments={[]}
        subtitleSegments={[]}
        selectedImportedIds={selected}
        selectedNarrationIds={new Set()}
        onUpdateImportedSegment={update}
        onUpdateNarrationSegment={vi.fn()}
      />
      <ImportedAudioTrack segments={segments} duration={20} selectedIds={selected} canvasWidthPx={1000} />
    </>
  );
}

function trackSegment(id: string) {
  const element = document.querySelector<HTMLElement>(`.audio-track-segment[data-audio-segment-id="${id}"]`);
  if (!element) throw new Error(`missing segment ${id}`);
  return element;
}

function speedBadges(id: string) {
  return [...trackSegment(id).querySelectorAll(".audio-track-segment-speed")].map((badge) => badge.textContent);
}

describe("AudioPanel rate drafts", () => {
  afterEach(() => {
    clearImportedAudioDrafts([...getImportedAudioDrafts().keys()]);
  });

  it("previews a single-clip rate draft declaratively and clears it on commit", () => {
    render(<Harness initial={[clip("a", 0)]} />);
    const slider = screen.getByLabelText(en.audioPanelSpeed);

    fireEvent.change(slider, { target: { value: "2" } });
    expect(speedBadges("a")).toEqual(["2.00×"]);
    expect(trackSegment("a").style.width).toBe("5%");
    expect(getImportedAudioDrafts().get("a")).toEqual({ playbackRate: 2 });

    fireEvent.pointerUp(slider);
    expect(getImportedAudioDrafts().size).toBe(0);
    expect(speedBadges("a")).toEqual(["2.00×"]);
    expect(trackSegment("a").style.width).toBe("5%");
  });

  it("apply-to-all commit leaves exactly one React-owned badge per clip and no drafts", () => {
    render(<Harness initial={[clip("a", 0), clip("b", 5)]} />);
    const slider = screen.getByLabelText(en.audioPanelBulkSpeed);

    fireEvent.change(slider, { target: { value: "1.5" } });
    expect(speedBadges("a")).toEqual(["1.50×"]);
    expect(speedBadges("b")).toEqual(["1.50×"]);

    fireEvent.pointerUp(slider);
    expect(getImportedAudioDrafts().size).toBe(0);
    expect(speedBadges("a")).toEqual(["1.50×"]);
    expect(speedBadges("b")).toEqual(["1.50×"]);

    // Returning to 1x through committed state hides the badge entirely.
    fireEvent.change(slider, { target: { value: "1" } });
    fireEvent.pointerUp(slider);
    expect(speedBadges("a")).toEqual([]);
    expect(speedBadges("b")).toEqual([]);
    expect(trackSegment("a").querySelectorAll("[data-sgt-preview-created]")).toHaveLength(0);
  });

  it("drops a clip's draft when the panel unmounts without a commit", () => {
    const { unmount } = render(<Harness initial={[clip("a", 0)]} />);
    fireEvent.change(screen.getByLabelText(en.audioPanelSpeed), { target: { value: "3" } });
    expect(getImportedAudioDrafts().has("a")).toBe(true);
    unmount();
    expect(getImportedAudioDrafts().size).toBe(0);
  });
});
