import { act, render, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { clearResumableRun, readResumableRun, saveResumableRun, takeResumableStarts } from '@/hooks/resumableJobRegistry';
import type { VideoSegment } from '@/types/video';
import { activateNarrationProject } from '@/hooks/narrationProjectOwnership';
import { SubtitleRecoveryNotice } from '@/components/SubtitleRecoveryNotice';
import { clearSubtitleRecovery, getSubtitleRecovery } from '@/hooks/subtitleRecoveryJournal';
import { armSubtitleShutdownOverride, cancelSubtitleShutdownOverride, clearSubtitleShutdownBlocked, showSubtitleShutdownBlocked } from '@/hooks/subtitleShutdown';
import { beginAudioImportShutdown, cancelAudioImportShutdown } from '@/lib/audioImportWriteFence';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);
vi.mock('@/hooks/useSettings', () => ({ useSettings: () => ({ t: {} }) }));
const ticket = 'ticket-0123456789abcdef0123456789abcdef-transcription-1';

const segment = { trimStart: 0, trimEnd: 10, trimSegments: [{ id: 'trim', startTime: 0, endTime: 10 }], subtitleSegments: [] } as VideoSegment;
const noop = () => {};
const params = (projectResetKey: string | null) => ({
  t: { subtitleStatusCancelUnconfirmed: 'May still be running', subtitleStatusCancelled: 'Cancelled' } as never,
  projectResetKey, segment, setSegment: noop, setSegmentSilently: noop,
  composition: null, setComposition: noop, setCompositionSilently: noop,
  activeClipId: null, loadedClipIdRef: { current: null }, currentRawVideoPath: 'video.mp4',
  currentRawMicAudioPath: '', duration: 10, setActivePanel: noop,
});

afterEach(() => { localStorage.removeItem('sgt:subtitle-generation-start-v1'); clearResumableRun('subtitle-generation'); takeResumableStarts((key) => key === 'subtitle-generation'); cancelSubtitleShutdownOverride(); clearSubtitleShutdownBlocked(); vi.clearAllMocks(); });

it('does not start transcription after close is confirmed or shutdown is admitted', async () => {
  const hook = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => hook.result.current.setSubtitleSource('video'));
  showSubtitleShutdownBlocked();
  expect(armSubtitleShutdownOverride()).toBe(true);
  await act(async () => { await hook.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke).not.toHaveBeenCalledWith('allocate_narration_job_id', expect.anything());
  cancelSubtitleShutdownOverride();
  const token = beginAudioImportShutdown();
  expect(token).not.toBeNull();
  await act(async () => { await hook.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke).not.toHaveBeenCalledWith('allocate_narration_job_id', expect.anything());
  cancelAudioImportShutdown(token);
  hook.unmount();
});

it('retires a start cancelled before the worker returns its job identity', async () => {
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id' ? Promise.resolve({ jobId: ticket }) : command === 'start_subtitle_generation' ? start : Promise.resolve());
  const { result } = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  expect(result.current.isGeneratingSubtitles).toBe(true);
  await act(async () => { await result.current.handleCancelSubtitleGeneration(); });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(result.current.isGeneratingSubtitles).toBe(false);
  expect(result.current.subtitleStatusMessage).toBe('May still be running');
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: ticket }));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('recovers a completed worker job on reopen even when its start reply never arrives', async () => {
  const completed = { state: 'completed', results: [{ clipId: 'root', isPartial: false,
    segments: [{ startTime: 1, endTime: 2, text: 'recovered' }] }] };
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id' ? Promise.resolve({ jobId: ticket })
    : command === 'start_subtitle_generation' ? new Promise(() => {})
      : command === 'cancel_subtitle_generation' ? Promise.resolve(completed) : Promise.resolve());
  const first = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => first.result.current.setSubtitleSource('video'));
  act(() => { void first.result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.objectContaining({ jobId: ticket })));
  first.unmount();
  takeResumableStarts((key) => key === 'subtitle-generation');
  const notice = render(<SubtitleRecoveryNotice />);
  await waitFor(() => expect(getSubtitleRecovery().record?.jobId).toBe(ticket));
  expect(getSubtitleRecovery().record?.results[0].segments[0].text).toBe('recovered');
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(localStorage.getItem('sgt:subtitle-generation-start-v1')).toBeNull();
  notice.unmount();
  await clearSubtitleRecovery(ticket);
});

it('does not start on a legacy worker without transcription tickets', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id'
    ? Promise.reject(new Error('Invalid narration kind')) : Promise.resolve());
  const hook = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => hook.result.current.setSubtitleSource('video'));
  await act(async () => { await hook.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke).not.toHaveBeenCalledWith('start_subtitle_generation', expect.anything());
  expect(localStorage.getItem('sgt:subtitle-generation-start-v1')).toBeNull();
  hook.unmount();
});

it('does not start when the durable ticket cannot be saved', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id'
    ? Promise.resolve({ jobId: ticket }) : Promise.resolve());
  const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation((key) => {
    if (key === 'sgt:subtitle-generation-start-v1') throw new DOMException('quota', 'QuotaExceededError');
  });
  const hook = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => hook.result.current.setSubtitleSource('video'));
  await act(async () => { await hook.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke).not.toHaveBeenCalledWith('start_subtitle_generation', expect.anything());
  setItem.mockRestore();
  hook.unmount();
});

it.each([
  ['other project', { projectKey: 'project-a' }],
  ['ambiguous legacy owner', {}],
])('does not adopt a cached job from %s', async (_label, owner) => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status' ? new Promise(() => {}) : Promise.resolve());
  saveResumableRun('subtitle-generation', {
    ...owner, jobId: 'old', jobContext: null, status: null,
    lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  renderHook(() => useSubtitleGeneration(params('project-b')));
  await act(async () => { await Promise.resolve(); });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'old' });
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: 'old' }));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('resumes the cached job for the same project after remount', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status' ? new Promise(() => {}) : Promise.resolve());
  saveResumableRun('subtitle-generation', {
    projectEpoch: 0, projectKey: 'project-a', jobId: 'owned', jobContext: null, status: null,
    lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  renderHook(() => useSubtitleGeneration(params('project-a')));
  await act(async () => { await Promise.resolve(); });
  expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: 'owned' }));
  expect(ipc.invoke).not.toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'owned' });
});

it('rejects the prior activation after reopening the same project', async () => {
  const owner = { current: 'project-a' as string | null };
  activateNarrationProject(owner, 'project-a');
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'project-a', jobId: 'old', jobContext: null,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  ipc.invoke.mockResolvedValue(undefined);
  renderHook(() => useSubtitleGeneration({ ...params('project-a'), currentProjectIdRef: owner }));
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'old' }));
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: 'old' }));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('cancels a late worker start after the same project is reopened before rerender', async () => {
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  const owner = { current: 'project-a' as string | null };
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id' ? Promise.resolve({ jobId: ticket }) : command === 'start_subtitle_generation' ? start : Promise.resolve());
  const { result } = renderHook(() => useSubtitleGeneration({ ...params('project-a'), currentProjectIdRef: owner }));
  act(() => result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  activateNarrationProject(owner, 'project-a');
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('ignores a delayed result when the same project is reopened before rerender', async () => {
  let finishPoll!: (value: unknown) => void;
  const poll = new Promise((resolve) => { finishPoll = resolve; });
  const owner = { current: 'project-a' as string | null };
  const setter = vi.fn();
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status' ? poll : Promise.resolve());
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'project-a', jobId: 'old', jobContext: null,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
  renderHook(() => useSubtitleGeneration({ ...params('project-a'), currentProjectIdRef: owner, setSegment: setter, setSegmentSilently: setter }));
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.anything()));
  activateNarrationProject(owner, 'project-a');
  await act(async () => { finishPoll({ state: 'completed', progress: 1, message: '', activeClipId: null,
    totalClips: 1, completedClips: 1, resultsRevision: 1, skipped: [],
    results: [{ clipId: 'root', isPartial: false, segments: [{ startTime: 0, endTime: 1, text: 'old' }] }] }); await poll; });
  expect(setter).not.toHaveBeenCalled();
});

it('waits for the same project identity before restoring a cached job', async () => {
  ipc.invoke.mockImplementation((command: string) => command === 'get_subtitle_generation_status' ? new Promise(() => {}) : Promise.resolve());
  saveResumableRun('subtitle-generation', {
    projectEpoch: 0, projectKey: 'project-a', jobId: 'owned', jobContext: null, status: null,
    lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '',
  });
  const { rerender } = renderHook(({ owner }: { owner: string | null }) => useSubtitleGeneration(params(owner)), {
    initialProps: { owner: null },
  });
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.anything());
  await act(async () => { rerender({ owner: 'project-a' }); await Promise.resolve(); });
  expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: 'owned' }));
  expect(ipc.invoke).not.toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'owned' });
});

it('keeps a pending start cancellable after the same project remounts', async () => {
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id' ? Promise.resolve({ jobId: ticket }) : command === 'start_subtitle_generation' ? start : Promise.resolve());
  const first = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => first.result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = first.result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  first.unmount();
  const second = renderHook(() => useSubtitleGeneration(params('project-a')));
  expect(second.result.current.isGeneratingSubtitles).toBe(true);
  await act(async () => { await second.result.current.handleCancelSubtitleGeneration(); });
  expect(second.result.current.isGeneratingSubtitles).toBe(false);
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('retires a pending start when another project mounts', async () => {
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  ipc.invoke.mockImplementation((command: string) => command === 'allocate_narration_job_id' ? Promise.resolve({ jobId: ticket }) : command === 'start_subtitle_generation' ? start : Promise.resolve());
  const first = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => first.result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = first.result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  first.unmount();
  const second = renderHook(() => useSubtitleGeneration(params('project-b')));
  expect(second.result.current.isGeneratingSubtitles).toBe(false);
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
  expect(ipc.invoke).not.toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: ticket }));
  expect(readResumableRun('subtitle-generation')).toBeUndefined();
});

it('adopts a delayed start after the same project remounts and prevents a duplicate', async () => {
  let finishStart!: (value: { jobId: string }) => void;
  const start = new Promise<{ jobId: string }>((resolve) => { finishStart = resolve; });
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: ticket });
    if (command === 'start_subtitle_generation') return start;
    if (command === 'get_subtitle_generation_status') return new Promise(() => {});
    return Promise.resolve();
  });
  const first = renderHook(() => useSubtitleGeneration(params('project-a')));
  act(() => first.result.current.setSubtitleSource('video'));
  let generating!: Promise<void>;
  act(() => { generating = first.result.current.handleGenerateSubtitles(); });
  await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith('start_subtitle_generation', expect.anything()));
  first.unmount();
  const second = renderHook(() => useSubtitleGeneration(params('project-a')));
  await act(async () => { await second.result.current.handleGenerateSubtitles(); });
  expect(ipc.invoke.mock.calls.filter(([command]) => command === 'start_subtitle_generation')).toHaveLength(1);
  await act(async () => { finishStart({ jobId: ticket }); await generating; });
  expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ jobId: ticket }));
  expect(ipc.invoke).not.toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: ticket });
});
