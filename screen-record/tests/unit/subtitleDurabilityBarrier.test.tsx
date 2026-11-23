import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { clearResumableRun, readResumableRun, saveResumableRun, takeResumableStarts } from '@/hooks/resumableJobRegistry';
import { clearSubtitleRecovery, discardPendingSubtitleRecovery, getSubtitleRecovery, retrySubtitleRecovery } from '@/hooks/subtitleRecoveryJournal';
import { captureSubtitleSourceOwners } from '@/hooks/subtitleSourceOwnership';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { VideoSegment } from '@/types/video';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);
const segment = { trimStart: 0, trimEnd: 4, trimSegments: [{ id: 'trim', startTime: 0, endTime: 4 }], subtitleSegments: [] } as VideoSegment;
const plan = { clips: [{ clipId: 'root', sourcePath: 'video.mp4', sourceDuration: 4, trimSegments: [] }],
  sourceTypeForNative: 'video', clipTransformsByClip: {}, replacementRangesByClip: {}, indicator: {} as never } as SubtitleGenerationPlan;
const context = { projectKey: 'p', indicator: {} as never, sourceTypeForNative: 'video' as const,
  replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: captureSubtitleSourceOwners(plan, segment, null) };

afterEach(async () => {
  clearResumableRun('subtitle-generation');
  takeResumableStarts((key) => key === 'subtitle-generation');
  await clearSubtitleRecovery('durability-job').catch(() => {});
  vi.restoreAllMocks();
});

it('keeps the worker job unacknowledged until the recovery transaction commits', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? Promise.resolve({ state: 'completed', message: 'done', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'generated' }] }] })
    : Promise.resolve());
  let failWrite = true;
  const put = IDBObjectStore.prototype.put;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    if (key === 'subtitle-completed-recovery-v1' && failWrite) throw new DOMException('quota', 'QuotaExceededError');
    return key === undefined ? put.call(this, value) : put.call(this, value, key);
  });
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'p', jobId: 'durability-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setSegment = vi.fn();
  const { result, unmount } = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusSavingRecovery: 'Protecting', subtitleStatusCancelUnconfirmed: 'May still be running' } as never,
    projectResetKey: 'p', segment, setSegment, setSegmentSilently: setSegment,
    composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 4, setActivePanel: vi.fn(),
  }));
  await waitFor(() => expect(getSubtitleRecovery().error).toContain('quota'));
  expect(getSubtitleRecovery().pending?.jobId).toBe('durability-job');
  expect(readResumableRun('subtitle-generation')).toBeDefined();
  expect(result.current.isGeneratingSubtitles).toBe(true);
  failWrite = false;
  act(() => retrySubtitleRecovery());
  await waitFor(() => expect(getSubtitleRecovery().record?.jobId).toBe('durability-job'));
  await waitFor(() => expect(result.current.isGeneratingSubtitles).toBe(false));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
  spy.mockRestore();
  unmount();
});

it('retains the old project recovery without touching a new project after a switch', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? Promise.resolve({ state: 'completed', message: 'done', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'old project' }] }] })
    : Promise.resolve());
  let failWrite = true;
  const put = IDBObjectStore.prototype.put;
  const spy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (value, key) {
    if (key === 'subtitle-completed-recovery-v1' && failWrite) throw new DOMException('quota', 'QuotaExceededError');
    return key === undefined ? put.call(this, value) : put.call(this, value, key);
  });
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'p', jobId: 'durability-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const props = { t: { subtitleStatusSavingRecovery: 'Protecting' } as never, segment,
    setSegment: setter, setSegmentSilently: setter, composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 4, setActivePanel: vi.fn() };
  const { rerender, unmount } = renderHook(({ owner }: { owner: string }) => useSubtitleGeneration({ ...props, projectResetKey: owner }),
    { initialProps: { owner: 'p' } });
  await waitFor(() => expect(getSubtitleRecovery().error).toContain('quota'));
  const callsAtSwitch = setter.mock.calls.length;
  rerender({ owner: 'other' });
  failWrite = false;
  act(() => retrySubtitleRecovery());
  await waitFor(() => expect(getSubtitleRecovery().record?.jobId).toBe('durability-job'));
  expect(getSubtitleRecovery().record?.projectId).toBe('p');
  expect(setter).toHaveBeenCalledTimes(callsAtSwitch);
  spy.mockRestore();
  unmount();
});

it('releases the completed worker barrier after explicit pending recovery discard', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? Promise.resolve({ state: 'completed', message: 'done', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'generated' }] }] })
    : Promise.resolve());
  vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => { throw new DOMException('quota', 'QuotaExceededError'); });
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'p', jobId: 'durability-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const { result, unmount } = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusSavingRecovery: 'Protecting', subtitleStatusCancelled: 'Cancelled' } as never,
    projectResetKey: 'p', segment, setSegment: setter, setSegmentSilently: setter,
    composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 4, setActivePanel: vi.fn(),
  }));
  await waitFor(() => expect(getSubtitleRecovery().error).toContain('quota'));
  const pending = getSubtitleRecovery().pending!;
  await act(async () => { expect(await discardPendingSubtitleRecovery(pending)).toBe(true); });
  await waitFor(() => expect(result.current.isGeneratingSubtitles).toBe(false));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
  expect(getSubtitleRecovery().pending).toBeNull();
  expect(getSubtitleRecovery().record).toBeNull();
  unmount();
});

it('retires only the original resumable job when pending recovery is discarded after unmount', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? Promise.resolve({ state: 'completed', message: 'done', progress: 1, activeClipId: null,
      totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
      results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'generated' }] }] })
    : Promise.resolve());
  vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(() => { throw new DOMException('quota', 'QuotaExceededError'); });
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'p', jobId: 'durability-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const { unmount } = renderHook(() => useSubtitleGeneration({
    t: { subtitleStatusSavingRecovery: 'Protecting' } as never, projectResetKey: 'p', segment,
    setSegment: setter, setSegmentSilently: setter, composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 4, setActivePanel: vi.fn(),
  }));
  await waitFor(() => expect(getSubtitleRecovery().error).toContain('quota'));
  const pending = getSubtitleRecovery().pending!;
  unmount();
  expect(await discardPendingSubtitleRecovery(pending)).toBe(true);
  await waitFor(() => expect(readResumableRun('subtitle-generation')).toBeUndefined());
});
