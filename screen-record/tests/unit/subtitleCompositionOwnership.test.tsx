import { act, renderHook } from "@testing-library/react";
import { useCallback, useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { useSubtitleGeneration } from "@/hooks/useSubtitleGeneration";
import { useSubtitleResultApplication } from "@/hooks/useSubtitleResultApplication";
import { useAppHistoryState } from "@/hooks/useAppHistoryState";
import { loadClipMediaIntoEditorCore, type LoadClipMediaCoreParams } from "@/hooks/useClipMediaCache/helpers";
import { useSequenceComposition, type UseSequenceCompositionParams } from "@/hooks/useSequenceComposition";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import type { Project, ProjectComposition, VideoSegment } from "@/types/video";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: {} }));
vi.mock("@/hooks/subtitleSourceOwnership", () => ({
  subtitleSourceMatches: () => true,
  captureSubtitleSourceOwners: () => ({}),
}));

const rootSegment = {
  trimStart: 0, trimEnd: 3,
  trimSegments: [{ id: "root-range", startTime: 0, endTime: 3 }],
  subtitleSegments: [],
} as VideoSegment;
const clipSegment = {
  trimStart: 0, trimEnd: 7,
  trimSegments: [{ id: "clip-range", startTime: 0, endTime: 7 }],
  subtitleSegments: [],
} as VideoSegment;
const initialComposition = {
  mode: "unified", focusedClipId: "clip", selectedClipId: "clip",
  clips: [
    { id: "root", duration: 3, segment: rootSegment, webcamConfig: DEFAULT_WEBCAM_CONFIG, backgroundConfig: DEFAULT_BACKGROUND_CONFIG },
    { id: "clip", duration: 7, segment: clipSegment, webcamConfig: DEFAULT_WEBCAM_CONFIG, backgroundConfig: DEFAULT_BACKGROUND_CONFIG },
  ],
} as ProjectComposition;
const mousePositions: UseSequenceCompositionParams["mousePositions"] = [];

describe("unified composition editor ownership", () => {
  it("groups partial and final subtitle results into one undo boundary", () => {
    const { result } = renderHook(() => {
      const [segment, rawSetSegment] = useState<VideoSegment | null>(clipSegment);
      const [composition, rawSetComposition] = useState<ProjectComposition | null>(initialComposition);
      const segmentRef = useRef<VideoSegment | null>(clipSegment);
      const projectRef = useRef<Project | null>({
        id: "p", name: "Project", createdAt: 1, lastModified: 1, duration: 7,
        segment: clipSegment, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
        webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [], composition: initialComposition,
      });
      const history = useAppHistoryState({
        backgroundConfig: DEFAULT_BACKGROUND_CONFIG, backgroundConfigRef: { current: DEFAULT_BACKGROUND_CONFIG },
        webcamConfig: DEFAULT_WEBCAM_CONFIG, webcamConfigRef: { current: DEFAULT_WEBCAM_CONFIG },
        composition, currentProjectDataRef: projectRef, currentRawMicAudioPath: "", currentRawVideoPath: "",
        currentRawWebcamVideoPath: "", currentRecordingMode: "withoutCursor", duration: 7,
        handleProjectRawVideoPathChange: vi.fn(), isPlaying: false, isPlayingRef: { current: false },
        isProjectTransitionRef: { current: false }, pendingSilentSegmentRef: { current: null },
        pendingSilentSegmentTimerRef: { current: null }, rawSetBackgroundConfig: vi.fn(), rawSetComposition,
        rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
        rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(),
        rawSetSegment, rawSetWebcamConfig: vi.fn(), segment, segmentRef,
        setBackgroundConfigState: vi.fn(), setCurrentProjectData: vi.fn(),
        setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
      });
      const subtitle = useSubtitleResultApplication({
        segment, loadedClipIdRef: { current: "clip" }, composition,
        setSegment: history.setSegment, setSegmentSilently: history.setSegmentSilently,
        setComposition: history.setComposition, setCompositionSilently: history.setCompositionSilently,
        jobId: "job", jobContext: {
          indicator: {} as never, sourceTypeForNative: "video", replacementRangesByClip: {}, clipTransformsByClip: {},
        },
      });
      return { segment, composition, history, subtitle };
    });
    for (const [text, isPartial] of [["first", true], ["second", true], ["final", false]] as const) {
      act(() => result.current.subtitle.queueSubtitleResults([
        { clipId: "clip", isPartial, segments: [{ startTime: 1, endTime: 2, text }] },
      ], true));
    }
    expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ text: "final" }),
    ]));
    expect(result.current.history.canUndo).toBe(true);
    act(() => result.current.history.undo());
    expect(result.current.composition?.clips[1].segment.subtitleSegments).toEqual([]);
    expect(result.current.segment?.subtitleSegments).toEqual([]);
    expect(result.current.history.canUndo).toBe(false);
  });

  it("keeps the active editor segment clip-local when composition changes", () => {
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>(initialComposition);
      const [segment, setEditorSegment] = useState<VideoSegment | null>(clipSegment);
      const updates = useRef(0);
      const setSegment = useCallback((value: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => {
        // Bound a defective effect loop so the test can inspect the first corruption.
        updates.current += 1;
        if (updates.current <= 1) setEditorSegment(value);
      }, []);
      const params = {
        composition, setComposition, setCompositionSilently: setComposition,
        currentProjectId: "p", currentProjectData: null, setCurrentProjectData: vi.fn(),
        backgroundConfig: DEFAULT_BACKGROUND_CONFIG, webcamConfig: DEFAULT_WEBCAM_CONFIG,
        segment, mousePositions, duration: 7, currentRawVideoPath: "", currentRecordingMode: "withoutCursor", loadedClipId: "clip",
        isSwitchingCompositionClipRef: { current: false }, isProjectTransitionRef: { current: false },
        clipLoadRequestSeqRef: { current: 0 }, clearClipMediaCaches: vi.fn(),
        primePreloadSlot: vi.fn(), preloadedSlotClipIdsRef: { current: { previous: null, next: null } },
        persistRef: { current: null }, videoControllerRef: { current: undefined }, isPlaying: false,
        currentTime: 0, seek: vi.fn(), togglePlayback: vi.fn(), setShowProjectsDialog: vi.fn(),
      } as unknown as UseSequenceCompositionParams;
      useSequenceComposition(params);
      useSubtitleGeneration({
        t: {} as never, projectResetKey: "p", segment, setSegment, setSegmentSilently: setSegment,
        composition, setComposition, setCompositionSilently: setComposition, activeClipId: "clip", loadedClipIdRef: { current: "clip" }, currentRawVideoPath: "",
        currentRawMicAudioPath: "", duration: 7, setActivePanel: vi.fn(),
      });
      return { composition, segment, setComposition, setEditorSegment, updates };
    });

    act(() => result.current.setComposition((prev) => prev && { ...prev, audioSegments: [] }));
    expect(result.current.updates.current).toBe(0);
    expect(result.current.segment?.trimEnd).toBe(7);
    expect(result.current.composition?.clips[0].segment.trimEnd).toBe(3);
    expect(result.current.composition?.clips[1].segment.trimEnd).toBe(7);

    act(() => result.current.setEditorSegment((prev) => prev && { ...prev, trimEnd: 6 }));
    expect(result.current.composition?.clips[0].segment.trimEnd).toBe(3);
    expect(result.current.composition?.clips[1].segment.trimEnd).toBe(6);
  });

  it.each(["unified", "separate"] as const)("initializes %s imported-audio results in sequence time without modifying a clip", (mode) => {
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>({ ...initialComposition, mode });
      const subtitle = useSubtitleResultApplication({
        segment: clipSegment, setSegment: vi.fn(), setSegmentSilently: vi.fn(), loadedClipIdRef: { current: "clip" }, composition, setComposition, setCompositionSilently: setComposition,
        jobId: "job", jobContext: {
          indicator: {} as never, sourceTypeForNative: "audio", replacementRangesByClip: {},
          clipTransformsByClip: { audio: {
            kind: "audio", audioSegmentId: "audio-id", sourceName: "audio",
            sourcePath: "audio.wav", timelineOffsetSec: 4, sourceLocalOffsetSec: 0,
          } },
        },
      });
      return { composition, ...subtitle };
    });
    act(() => result.current.queueSubtitleResults([
      { clipId: "audio", isPartial: false, segments: [{ startTime: 0, endTime: 1, text: "hello" }] },
    ], true));
    const composition = result.current.composition!;
    expect(composition.globalSegment?.trimEnd).toBe(10);
    expect(composition.globalSegment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ startTime: 4, endTime: 5, text: "hello" }),
    ]));
    expect(composition.clips.map((clip) => clip.segment.trimEnd)).toEqual([3, 7]);
  });

  it("retains sequence-owned audio subtitles when a clip result replaces its overlapping window", () => {
    const compositionWithAudio = {
      ...initialComposition,
      audioSegments: [{ id: "audio-id", rawAudioPath: "audio.wav", name: "audio", duration: 10,
        startTime: 0, inPoint: 0, outPoint: 10, addedAt: 1 }],
    } as ProjectComposition;
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>(compositionWithAudio);
      const subtitle = useSubtitleResultApplication({
        segment: clipSegment, setSegment: vi.fn(), setSegmentSilently: vi.fn(),
        loadedClipIdRef: { current: "clip" }, composition, setComposition, setCompositionSilently: setComposition, jobId: "job",
        jobContext: {
          indicator: {} as never, sourceTypeForNative: "audio", replacementRangesByClip: {},
          clipTransformsByClip: { audio: {
            kind: "audio", audioSegmentId: "audio-id", sourceName: "audio", sourcePath: "audio.wav",
            timelineOffsetSec: 4, sourceLocalOffsetSec: 0,
          } },
        },
      });
      return { composition, ...subtitle };
    });
    act(() => result.current.queueSubtitleResults([
      { clipId: "audio", isPartial: false, segments: [{ startTime: 0, endTime: 1, text: "global audio" }] },
      { clipId: "clip", isPartial: false, segments: [{ startTime: 1, endTime: 2, text: "clip source" }] },
    ], true));
    expect(result.current.composition?.globalSegment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ text: "global audio", startTime: 4, endTime: 5 }),
      expect.objectContaining({ text: "clip source" }),
    ]));
  });

  it("updates the active clip editor as well as its composition after subtitle results", () => {
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>(initialComposition);
      const [segment, setSegment] = useState<VideoSegment | null>(clipSegment);
      const subtitle = useSubtitleResultApplication({
        segment, setSegment, setSegmentSilently: setSegment, loadedClipIdRef: { current: "clip" }, composition, setComposition, setCompositionSilently: setComposition, jobId: "job",
        jobContext: { indicator: {} as never, sourceTypeForNative: "video", replacementRangesByClip: {}, clipTransformsByClip: {} },
      });
      return { composition, segment, ...subtitle };
    });
    act(() => result.current.queueSubtitleResults([
      { clipId: "clip", isPartial: false, segments: [{ startTime: 1, endTime: 2, text: "active" }] },
    ], true));
    expect(result.current.composition?.clips[1].segment.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ startTime: 1, endTime: 2, text: "active" }),
    ]));
    expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ startTime: 1, endTime: 2, text: "active" }),
    ]));
  });

  it("loads the newest target clip when subtitles arrive during an asynchronous clip switch", async () => {
    let finishVideo!: (url: string) => void;
    const videoLoad = new Promise<string>((resolve) => { finishVideo = resolve; });
    const loadedClipIdRef = { current: "root" as string | null };
    const targetClip = { ...initialComposition.clips[1], rawVideoPath: "clip.mp4" };
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>(initialComposition);
      const [segment, setSegment] = useState<VideoSegment | null>(rootSegment);
      const subtitle = useSubtitleResultApplication({
        segment, setSegment, setSegmentSilently: setSegment, loadedClipIdRef, composition, setComposition, setCompositionSilently: setComposition,
        jobId: "job", jobContext: {
          indicator: {} as never, sourceTypeForNative: "video", replacementRangesByClip: {}, clipTransformsByClip: {},
        },
      });
      return { composition, segment, setSegment, ...subtitle };
    });
    const params = {
      projectId: "p", clip: targetClip, project: { id: "p" }, nextComposition: initialComposition,
      clipPreviewDuration: 7, clipThumbnailPlaceholder: null, isLatestRequest: () => true,
      resolveCurrentClip: () => result.current.composition?.clips.find((clip) => clip.id === "clip") ?? null,
      clipUrlCacheRef: { current: new Map() }, currentVideo: null, currentAudio: null,
      currentMicAudio: null, currentWebcamVideo: null, webcamVideoRef: { current: null },
      videoControllerRef: { current: { loadVideo: () => videoLoad, loadDeviceAudio: async () => null, duration: 7 } },
      invalidateThumbnails: vi.fn(), loadClipAssets: async () => ({ videoBlob: new Blob(["video"]) }),
      getClipMediaUrls: async () => null, drawPreloadedClipFrame: vi.fn(), preferPreloadedFrame: false,
      deferThumbnailsMs: 0, setCurrentVideo: vi.fn(), setCurrentAudio: vi.fn(), setCurrentMicAudio: vi.fn(),
      setCurrentWebcamVideo: vi.fn(), setPreviewDuration: vi.fn(), setSegment: result.current.setSegment,
      setThumbnails: vi.fn(), applyLoadedBackgroundConfig: vi.fn(), setWebcamConfig: vi.fn(),
      setMousePositions: vi.fn(), setCurrentRecordingMode: vi.fn(), handleProjectRawVideoPathChange: vi.fn(),
      setCurrentRawMicAudioPath: vi.fn(), setCurrentRawWebcamVideoPath: vi.fn(),
      setLoadedClipId: (id: string) => { loadedClipIdRef.current = id; },
      generateThumbnailsForSource: async () => {},
    } as unknown as LoadClipMediaCoreParams;
    const loading = loadClipMediaIntoEditorCore(params);
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    act(() => result.current.queueSubtitleResults([
      { clipId: "clip", isPartial: false, segments: [{ startTime: 1, endTime: 2, text: "during load" }] },
    ], true));
    expect(result.current.segment).toEqual(rootSegment);
    await act(async () => { finishVideo("clip-url"); await loading; });
    expect(loadedClipIdRef.current).toBe("clip");
    expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ text: "during load" }),
    ]));

    let finishStale!: (url: string) => void;
    let ownsRequest = true;
    const setStaleSegment = vi.fn();
    const staleLoad = loadClipMediaIntoEditorCore({
      ...params,
      isLatestRequest: () => ownsRequest,
      setSegment: setStaleSegment,
      videoControllerRef: { current: {
        loadVideo: () => new Promise<string>((resolve) => { finishStale = resolve; }),
      } } as unknown as LoadClipMediaCoreParams["videoControllerRef"],
    });
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    ownsRequest = false;
    await act(async () => { finishStale("stale-url"); await staleLoad; });
    expect(setStaleSegment).not.toHaveBeenCalled();
    expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ text: "during load" }),
    ]));
  });

  it("targets the synchronously loaded clip before React publishes its rendered identity", () => {
    const loadedClipIdRef = { current: "root" as string | null };
    const { result } = renderHook(() => {
      const [composition, setComposition] = useState<ProjectComposition | null>(initialComposition);
      const [segment, setSegment] = useState<VideoSegment | null>(rootSegment);
      const subtitle = useSubtitleResultApplication({
        segment, setSegment, setSegmentSilently: setSegment, loadedClipIdRef, composition, setComposition, setCompositionSilently: setComposition,
        jobId: "job", jobContext: {
          indicator: {} as never, sourceTypeForNative: "video", replacementRangesByClip: {}, clipTransformsByClip: {},
        },
      });
      return { segment, setSegment, ...subtitle };
    });
    act(() => {
      loadedClipIdRef.current = "clip";
      result.current.setSegment(clipSegment);
      result.current.queueSubtitleResults([
        { clipId: "clip", isPartial: false, segments: [{ startTime: 1, endTime: 2, text: "after load" }] },
      ], true);
    });
    expect(result.current.segment?.subtitleSegments).toEqual(expect.arrayContaining([
      expect.objectContaining({ text: "after load" }),
    ]));
  });
});
