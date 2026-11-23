import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useProjectPersistence } from '@/hooks/useProjectPersistence';
import { useSubtitleResultApplication } from '@/hooks/useSubtitleResultApplication';
import { registerSubtitleSaveEditor } from '@/hooks/subtitleCompletedPersistence';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import { DEFAULT_WEBCAM_CONFIG } from '@/lib/webcam';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';

const manager = vi.hoisted(() => ({
  createEditorWriteIntent: vi.fn(() => ({})), isEditorWriteIntentLatest: vi.fn(() => true),
  loadProject: vi.fn(), updateProject: vi.fn(),
}));
vi.mock('@/lib/projectManager', () => ({ projectManager: manager }));

afterEach(() => { vi.useRealTimers(); vi.clearAllMocks(); });

it('retries a completed save after a synchronous project edit while clip assets load', async () => {
  vi.useFakeTimers();
  const segment = { trimStart: 0, trimEnd: 5, trimSegments: [{ id: 'trim', startTime: 0, endTime: 5 }],
    subtitleSegments: [] } as VideoSegment;
  const composition = { mode: 'separate', selectedClipId: 'root', focusedClipId: 'root',
    clips: [{ id: 'root', role: 'root', name: 'Root', duration: 5, segment,
      backgroundConfig: DEFAULT_BACKGROUND_CONFIG, webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [] }] } as ProjectComposition;
  const original = { id: 'p', name: 'Project', createdAt: 1, lastModified: 1, duration: 5,
    segment, composition, backgroundConfig: DEFAULT_BACKGROUND_CONFIG, webcamConfig: DEFAULT_WEBCAM_CONFIG,
    mousePositions: [], rawVideoPath: 'video.mp4' } as Project;
  const projectRef = { current: original as Project | null };
  const idRef = { current: 'p' as string | null };
  const unregister = registerSubtitleSaveEditor(idRef);
  let finishLoad!: () => void;
  const loadBarrier = new Promise<void>((resolve) => { finishLoad = resolve; });
  let loads = 0;
  const commitPersistedComposition = vi.fn();
  const loadClipAssets = vi.fn(async () => { if (++loads === 1) await loadBarrier; return null; });
  manager.loadProject.mockImplementation(async () => projectRef.current);
  manager.updateProject.mockImplementation(async (_id, _updates, _intent, canCommit) => canCommit?.() ?? true);
  const { result, unmount } = renderHook(() => {
    const { persistCurrentProjectNow } = useProjectPersistence({
      currentProjectId: 'p', currentProjectIdRef: idRef, projects: { refreshProjectEntry: async () => {} },
      currentVideo: null, currentAudio: null, currentMicAudio: null, currentWebcamVideo: null,
      loadedClipId: 'root', currentProjectData: original, currentProjectDataRef: projectRef,
      segment, composition, backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
      generateThumbnail: () => null, duration: 5, currentRecordingMode: 'withoutCursor',
      currentRawVideoPath: 'video.mp4', currentRawMicAudioPath: '', currentRawWebcamVideoPath: '',
      webcamConfig: DEFAULT_WEBCAM_CONFIG, canvasRef: { current: null },
      isProjectTransitionRef: { current: false }, isSwitchingCompositionClipRef: { current: false },
      loadClipAssets, commitPersistedComposition,
    });
    return useSubtitleResultApplication({
      segment, segmentRef: { current: segment }, currentProjectDataRef: projectRef, currentProjectIdRef: idRef,
      projectResetKey: 'p', loadedClipIdRef: { current: 'root' }, setSegment: vi.fn(), setSegmentSilently: vi.fn(),
      composition, setComposition: vi.fn(), setCompositionSilently: vi.fn(), jobId: 'admission-job',
      jobContext: { projectKey: 'p', indicator: {} as never, sourceTypeForNative: 'video',
        replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: {} },
      persistProject: persistCurrentProjectNow,
    });
  });
  act(() => result.current.markCompletedJobForPersist());
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  const audio = { id: 'audio', rawAudioPath: 'audio.wav', name: 'Audio', duration: 1,
    startTime: 0, inPoint: 0, outPoint: 1, addedAt: 1 };
  projectRef.current = { ...original, composition: { ...composition, audioSegments: [audio] } };
  await act(async () => { finishLoad(); await loadBarrier; await Promise.resolve(); });
  expect(manager.updateProject).not.toHaveBeenCalled();
  await act(async () => { await vi.advanceTimersByTimeAsync(251); });
  expect(manager.updateProject).toHaveBeenCalledOnce();
  expect(manager.updateProject.mock.calls[0][1].composition.audioSegments).toEqual([audio]);
  // The write-back goes through the history-neutral setter so undo/redo survive autosave.
  expect(commitPersistedComposition).toHaveBeenCalledWith(manager.updateProject.mock.calls[0][1].composition);
  unmount();
  unregister();
});
