import { act, renderHook } from "@testing-library/react";
import { StrictMode, useEffect, useRef, useState } from "react";
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
  const backgroundConfigRef = { current: backgroundConfig };
  const webcamConfigRef = { current: webcamConfig };
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
    backgroundConfigRef,
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
    isProjectTransitionRef: { current: false },
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
    webcamConfigRef,
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
    vi.useRealTimers();
  });

  it("keeps undo and redo authoritative over a deferred playback update", () => {
    vi.useFakeTimers();
    const original = { trimStart: 0, trimEnd: 10, subtitleSegments: [] } as VideoSegment;
    const generated = { ...original, subtitleSegments: [{ id: "generated" }] } as VideoSegment;
    const before = createComposition();
    const after = { ...before, mode: "unified" as const };
    const { result } = renderHook(() => {
      const [segment, rawSetSegment] = useState<VideoSegment | null>(original);
      const [composition, rawSetComposition] = useState<ProjectComposition | null>(before);
      const segmentRef = useRef<VideoSegment | null>(original);
      const pending = useRef<VideoSegment | null>(null);
      const timer = useRef<number | null>(null);
      const project = useRef<Project | null>({
        id: "project", name: "Project", createdAt: 1, lastModified: 1, duration: 10,
        segment: original, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
        webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [], composition: before,
      });
      useEffect(() => { segmentRef.current = segment; }, [segment]);
      const history = useAppHistoryState({
        backgroundConfig: DEFAULT_BACKGROUND_CONFIG, backgroundConfigRef: { current: DEFAULT_BACKGROUND_CONFIG },
        webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
        composition, currentProjectDataRef: project, currentRawMicAudioPath: "", currentRawVideoPath: "",
        currentRawWebcamVideoPath: "", currentRecordingMode: "withoutCursor", duration: 10,
        handleProjectRawVideoPathChange: vi.fn(), isPlaying: true, isPlayingRef: { current: true },
        isProjectTransitionRef: { current: false }, pendingSilentSegmentRef: pending,
        pendingSilentSegmentTimerRef: timer, rawSetBackgroundConfig: vi.fn(), rawSetComposition,
        rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
        rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(),
        rawSetSegment, rawSetWebcamConfig: vi.fn(), segment, segmentRef,
        setBackgroundConfigState: vi.fn(), setCurrentProjectData: vi.fn(),
        setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
      });
      return { segment, composition, history, pending };
    });
    act(() => {
      result.current.history.setComposition(after);
      result.current.history.setSegmentSilently(generated);
    });
    expect(result.current.pending.current).toEqual(generated);
    act(() => result.current.history.undo());
    expect(result.current.segment).toEqual(original);
    expect(result.current.composition).toEqual(before);
    act(() => vi.advanceTimersByTime(301));
    expect(result.current.segment).toEqual(original);
    expect(result.current.composition).toEqual(before);
    act(() => result.current.history.redo());
    expect(result.current.segment).toEqual(generated);
    expect(result.current.composition).toEqual(after);
  });

  it("keeps redo available when an autosave write-back lands after undo", () => {
    const before = createComposition();
    const edited = { ...before, mode: "unified" as const };
    const { result } = renderHook(() => {
      const [composition, rawSetComposition] = useState<ProjectComposition | null>(before);
      const project = useRef<Project | null>({
        id: "project", name: "Project", createdAt: 1, lastModified: 1, duration: 10,
        segment: { trimStart: 0, trimEnd: 10 } as VideoSegment, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
        webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [], composition: before,
      });
      const history = useAppHistoryState({
        backgroundConfig: DEFAULT_BACKGROUND_CONFIG, backgroundConfigRef: { current: DEFAULT_BACKGROUND_CONFIG },
        webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
        composition, currentProjectDataRef: project, currentRawMicAudioPath: "", currentRawVideoPath: "",
        currentRawWebcamVideoPath: "", currentRecordingMode: "withoutCursor", duration: 10,
        handleProjectRawVideoPathChange: vi.fn(), isPlaying: false, isPlayingRef: { current: false },
        isProjectTransitionRef: { current: false }, pendingSilentSegmentRef: { current: null },
        pendingSilentSegmentTimerRef: { current: null }, rawSetBackgroundConfig: vi.fn(), rawSetComposition,
        rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
        rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(),
        rawSetSegment: vi.fn(), rawSetWebcamConfig: vi.fn(), segment: null, segmentRef: { current: null },
        setBackgroundConfigState: vi.fn(), setCurrentProjectData: vi.fn(),
        setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
      });
      return { composition, history };
    }, { wrapper: StrictMode });
    act(() => result.current.history.setComposition(edited));
    act(() => result.current.history.undo());
    expect(result.current.composition).toEqual(before);
    // Persistence normalizes and writes back the saved composition (new identity).
    act(() => result.current.history.setCompositionSilently({ ...before }));
    expect(result.current.history.canRedo).toBe(true);
    expect(result.current.history.canUndo).toBe(false);
    act(() => result.current.history.redo());
    expect(result.current.composition).toEqual(edited);
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

    expect(cloneSpy).not.toHaveBeenCalled();
    expect(hook.result.current.canUndo).toBe(false);
    expect(hook.rawSetBackgroundConfig).not.toHaveBeenCalled();
    expect(hook.rawSetComposition).not.toHaveBeenCalled();
    expect(hook.rawSetSegment).not.toHaveBeenCalled();
    expect(hook.rawSetWebcamConfig).not.toHaveBeenCalled();
    expect(hook.setCurrentProjectData).not.toHaveBeenCalled();
  });

  it.each(["setComposition", "setCompositionSilently"] as const)(
    "keeps explicit empty audio lanes when using %s",
    (setter) => {
    const composition = createComposition();
    const hook = renderAppHistory(composition);

    act(() => {
      hook.result.current[setter]({
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
    expect(updated?.audioSegments).toEqual([]);
    expect(updated?.audioTrackVolumePoints).toEqual([]);
    expect(updated?.narrationSegments).toEqual([]);
    expect(updated?.narrationTrackVolumePoints).toEqual([]);
    expect(hook.currentProjectDataRef.current.composition).toBe(updated);
  });

  it("preserves absent lanes from the same project", () => {
    const composition = createComposition();
    const hook = renderAppHistory(composition);
    act(() => {
      hook.result.current.setComposition({
        ...composition,
        audioSegments: undefined,
        narrationSegments: undefined,
      });
    });
    const updated = hook.rawSetComposition.mock.calls[0]?.[0] as ProjectComposition;
    expect(updated.audioSegments).toBe(composition.audioSegments);
    expect(updated.narrationSegments).toBe(composition.narrationSegments);
  });

  it("does not copy old project lanes into a newly selected project", () => {
    const hook = renderAppHistory();
    hook.currentProjectDataRef.current = {
      ...hook.currentProjectDataRef.current,
      id: "project-2",
      composition: undefined,
    };
    const fresh: ProjectComposition = {
      mode: "separate",
      selectedClipId: null,
      focusedClipId: null,
      clips: [],
    };
    act(() => hook.result.current.setComposition(fresh));
    expect(hook.rawSetComposition).toHaveBeenCalledWith(fresh);
    expect(hook.currentProjectDataRef.current.composition?.audioSegments).toBeUndefined();
    expect(hook.currentProjectDataRef.current.composition?.narrationSegments).toBeUndefined();
  });

  it("undoes a slider change and a background upload as separate steps under StrictMode", () => {
    const original = { ...DEFAULT_BACKGROUND_CONFIG, scale: 100 };
    const { result } = renderHook(() => {
      const [backgroundConfig, setBackgroundState] = useState(original);
      const backgroundConfigRef = useRef(original);
      // Mirrors the editor: the ref is the synchronous source of truth for the next edit.
      const applyBackground = (next: typeof original | ((prev: typeof original) => typeof original)) => {
        const resolved = typeof next === "function" ? next(backgroundConfigRef.current) : next;
        backgroundConfigRef.current = resolved;
        setBackgroundState(resolved);
      };
      const project = useRef<Project | null>({
        id: "project", name: "Project", createdAt: 1, lastModified: 1, duration: 10,
        segment: { trimStart: 0, trimEnd: 10 } as VideoSegment, backgroundConfig: original,
        webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [],
      });
      const history = useAppHistoryState({
        backgroundConfig, backgroundConfigRef,
        webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
        composition: null, currentProjectDataRef: project, currentRawMicAudioPath: "", currentRawVideoPath: "",
        currentRawWebcamVideoPath: "", currentRecordingMode: "withoutCursor", duration: 10,
        handleProjectRawVideoPathChange: vi.fn(), isPlaying: false, isPlayingRef: { current: false },
        isProjectTransitionRef: { current: false }, pendingSilentSegmentRef: { current: null },
        pendingSilentSegmentTimerRef: { current: null }, rawSetBackgroundConfig: applyBackground,
        rawSetComposition: vi.fn(), rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
        rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(),
        rawSetSegment: vi.fn(), rawSetWebcamConfig: vi.fn(), segment: null, segmentRef: { current: null },
        setBackgroundConfigState: applyBackground, setCurrentProjectData: vi.fn(),
        setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
      });
      return { backgroundConfig, history };
    }, { wrapper: StrictMode });

    act(() => result.current.history.setBackgroundConfig((prev) => ({
      ...prev, backgroundType: "custom", customBackground: "upload.png",
    })));
    act(() => result.current.history.setBackgroundConfig((prev) => ({ ...prev, scale: 80 })));
    expect(result.current.backgroundConfig).toMatchObject({ customBackground: "upload.png", scale: 80 });

    act(() => result.current.history.undo());
    expect(result.current.backgroundConfig).toMatchObject({ customBackground: "upload.png", scale: 100 });
    act(() => result.current.history.undo());
    expect(result.current.backgroundConfig.customBackground).toBe(original.customBackground);
    expect(result.current.backgroundConfig.scale).toBe(100);
    act(() => result.current.history.redo());
    act(() => result.current.history.redo());
    expect(result.current.backgroundConfig).toMatchObject({ customBackground: "upload.png", scale: 80 });
  });
});
