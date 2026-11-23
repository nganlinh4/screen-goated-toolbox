import { useRef } from "react";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAppHistoryState } from "@/hooks/useAppHistoryState";
import { useBackgroundConfig } from "@/hooks/useBackgroundConfig";
import { useSynchronizedState } from "@/hooks/useSynchronizedState";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import type { BackgroundConfig, Project, ProjectComposition, VideoSegment, WebcamConfig } from "@/types/video";

function useVisualState() {
  const [backgroundConfig, setBackgroundConfigState, backgroundConfigRef] =
    useSynchronizedState<BackgroundConfig>(() => ({ ...DEFAULT_BACKGROUND_CONFIG }));
  const [webcamConfig, rawSetWebcamConfig, webcamConfigRef] =
    useSynchronizedState<WebcamConfig>(() => ({ ...DEFAULT_WEBCAM_CONFIG }));
  const segment = useRef({ trimStart: 0, trimEnd: 10 } as VideoSegment);
  const project = useRef<Project>({
    id: "project", name: "Project", createdAt: 1, lastModified: 1, duration: 10,
    segment: segment.current, backgroundConfig: { ...DEFAULT_BACKGROUND_CONFIG },
    webcamConfig: { ...DEFAULT_WEBCAM_CONFIG }, mousePositions: [],
  });
  const projectRef = project as React.MutableRefObject<Project | null>;
  const transitionRef = useRef(false);
  const clipTransitionRef = useRef(false);
  const { setBackgroundConfig: rawSetBackgroundConfig, applyLoadedBackgroundConfig } =
    useBackgroundConfig({ initialConfig: backgroundConfig, setBackgroundConfigState,
      isProjectTransitionRef: transitionRef });
  const history = useAppHistoryState({
    backgroundConfig, backgroundConfigRef, webcamConfig, webcamConfigRef,
    composition: project.current.composition ?? null, currentProjectDataRef: projectRef,
    currentRawMicAudioPath: "", currentRawVideoPath: "", currentRawWebcamVideoPath: "",
    currentRecordingMode: "withoutCursor", duration: 10, handleProjectRawVideoPathChange: vi.fn(),
    isPlaying: false, isPlayingRef: { current: false }, isProjectTransitionRef: transitionRef, pendingSilentSegmentRef: { current: null },
    pendingSilentSegmentTimerRef: { current: null }, rawSetBackgroundConfig,
    rawSetComposition: vi.fn(), rawSetCurrentRawMicAudioPath: vi.fn(), rawSetCurrentRawVideoPath: vi.fn(),
    rawSetCurrentRawWebcamVideoPath: vi.fn(), rawSetCurrentRecordingMode: vi.fn(), rawSetSegment: vi.fn(),
    rawSetWebcamConfig, segment: segment.current, segmentRef: segment, setBackgroundConfigState,
    setCurrentProjectData: vi.fn(), setLastRawSavedPath: vi.fn(), setPreviewDuration: vi.fn(),
  });
  history.bindVisualTransition(clipTransitionRef);
  return { history, backgroundConfig, webcamConfig, rawSetBackgroundConfig,
    rawSetWebcamConfig, applyLoadedBackgroundConfig, projectRef, transitionRef, clipTransitionRef };
}

describe("visual state handoff", () => {
  it("rejects user background and webcam updaters while the next clip is loading", () => {
    const { result } = renderHook(useVisualState);
    const background = vi.fn((previous: BackgroundConfig) => ({ ...previous, borderRadius: 90 }));
    const webcam = vi.fn((previous: WebcamConfig) => ({ ...previous, mirror: true }));
    act(() => {
      result.current.clipTransitionRef.current = true;
      result.current.history.setBackgroundConfig(background);
      result.current.history.setWebcamConfig(webcam);
    });
    expect(background).not.toHaveBeenCalled();
    expect(webcam).not.toHaveBeenCalled();
    expect(result.current.backgroundConfig.borderRadius).toBe(DEFAULT_BACKGROUND_CONFIG.borderRadius);
    expect(result.current.webcamConfig.mirror).toBe(false);
    expect(result.current.history.canUndo).toBe(false);
  });
  it("preserves silent background and webcam changes across rapid edits and undo/redo", () => {
    const { result } = renderHook(useVisualState);
    act(() => {
      result.current.rawSetBackgroundConfig((previous) => ({ ...previous, backgroundType: "custom" }));
      result.current.rawSetWebcamConfig((previous) => ({ ...previous, mirror: true }));
      result.current.history.setBackgroundConfig((previous) => ({ ...previous, borderRadius: 23 }));
      result.current.history.setBackgroundConfig((previous) => ({ ...previous, scale: previous.scale + 5 }));
      result.current.history.setWebcamConfig((previous) => ({ ...previous, visible: true }));
    });
    expect(result.current.backgroundConfig).toMatchObject({ backgroundType: "custom", borderRadius: 23, scale: DEFAULT_BACKGROUND_CONFIG.scale + 5 });
    expect(result.current.webcamConfig).toMatchObject({ mirror: true, visible: true });
    act(() => result.current.history.undo());
    expect(result.current.webcamConfig).toMatchObject({ mirror: true, visible: false });
    act(() => result.current.history.redo());
    expect(result.current.webcamConfig).toMatchObject({ mirror: true, visible: true });
  });

  it("uses a loaded clip as the baseline and preserves the root configuration", () => {
    const { result } = renderHook(useVisualState);
    const root = result.current.projectRef.current!;
    root.composition = {
      mode: "separate", focusedClipId: "clip", selectedClipId: "clip", clips: [{ id: "root" }, { id: "clip" }],
    } as ProjectComposition;
    act(() => {
      result.current.applyLoadedBackgroundConfig({ ...DEFAULT_BACKGROUND_CONFIG, borderRadius: 40 });
      result.current.rawSetWebcamConfig({ ...DEFAULT_WEBCAM_CONFIG, mirror: true });
      result.current.history.beginBatch();
      result.current.history.setBackgroundConfig((previous) => ({ ...previous, borderRadius: previous.borderRadius + 1 }));
      result.current.history.setWebcamConfig((previous) => ({ ...previous, visible: true }));
      result.current.history.commitBatch();
    });
    expect(result.current.backgroundConfig.borderRadius).toBe(41);
    expect(result.current.projectRef.current?.backgroundConfig.borderRadius).toBe(DEFAULT_BACKGROUND_CONFIG.borderRadius);
    expect(result.current.projectRef.current?.webcamConfig.mirror).toBe(false);
    act(() => result.current.history.undo());
    expect(result.current.backgroundConfig.borderRadius).toBe(40);
    expect(result.current.webcamConfig).toMatchObject({ mirror: true, visible: false });
    expect(result.current.projectRef.current?.backgroundConfig.borderRadius).toBe(DEFAULT_BACKGROUND_CONFIG.borderRadius);
    expect(result.current.projectRef.current?.webcamConfig.mirror).toBe(false);
  });

  it.each(['project', 'clip'] as const)("ignores tracked edits during a %s handoff while accepting the loaded state", (owner) => {
    const { result } = renderHook(useVisualState);
    act(() => {
      (owner === 'project' ? result.current.transitionRef : result.current.clipTransitionRef).current = true;
      result.current.history.setBackgroundConfig((previous) => ({ ...previous, borderRadius: 90 }));
      result.current.history.setWebcamConfig((previous) => ({ ...previous, mirror: true }));
      result.current.applyLoadedBackgroundConfig({ ...DEFAULT_BACKGROUND_CONFIG, borderRadius: 30 });
      result.current.rawSetWebcamConfig({ ...DEFAULT_WEBCAM_CONFIG, visible: true });
    });
    expect(result.current.backgroundConfig.borderRadius).toBe(30);
    expect(result.current.webcamConfig).toMatchObject({ visible: true, mirror: false });
    expect(result.current.history.canUndo).toBe(false);
  });

  it('blocks undo and redo during clip loading and resumes them after the handoff', () => {
    const { result } = renderHook(useVisualState);
    act(() => result.current.history.setBackgroundConfig((previous) => ({ ...previous, borderRadius: 17 })));
    act(() => { result.current.clipTransitionRef.current = true; result.current.history.undo(); });
    expect(result.current.backgroundConfig.borderRadius).toBe(17);
    expect(result.current.history.canUndo).toBe(true);
    act(() => { result.current.clipTransitionRef.current = false; result.current.history.undo(); });
    expect(result.current.backgroundConfig.borderRadius).toBe(DEFAULT_BACKGROUND_CONFIG.borderRadius);
    act(() => { result.current.clipTransitionRef.current = true; result.current.history.redo(); });
    expect(result.current.backgroundConfig.borderRadius).toBe(DEFAULT_BACKGROUND_CONFIG.borderRadius);
    act(() => { result.current.clipTransitionRef.current = false; result.current.history.redo(); });
    expect(result.current.backgroundConfig.borderRadius).toBe(17);
  });
});
