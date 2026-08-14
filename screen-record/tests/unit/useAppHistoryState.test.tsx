import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useAppHistoryState } from "@/hooks/useAppHistoryState";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import type {
  Project,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";

function createComposition(): ProjectComposition {
  return {
    mode: "separate",
    selectedClipId: null,
    focusedClipId: null,
    clips: [],
    audioSegments: [{
      id: "audio-1",
      rawAudioPath: "audio.wav",
      name: "Audio",
      duration: 10,
      startTime: 0,
      inPoint: 0,
      outPoint: 10,
      addedAt: 1,
    }],
    audioTrackVolumePoints: [{ time: 0, volume: 1 }],
    narrationSegments: [{
      id: "narration-1",
      rawAudioPath: "narration.wav",
      name: "Narration",
      duration: 10,
      startTime: 0,
      inPoint: 0,
      outPoint: 10,
      addedAt: 1,
    }],
    narrationTrackVolumePoints: [{ time: 0, volume: 1 }],
  };
}

function renderAppHistory(composition = createComposition()) {
  const backgroundConfig = { ...DEFAULT_BACKGROUND_CONFIG };
  const webcamConfig = { ...DEFAULT_WEBCAM_CONFIG };
  const segment = { subtitles: [] } as unknown as VideoSegment;
  const project: Project = {
    id: "project-1",
    name: "Project",
    createdAt: 1,
    lastModified: 1,
    duration: 10,
    segment,
    backgroundConfig,
    webcamConfig,
    mousePositions: [],
    rawVideoPath: "video.mp4",
    composition,
  };
  const currentProjectDataRef = { current: project };
  const segmentRef = { current: segment };
  const rawSetBackgroundConfig = vi.fn();
  const rawSetComposition = vi.fn();
  const rawSetSegment = vi.fn();
  const rawSetWebcamConfig = vi.fn();
  const setCurrentProjectData = vi.fn();

  const hook = renderHook(() => useAppHistoryState({
    backgroundConfig,
    composition,
    currentProjectDataRef,
    currentRawMicAudioPath: "",
    currentRawVideoPath: "video.mp4",
    currentRawWebcamVideoPath: "",
    currentRecordingMode: "withoutCursor",
    duration: 10,
    handleProjectRawVideoPathChange: vi.fn(),
    isPlaying: false,
    isPlayingRef: { current: false },
    pendingSilentSegmentRef: { current: null },
    pendingSilentSegmentTimerRef: { current: null },
    rawSetBackgroundConfig,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath: vi.fn(),
    rawSetCurrentRawVideoPath: vi.fn(),
    rawSetCurrentRawWebcamVideoPath: vi.fn(),
    rawSetCurrentRecordingMode: vi.fn(),
    rawSetSegment,
    rawSetWebcamConfig,
    segment,
    segmentRef,
    setBackgroundConfigState: vi.fn(),
    setCurrentProjectData,
    setLastRawSavedPath: vi.fn(),
    setPreviewDuration: vi.fn(),
    webcamConfig,
  }));

  return {
    ...hook,
    currentProjectDataRef,
    rawSetBackgroundConfig,
    rawSetComposition,
    rawSetSegment,
    rawSetWebcamConfig,
    setCurrentProjectData,
  };
}

describe("useAppHistoryState", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("ignores equivalent editor updates without cloning or dispatching", () => {
    const cloneSpy = vi.spyOn(globalThis, "structuredClone");
    const hook = renderAppHistory();

    act(() => {
      hook.result.current.setSegment((previous) => previous);
      hook.result.current.setSegmentSilently((previous) => previous);
      hook.result.current.setComposition((previous) => previous);
      hook.result.current.setCompositionSilently((previous) => previous);
      hook.result.current.setBackgroundConfig((previous) => ({ ...previous }));
      hook.result.current.setWebcamConfig((previous) => ({ ...previous }));
      hook.result.current.setEditorPreviewDuration(10);
      hook.result.current.handleEditorRawVideoPathChange("video.mp4");
    });

    expect(cloneSpy).toHaveBeenCalledTimes(1);
    expect(hook.result.current.canUndo).toBe(false);
    expect(hook.rawSetBackgroundConfig).not.toHaveBeenCalled();
    expect(hook.rawSetComposition).not.toHaveBeenCalled();
    expect(hook.rawSetSegment).not.toHaveBeenCalled();
    expect(hook.rawSetWebcamConfig).not.toHaveBeenCalled();
    expect(hook.setCurrentProjectData).not.toHaveBeenCalled();
  });

  it("preserves populated project audio lanes from explicit empty updates", () => {
    const composition = createComposition();
    const hook = renderAppHistory(composition);

    act(() => {
      hook.result.current.setComposition({
        ...composition,
        audioSegments: [],
        audioTrackVolumePoints: [],
        narrationSegments: [],
        narrationTrackVolumePoints: [],
      });
    });

    const updated = hook.rawSetComposition.mock.calls[0]?.[0] as
      | ProjectComposition
      | undefined;
    expect(updated?.audioSegments).toBe(composition.audioSegments);
    expect(updated?.audioTrackVolumePoints).toBe(
      composition.audioTrackVolumePoints,
    );
    expect(updated?.narrationSegments).toBe(composition.narrationSegments);
    expect(updated?.narrationTrackVolumePoints).toBe(
      composition.narrationTrackVolumePoints,
    );
    expect(hook.currentProjectDataRef.current.composition).toBe(updated);
  });
});
