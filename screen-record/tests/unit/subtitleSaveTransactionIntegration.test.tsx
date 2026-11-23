import 'fake-indexeddb/auto';
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useSubtitleResultApplication } from '@/hooks/useSubtitleResultApplication';
import { useProjectPersistence } from '@/hooks/useProjectPersistence';
import { registerSubtitleSaveEditor, retainedSubtitleSave } from '@/hooks/subtitleCompletedPersistence';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import { DEFAULT_WEBCAM_CONFIG } from '@/lib/webcam';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { idbCreateProjectBundle, idbGet, PROJECTS_STORE } from '@/lib/projectStorage';
import { projectManager } from '@/lib/projectManager';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(async () => null), logToHost: vi.fn() }));

it('retries the complete hook-to-IndexedDB save and acknowledges only a committed result', async () => {
  const id = 'subtitle-save-full-transaction';
  const original = { trimStart: 0, trimEnd: 3, trimSegments: [{ id: 'trim', startTime: 0, endTime: 3 }], subtitleSegments: [] } as VideoSegment;
  const updated = { ...original, subtitleSegments: [{ id: 'cue', startTime: 1, endTime: 2, text: 'saved', style: defaultSubtitleStyle() }] } as VideoSegment;
  const makeComposition = (segment: VideoSegment) => ({ mode: 'separate', selectedClipId: 'root', focusedClipId: 'root',
    clips: [{ id: 'root', role: 'root', name: 'root', duration: 3, segment, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
      webcamConfig: DEFAULT_WEBCAM_CONFIG, mousePositions: [], rawVideoPath: 'video.mp4' }],
  } as ProjectComposition);
  const project = { id, name: id, createdAt: 1, lastModified: 1, duration: 3, segment: original,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, webcamConfig: DEFAULT_WEBCAM_CONFIG,
    rawVideoPath: 'video.mp4', mousePositions: [], composition: makeComposition(original) } as Project;
  await idbCreateProjectBundle(project);
  projectManager.setActiveProjectId(id);
  const ownerRef = { current: id as string | null };
  const projectRef = { current: { ...project, segment: updated, composition: makeComposition(updated) } as Project | null };
  const segmentRef = { current: updated as VideoSegment | null };
  const unregister = registerSubtitleSaveEditor(ownerRef);
  let writes = 0;
  const put = IDBObjectStore.prototype.put;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    if (this.name === PROJECTS_STORE && (value as Project).id === id && ++writes === 1) throw new Error('temporary write failure');
    return key === undefined ? put.call(this, value) : put.call(this, value, key);
  });
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  try {
    const { result, unmount } = renderHook(() => {
      const { persistCurrentProjectNow } = useProjectPersistence({
        currentProjectId: id, currentProjectIdRef: ownerRef, projects: { refreshProjectEntry: async () => {} },
        currentVideo: null, currentAudio: null, currentMicAudio: null, currentWebcamVideo: null,
        loadedClipId: 'root', currentProjectData: projectRef.current, currentProjectDataRef: projectRef,
        segment: updated, composition: projectRef.current?.composition ?? null,
        backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [], generateThumbnail: () => undefined,
        duration: 3, currentRecordingMode: 'withoutCursor', currentRawVideoPath: 'video.mp4',
        currentRawMicAudioPath: '', currentRawWebcamVideoPath: '', webcamConfig: DEFAULT_WEBCAM_CONFIG,
        canvasRef: { current: null }, isProjectTransitionRef: { current: false }, isSwitchingCompositionClipRef: { current: false },
        loadClipAssets: async () => null, commitPersistedComposition: vi.fn(),
      });
      return useSubtitleResultApplication({
        segment: updated, segmentRef, currentProjectDataRef: projectRef, currentProjectIdRef: ownerRef,
        projectResetKey: id, loadedClipIdRef: { current: 'root' }, setSegment: vi.fn(), setSegmentSilently: vi.fn(),
        composition: projectRef.current?.composition ?? null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
        jobId: 'save-integration', jobContext: { projectKey: id, indicator: {} as never, sourceTypeForNative: 'video',
          replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: {} },
        persistProject: persistCurrentProjectNow,
      });
    });
    act(() => result.current.markCompletedJobForPersist());
    unmount();
    await waitFor(async () => {
      const stored = await idbGet<Project>(PROJECTS_STORE, id);
      expect(stored?.segment.subtitleSegments).toEqual(expect.arrayContaining([expect.objectContaining({ text: 'saved' })]));
      expect(retainedSubtitleSave()).toBeNull();
    }, { timeout: 5000 });
    expect(writes).toBeGreaterThanOrEqual(2);
  } finally {
    spy.mockRestore();
    unregister();
    projectManager.setActiveProjectId(null);
    vi.restoreAllMocks();
  }
});
