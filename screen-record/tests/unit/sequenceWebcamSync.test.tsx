import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useSequenceComposition, type UseSequenceCompositionParams } from "@/hooks/useSequenceComposition";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import type { ProjectComposition, VideoSegment, WebcamConfig } from "@/types/video";

describe("sequence webcam state", () => {
  it("updates the loaded clip before a later clip switch without changing the root", () => {
    const segment = { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as VideoSegment;
    let composition = {
      mode: "separate", focusedClipId: "clip", selectedClipId: "clip",
      clips: [
        { id: "root", segment, webcamConfig: { ...DEFAULT_WEBCAM_CONFIG }, backgroundConfig: DEFAULT_BACKGROUND_CONFIG },
        { id: "clip", segment, webcamConfig: { ...DEFAULT_WEBCAM_CONFIG }, backgroundConfig: DEFAULT_BACKGROUND_CONFIG },
      ],
    } as ProjectComposition;
    const sync = vi.fn((update: (previous: ProjectComposition) => ProjectComposition) => { composition = update(composition); });
    const params = {
      composition, currentProjectId: "p", currentProjectData: null,
      setComposition: vi.fn(), setCompositionSilently: sync, setCurrentProjectData: vi.fn(),
      backgroundConfig: DEFAULT_BACKGROUND_CONFIG, segment, mousePositions: [], duration: 10,
      currentRawVideoPath: "", currentRecordingMode: "withoutCursor", loadedClipId: "clip",
      isSwitchingCompositionClipRef: { current: false }, isProjectTransitionRef: { current: false },
      clipLoadRequestSeqRef: { current: 0 }, clearClipMediaCaches: vi.fn(),
      primePreloadSlot: vi.fn(), preloadedSlotClipIdsRef: { current: { previous: null, next: null } },
      persistRef: { current: null }, videoControllerRef: { current: undefined }, isPlaying: false,
      currentTime: 0, seek: vi.fn(), togglePlayback: vi.fn(), setShowProjectsDialog: vi.fn(),
    } as unknown as UseSequenceCompositionParams;
    const { rerender } = renderHook(({ webcamConfig }: { webcamConfig: WebcamConfig }) =>
      useSequenceComposition({ ...params, composition, webcamConfig }),
      { initialProps: { webcamConfig: { ...DEFAULT_WEBCAM_CONFIG } } });
    rerender({ webcamConfig: { ...DEFAULT_WEBCAM_CONFIG, mirror: true } });
    expect(composition.clips[0].webcamConfig.mirror).toBe(false);
    expect(composition.clips[1].webcamConfig.mirror).toBe(true);
  });
});
