import 'fake-indexeddb/auto';
import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { subscribeSubtitleSaveInvalidation } from '@/hooks/subtitleCompletedPersistence';
import { useTimelineWorkspaceComposition } from '@/hooks/useTimelineWorkspaceComposition';
import { useProjectEditorState } from '@/hooks/useProjectEditorState';
import { idbCreateProjectBundle, idbGet, idbUpdateProjectBundle, PROJECTS_STORE } from '@/lib/projectStorage';
import type { NarrationSegment, Project, ProjectComposition, VideoSegment } from '@/types/video';

it('aborts a guarded project write when live narration mutates its source during playback', async () => {
  const id = 'subtitle-save-live-narration';
  const segment = { trimStart: 0, trimEnd: 3, trimSegments: [{ id: 'trim', startTime: 0, endTime: 3 }] } as VideoSegment;
  const backgroundConfig = { scale: 100, borderRadius: 0, backgroundType: 'solid' } as Project['backgroundConfig'];
  const composition = { mode: 'separate', focusedClipId: 'root', selectedClipId: 'root',
    clips: [{ id: 'root', role: 'root', name: 'root', duration: 3, segment, backgroundConfig, mousePositions: [] }],
  } as ProjectComposition;
  const project = { id, name: id, createdAt: 1, lastModified: 1, duration: 3, segment, backgroundConfig, mousePositions: [], composition } as Project;
  await idbCreateProjectBundle(project);
  const projectRef = { current: project as Project | null };
  const ownerRef = { current: id as string | null };
  const { result } = renderHook(() => useTimelineWorkspaceComposition({
    composition, currentProjectDataRef: projectRef, currentProjectId: id, currentProjectIdRef: ownerRef,
    duration: 3, segmentRef: { current: segment }, isPlaying: true, isPlaceholderBackedProject: false,
    videoRef: { current: { paused: false } }, editorHistory: { replaceSnapshot: vi.fn() },
    setComposition: vi.fn(), setCompositionSilently: vi.fn(), rawSetComposition: vi.fn(), rawSetSegment: vi.fn(),
    setSegment: vi.fn(), setCurrentProjectData: vi.fn(), setEditorPreviewDuration: vi.fn(),
  } as unknown as Parameters<typeof useTimelineWorkspaceComposition>[0]));
  const narration = { id: 'narration', rawAudioPath: 'speech.wav', duration: 1,
    startTime: 0, inPoint: 0, outPoint: 1, addedAt: 1 } as NarrationSegment;
  const captured = projectRef.current;
  const put = IDBObjectStore.prototype.put;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
    if (this.name === PROJECTS_STORE) {
      act(() => result.current.applyNarrationAudioSegments([narration], []));
    }
    return request;
  });
  try {
    const updated = await idbUpdateProjectBundle(id, { name: 'stale save' },
      () => projectRef.current === captured,
      (listener) => subscribeSubtitleSaveInvalidation(ownerRef, listener));
    expect(updated).toBeUndefined();
    expect((await idbGet<Project>(PROJECTS_STORE, id))?.name).toBe(id);
    expect(projectRef.current?.composition?.narrationSegments).toEqual([narration]);
  } finally {
    spy.mockRestore();
  }
});

it('aborts a guarded write when the editor segment is cleared', async () => {
  const id = 'subtitle-save-clear-segment';
  const segment = { trimStart: 0, trimEnd: 3 } as VideoSegment;
  const backgroundConfig = { scale: 100, borderRadius: 0, backgroundType: 'solid' } as Project['backgroundConfig'];
  const project = { id, name: id, createdAt: 1, lastModified: 1, segment, backgroundConfig, mousePositions: [] } as Project;
  await idbCreateProjectBundle(project);
  const { result } = renderHook(() => useProjectEditorState());
  const ownerRef = result.current.currentProjectIdRef;
  ownerRef.current = id;
  act(() => result.current.rawSetSegment(segment));
  const put = IDBObjectStore.prototype.put;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
    if (this.name === PROJECTS_STORE) act(() => result.current.rawSetSegment(null));
    return request;
  });
  try {
    const updated = await idbUpdateProjectBundle(id, { name: 'stale save' }, () => true,
      (listener) => subscribeSubtitleSaveInvalidation(ownerRef, listener));
    expect(updated).toBeUndefined();
    expect((await idbGet<Project>(PROJECTS_STORE, id))?.name).toBe(id);
  } finally {
    spy.mockRestore();
  }
});
