import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { clearResumableRun, saveResumableRun } from '@/hooks/resumableJobRegistry';
import { clearSubtitleRecovery, getSubtitleRecovery, retainSubtitleRecovery } from '@/hooks/subtitleRecoveryJournal';
import { captureSubtitleSourceOwners } from '@/hooks/subtitleSourceOwnership';
import { getRetiringSubtitleGenerations, hasRetiringSubtitleGeneration, releaseRetiringSubtitleGeneration, retireSubtitleGeneration } from '@/hooks/retiredSubtitleGeneration';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { VideoSegment } from '@/types/video';
import { APP_META_STORE, openProjectDB } from '@/lib/projectStorage';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);
const segment = { trimStart: 0, trimEnd: 4, trimSegments: [{ id: 'trim', startTime: 0, endTime: 4 }], subtitleSegments: [] } as VideoSegment;
const plan = { clips: [{ clipId: 'root', sourcePath: 'video.mp4', sourceDuration: 4, trimSegments: [] }],
  sourceTypeForNative: 'video', clipTransformsByClip: {}, replacementRangesByClip: {}, indicator: {} as never } as SubtitleGenerationPlan;
const context = { projectKey: 'old', indicator: {} as never, sourceTypeForNative: 'video' as const,
  replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: captureSubtitleSourceOwners(plan, segment, null) };
const completed = { state: 'completed', message: 'done', progress: 1, activeClipId: null, totalClips: 1,
  completedClips: 1, resultsRevision: 1, skipped: [],
  results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 1, endTime: 2, text: 'old result' }] }] };

afterEach(async () => {
  clearResumableRun('subtitle-generation');
  getRetiringSubtitleGenerations().forEach(releaseRetiringSubtitleGeneration);
  await clearSubtitleRecovery('retired-job').catch(() => {});
  const db = await openProjectDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(APP_META_STORE, 'readwrite');
    tx.objectStore(APP_META_STORE).delete('subtitle-recovery-acknowledged-v1');
    tx.oncomplete = () => resolve();
    tx.onabort = () => reject(tx.error);
  });
  vi.restoreAllMocks();
});

it('journals an atomically returned completed snapshot after switching while status is delayed', async () => {
  let releaseStatus!: (value: typeof completed) => void;
  const delayedStatus = new Promise<typeof completed>((resolve) => { releaseStatus = resolve; });
  let releaseCancel!: (value: typeof completed) => void;
  const delayedCancel = new Promise<typeof completed>((resolve) => { releaseCancel = resolve; });
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status' ? delayedStatus
    : command === 'cancel_subtitle_generation' ? delayedCancel : Promise.resolve());
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const props = { t: {} as never, segment, setSegment: setter, setSegmentSilently: setter,
    composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(), activeClipId: 'root',
    loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4', currentRawMicAudioPath: '', duration: 4,
    setActivePanel: vi.fn() };
  const hook = renderHook(({ owner }: { owner: string }) => useSubtitleGeneration({ ...props, projectResetKey: owner }),
    { initialProps: { owner: 'old' } });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.anything()));
  hook.rerender({ owner: 'new' });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'retired-job' }));
  await act(async () => { releaseStatus(completed); releaseCancel(completed); });
  await waitFor(() => expect(getSubtitleRecovery().record?.jobId).toBe('retired-job'));
  expect(getSubtitleRecovery().record?.projectId).toBe('old');
  expect(getSubtitleRecovery().record?.results[0].segments[0].text).toBe('old result');
  expect(setter).not.toHaveBeenCalled();
  hook.unmount();
});

it('retries an unconfirmed retirement and retains the eventual completed result', async () => {
  ipc.invoke.mockReset().mockRejectedValueOnce(new Error('IPC unavailable')).mockResolvedValueOnce(completed);
  retireSubtitleGeneration({ projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  expect(hasRetiringSubtitleGeneration()).toBe(true);
  await waitFor(() => expect(getSubtitleRecovery().record?.jobId).toBe('retired-job'), { timeout: 3000 });
  await waitFor(() => expect(hasRetiringSubtitleGeneration()).toBe(false));
  expect(ipc.invoke).toHaveBeenCalledTimes(2);
});

it('does not resurrect a recovery acknowledged while its retirement response was in flight', async () => {
  let releaseCancel!: (value: typeof completed) => void;
  ipc.invoke.mockReset().mockReturnValue(new Promise<typeof completed>((resolve) => { releaseCancel = resolve; }));
  retireSubtitleGeneration({ projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  await retainSubtitleRecovery({ version: 1, jobId: 'retired-job', projectId: 'old', context, results: completed.results });
  expect(await clearSubtitleRecovery('retired-job')).toBe(true);
  releaseCancel(completed);
  await waitFor(() => expect(hasRetiringSubtitleGeneration()).toBe(false));
  expect(getSubtitleRecovery().record).toBeNull();
});

it('does not resurrect a recovery when the retired response races its deletion transaction', async () => {
  let releaseCancel!: (value: typeof completed) => void;
  ipc.invoke.mockReset().mockReturnValue(new Promise<typeof completed>((resolve) => { releaseCancel = resolve; }));
  await retainSubtitleRecovery({ version: 1, jobId: 'retired-job', projectId: 'old', context, results: completed.results });
  retireSubtitleGeneration({ projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const deleting = clearSubtitleRecovery('retired-job');
  releaseCancel(completed);
  expect(await deleting).toBe(true);
  await waitFor(() => expect(hasRetiringSubtitleGeneration()).toBe(false));
  expect(getSubtitleRecovery().record).toBeNull();
  expect(getSubtitleRecovery().pending).toBeNull();
});

it('does not recreate an acknowledged recovery when a stale ticket is replayed after restart', async () => {
  const jobId = 'ticket-0123456789abcdef0123456789abcdef-transcription-1';
  const ticket = { projectEpoch: 0, projectKey: 'old', jobId, jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' };
  const tickets = await import('@/hooks/subtitleStartTicket');
  tickets.saveSubtitleStartTicket(ticket);
  await retainSubtitleRecovery({ version: 1, jobId, projectId: 'old', context, results: completed.results });
  expect(await clearSubtitleRecovery(jobId)).toBe(true);
  // Simulate an old ticket surviving a crash after the journal transaction.
  tickets.saveSubtitleStartTicket(ticket);
  vi.resetModules();
  const restarted = await import('@/hooks/retiredSubtitleGeneration');
  const restartedJournal = await import('@/hooks/subtitleRecoveryJournal');
  ipc.invoke.mockReset().mockResolvedValue(completed);
  expect(await restarted.retireSubtitleGeneration(ticket)).toBe('cancelled');
  await restartedJournal.loadSubtitleRecovery(true);
  expect(restartedJournal.getSubtitleRecovery().record).toBeNull();
  expect(tickets.readSubtitleStartTicket()).toBeNull();
});

it('stops waiting only for the selected retirement and ignores its late response', async () => {
  let releaseCancel!: (value: typeof completed) => void;
  ipc.invoke.mockReset().mockReturnValue(new Promise<typeof completed>((resolve) => { releaseCancel = resolve; }));
  const pending = retireSubtitleGeneration({ projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const item = getRetiringSubtitleGenerations()[0];
  expect(releaseRetiringSubtitleGeneration({ ...item })).toBe(false);
  expect(releaseRetiringSubtitleGeneration(item)).toBe(true);
  expect(await pending).toBe('released');
  releaseCancel(completed);
  await Promise.resolve();
  expect(getSubtitleRecovery().record).toBeNull();
  expect(getSubtitleRecovery().pending).toBeNull();
});

it('keeps an unconfirmed retirement when ticket storage cannot be cleared', async () => {
  ipc.invoke.mockReset().mockReturnValue(new Promise(() => {}));
  const pending = retireSubtitleGeneration({ projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const item = getRetiringSubtitleGenerations()[0];
  const storage = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('storage unavailable'); });
  expect(releaseRetiringSubtitleGeneration(item)).toBe(false);
  expect(hasRetiringSubtitleGeneration()).toBe(true);
  storage.mockRestore();
  expect(releaseRetiringSubtitleGeneration(item)).toBe(true);
  expect(await pending).toBe('released');
});

it.each(['cancel', 'undo'] as const)('journals the worker completion that races %s', async (action) => {
  ipc.invoke.mockReset().mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? new Promise(() => {}) : command === 'cancel_subtitle_generation' ? Promise.resolve(completed) : Promise.resolve());
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const hook = renderHook(() => useSubtitleGeneration({ t: { subtitleStatusCancelUnconfirmed: 'Unconfirmed' } as never,
    projectResetKey: 'old', segment, setSegment: setter, setSegmentSilently: setter,
    composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(), activeClipId: 'root',
    loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4', currentRawMicAudioPath: '', duration: 4,
    setActivePanel: vi.fn() }));
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.anything()));
  await act(async () => {
    if (action === 'cancel') await hook.result.current.handleCancelSubtitleGeneration();
    else hook.result.current.retireSubtitleForHistory();
  });
  await waitFor(() => expect(getSubtitleRecovery().record?.results).toEqual(completed.results));
  expect(setter).not.toHaveBeenCalled();
  hook.unmount();
});

it('retires and journals a completed worker after status polling fails', async () => {
  ipc.invoke.mockReset().mockImplementation((command: string) => command === 'get_subtitle_generation_status'
    ? Promise.reject(new Error('temporary IPC error')) : command === 'cancel_subtitle_generation' ? Promise.resolve(completed) : Promise.resolve());
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'old', jobId: 'retired-job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  const setter = vi.fn();
  const hook = renderHook(() => useSubtitleGeneration({ t: {} as never, projectResetKey: 'old', segment,
    setSegment: setter, setSegmentSilently: setter, composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
    currentRawMicAudioPath: '', duration: 4, setActivePanel: vi.fn() }));
  // Status-fetch failures are retried (STATUS_FETCH_RETRY_DELAYS_MS) before the job is retired.
  await waitFor(() => expect(getSubtitleRecovery().record?.results).toEqual(completed.results), { timeout: 4000 });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'retired-job' });
  expect(setter).not.toHaveBeenCalled();
  hook.unmount();
});
