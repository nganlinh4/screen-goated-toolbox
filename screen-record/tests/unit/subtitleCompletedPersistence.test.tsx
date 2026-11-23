import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useSubtitleResultApplication } from '@/hooks/useSubtitleResultApplication';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { registerSubtitleSaveEditor, retainedSubtitleSave } from '@/hooks/subtitleCompletedPersistence';
import { useProjectEditorState } from '@/hooks/useProjectEditorState';
import { setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import type { PersistOptions } from '@/hooks/useSequenceComposition';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), logToHost: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);

const segment = { trimStart: 0, trimEnd: 3, subtitleSegments: [] } as VideoSegment;
const composition = { mode: 'separate', selectedClipId: 'root', focusedClipId: 'root',
  clips: [{ id: 'root', duration: 3, segment }] } as ProjectComposition;
const context = { projectKey: 'p', indicator: {} as never, sourceTypeForNative: 'video' as const,
  replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: {} };

let unregisterEditor: (() => void) | undefined;
afterEach(() => { unregisterEditor?.(); unregisterEditor = undefined; vi.useRealTimers(); vi.restoreAllMocks(); });

function harness(save: (options?: PersistOptions) => Promise<void>, idRef = { current: 'p' as string | null }) {
  unregisterEditor?.();
  unregisterEditor = registerSubtitleSaveEditor(idRef);
  const projectRef = { current: { id: 'p', composition, segment } as Project | null };
  const hook = renderHook(() => useSubtitleResultApplication({
    segment, segmentRef: { current: segment }, currentProjectDataRef: projectRef,
    currentProjectIdRef: idRef, projectResetKey: 'p', loadedClipIdRef: { current: 'root' },
    setSegment: vi.fn(), setSegmentSilently: vi.fn(), composition,
    setComposition: vi.fn(), setCompositionSilently: vi.fn(), jobId: `job-${Math.random()}`,
    jobContext: context, persistProject: save,
  }));
  return { ...hook, idRef, projectRef };
}

it('retries an unacknowledged completed save after its hook unmounts', async () => {
  vi.useFakeTimers();
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  let attempts = 0;
  const save = vi.fn(async (options?: PersistOptions) => {
    attempts += 1;
    if (attempts === 1) throw new Error('temporary failure');
    options?.onCommitted?.();
  });
  const hook = harness(save);
  act(() => hook.result.current.markCompletedJobForPersist());
  hook.unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(save).toHaveBeenCalledOnce();
  expect(save.mock.calls[0][0]?.expectedProjectId).toBe('p');
  await act(async () => { await vi.advanceTimersByTimeAsync(251); });
  expect(save).toHaveBeenCalledTimes(2);
});

it('retries with the newest project after a synchronous edit before commit', async () => {
  vi.useFakeTimers();
  let releaseFirst!: () => void;
  const firstAttempt = new Promise<void>((resolve) => { releaseFirst = resolve; });
  let attempts = 0;
  const committed: ProjectComposition[] = [];
  const save = vi.fn(async (options?: PersistOptions) => {
    if (++attempts === 1) await firstAttempt;
    if (!options?.canCommit?.()) return;
    committed.push(options.compositionOverride!);
    options.onCommitted?.();
  });
  const hook = harness(save);
  act(() => hook.result.current.markCompletedJobForPersist());
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  const added = { id: 'music', rawAudioPath: 'song.wav', name: 'song', duration: 1,
    startTime: 0, inPoint: 0, outPoint: 1, addedAt: 1 };
  hook.projectRef.current = { ...hook.projectRef.current!, composition: { ...composition, audioSegments: [added] } };
  await act(async () => { releaseFirst(); await firstAttempt; await Promise.resolve(); });
  expect(committed).toEqual([]);
  await act(async () => { await vi.advanceTimersByTimeAsync(251); });
  expect(committed).toHaveLength(1);
  expect(committed[0].audioSegments).toEqual([added]);
  hook.unmount();
});

it('uses the latest same-project composition when retrying', async () => {
  vi.useFakeTimers();
  const save = vi.fn(async (options?: PersistOptions) => { options?.onCommitted?.(); });
  const hook = harness(save);
  act(() => hook.result.current.markCompletedJobForPersist());
  const added = { id: 'music', rawAudioPath: 'song.wav', name: 'song', duration: 1,
    startTime: 0, inPoint: 0, outPoint: 1, addedAt: 1 };
  hook.projectRef.current = { ...hook.projectRef.current!, composition: { ...composition, audioSegments: [added] } };
  hook.unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(save.mock.calls[0][0]?.compositionOverride?.audioSegments).toEqual([added]);
});

it('wakes an unacknowledged save when the same owner remounts', async () => {
  vi.useFakeTimers();
  let attempts = 0;
  const save = vi.fn(async (options?: PersistOptions) => {
    attempts += 1;
    // A skipped persistence path resolves but did not acknowledge a commit.
    if (attempts > 1) options?.onCommitted?.();
  });
  const first = harness(save);
  act(() => first.result.current.markCompletedJobForPersist());
  first.unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(save).toHaveBeenCalledOnce();
  const second = harness(save, first.idRef);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(save).toHaveBeenCalledTimes(2);
  second.unmount();
});

it('keeps a hung save single-flight, retains its snapshot, and blocks another generation', async () => {
  vi.useFakeTimers();
  let finish!: () => void;
  const stuck = new Promise<void>((resolve) => { finish = resolve; });
  const save = vi.fn(async (options?: PersistOptions) => { await stuck; options?.onCommitted?.(); });
  const hook = harness(save);
  act(() => hook.result.current.markCompletedJobForPersist());
  hook.unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(30_001); });
  expect(save).toHaveBeenCalledOnce();
  expect(retainedSubtitleSave()).toEqual(expect.objectContaining({ projectId: 'p', segment }));
  const generation = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusSavePending: 'Saving', subtitleStatusSaveUnconfirmed: 'Save unconfirmed' } as never,
    projectResetKey: 'p', segment, segmentRef: { current: segment }, currentProjectDataRef: hook.projectRef,
    currentProjectIdRef: hook.idRef, currentRawVideoPathRef: { current: 'video.mp4' }, currentRawMicAudioPathRef: { current: '' },
    setSegment: vi.fn(), setSegmentSilently: vi.fn(), composition, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: 'root' }, currentRawVideoPath: 'video.mp4', currentRawMicAudioPath: '',
    duration: 3, setActivePanel: vi.fn(),
  }));
  expect(generation.result.current.subtitleStatusMessage).toBe('Save unconfirmed');
  await act(async () => { await generation.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke).not.toHaveBeenCalledWith('start_subtitle_generation', expect.anything());
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(save).toHaveBeenCalledOnce();
  await act(async () => { finish(); await stuck; await Promise.resolve(); });
  expect(retainedSubtitleSave()).toBeNull();
  generation.unmount();
});

it('retains the snapshot and never redirects it after switch, reopen, and editor unmount', async () => {
  vi.useFakeTimers();
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  const oldEditor = renderHook(() => useProjectEditorState());
  const oldOwner = oldEditor.result.current.currentProjectIdRef;
  oldOwner.current = 'p';
  const save = vi.fn(async (options?: PersistOptions) => { options?.onCommitted?.(); });
  const hook = harness(save, oldOwner);
  act(() => hook.result.current.markCompletedJobForPersist());
  hook.unmount();
  setNarrationProjectId(oldOwner, 'other');
  setNarrationProjectId(oldOwner, 'p');
  oldEditor.unmount();
  const newEditor = renderHook(() => useProjectEditorState());
  newEditor.result.current.currentProjectIdRef.current = 'other';
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(save).not.toHaveBeenCalled();
  expect(retainedSubtitleSave()).toEqual(expect.objectContaining({ projectId: 'p', segment, composition }));
  newEditor.unmount();
});
