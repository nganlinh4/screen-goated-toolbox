import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useSubtitleGeneration } from '@/hooks/useSubtitleGeneration';
import { captureSubtitleSourceOwners } from '@/hooks/subtitleSourceOwnership';
import { clearResumableRun, readResumableRun, saveResumableRun, takeResumableStarts } from '@/hooks/resumableJobRegistry';
import type { ResumableSubtitleGenerationRun } from '@/hooks/subtitleGenerationTypes';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { VideoSegment } from '@/types/video';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@/lib/ipc', () => ipc);

const segment = { trimStart: 0, trimEnd: 10, trimSegments: [{ id: 'trim', startTime: 0, endTime: 10 }], subtitleSegments: [] } as VideoSegment;
const plan = {
  clips: [{ clipId: 'root', sourcePath: 'video.mp4', sourceDuration: 10, trimSegments: [] }],
  sourceTypeForNative: 'video', clipTransformsByClip: {}, replacementRangesByClip: {}, indicator: {},
} as SubtitleGenerationPlan;
const context = {
  projectKey: 'p', indicator: {} as never, sourceTypeForNative: 'video' as const,
  replacementRangesByClip: {}, clipTransformsByClip: {}, sourceOwnersByClip: captureSubtitleSourceOwners(plan, segment, null),
};
const partial = { clipId: 'root', isPartial: true, segments: [{ startTime: 1, endTime: 2, text: 'partial' }] };
const running = { state: 'running', message: '', progress: 0.5, activeClipId: 'root', totalClips: 1,
  completedClips: 0, resultsRevision: 1, skipped: [], results: [partial] };
const complete = { ...running, state: 'completed', progress: 1, results: [] };

function seed() {
  saveResumableRun('subtitle-generation', { projectEpoch: 0, projectKey: 'p', jobId: 'job', jobContext: context,
    status: null, lastKnownResultsRevision: 0, lastAppliedResultsKey: '', lastStatusViewKey: '' });
}

function params(setSegment: ReturnType<typeof vi.fn>, currentRawVideoPath = 'video.mp4') {
  return {
    t: { subtitleStatusSourceChanged: 'Source changed', subtitleStatusCancelUnconfirmed: 'May still be running' } as never,
    projectResetKey: 'p', segment, setSegment, setSegmentSilently: setSegment,
    composition: null, setComposition: vi.fn(), setCompositionSilently: vi.fn(),
    activeClipId: 'root', loadedClipIdRef: { current: null }, currentRawVideoPath,
    currentRawMicAudioPath: '', duration: 10, setActivePanel: vi.fn(),
  };
}

afterEach(() => {
  clearResumableRun('subtitle-generation');
  takeResumableStarts((key) => key === 'subtitle-generation');
  vi.useRealTimers();
  vi.clearAllMocks();
});

it('restores an unflushed partial result after a same-project remount', async () => {
  vi.useFakeTimers();
  let polls = 0;
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'get_subtitle_generation_status') return ++polls === 1 ? Promise.resolve(running) : new Promise(() => {});
    return Promise.resolve();
  });
  seed();
  const firstSetter = vi.fn();
  const first = renderHook(() => useSubtitleGeneration(params(firstSetter)));
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.pendingResults).toEqual([partial]);
  first.unmount();
  const secondSetter = vi.fn();
  const second = renderHook(() => useSubtitleGeneration(params(secondSetter)));
  await act(async () => { await Promise.resolve(); await vi.advanceTimersByTimeAsync(2501); });
  expect(firstSetter).not.toHaveBeenCalled();
  expect(secondSetter).toHaveBeenCalled();
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.pendingResults).toEqual([]);
  second.unmount();
});

it('advances identical result revisions and resumes the next event without losing pending data', async () => {
  vi.useFakeTimers();
  const samePresentation = { ...partial, segments: [{ ...partial.segments[0], endTime: 2.5 }] };
  const next = { ...partial, segments: [{ startTime: 3, endTime: 4, text: 'next' }] };
  ipc.invoke.mockImplementation((command: string, args?: { knownResultsRevision?: number }) => {
    if (command !== 'get_subtitle_generation_status') return Promise.resolve();
    if (args?.knownResultsRevision === 0) return Promise.resolve(running);
    if (args?.knownResultsRevision === 1) return Promise.resolve({ ...running, resultsRevision: 2, results: [samePresentation] });
    if (args?.knownResultsRevision === 2) return Promise.resolve({ ...running, resultsRevision: 3, results: [next] });
    return new Promise(() => {});
  });
  seed();
  const first = renderHook(() => useSubtitleGeneration(params(vi.fn())));
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  await act(async () => { await vi.advanceTimersByTimeAsync(121); });
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.lastKnownResultsRevision).toBe(2);
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.pendingResults).toEqual([samePresentation]);
  first.unmount();
  const apply = vi.fn();
  const second = renderHook(() => useSubtitleGeneration(params(apply)));
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  expect(ipc.invoke).toHaveBeenCalledWith('get_subtitle_generation_status', expect.objectContaining({ knownResultsRevision: 2 }));
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.lastKnownResultsRevision).toBe(3);
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.pendingResults).toEqual([next]);
  await act(async () => { await vi.advanceTimersByTimeAsync(2501); });
  expect(apply).toHaveBeenCalledOnce();
  const updater = apply.mock.calls[0][0] as (previous: VideoSegment) => VideoSegment;
  expect(updater(segment).subtitleSegments).toEqual(expect.arrayContaining([expect.objectContaining({ text: 'next' })]));
  second.unmount();
});

it('retains a skipped-source result notice across remount until completion', async () => {
  vi.useFakeTimers();
  let polls = 0;
  ipc.invoke.mockImplementation((command: string) => {
    if (command === 'get_subtitle_generation_status') return ++polls === 1 ? Promise.resolve(running)
      : polls === 2 ? new Promise(() => {}) : Promise.resolve(complete);
    return Promise.resolve();
  });
  seed();
  const setter = vi.fn();
  const first = renderHook(() => useSubtitleGeneration(params(setter, 'replacement.mp4')));
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  await act(async () => { await vi.advanceTimersByTimeAsync(2501); });
  expect(setter).not.toHaveBeenCalled();
  expect(readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')?.staleResults).toBe(true);
  first.unmount();
  const second = renderHook(() => useSubtitleGeneration(params(setter, 'replacement.mp4')));
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  expect(second.result.current.subtitleStatusMessage).toBe('Source changed');
  second.unmount();
});

it('does not replay queued results into a different project', async () => {
  vi.useFakeTimers();
  ipc.invoke.mockImplementation(() => Promise.resolve());
  seed();
  const previous = readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation')!;
  saveResumableRun('subtitle-generation', { ...previous, pendingResults: [partial] });
  const setter = vi.fn();
  const { result } = renderHook(() => useSubtitleGeneration({ ...params(setter), projectResetKey: 'other' }));
  await act(async () => { await Promise.resolve(); await vi.advanceTimersByTimeAsync(2501); });
  expect(setter).not.toHaveBeenCalled();
  expect(result.current.isGeneratingSubtitles).toBe(false);
  expect(ipc.invoke).toHaveBeenCalledWith('cancel_subtitle_generation', { jobId: 'job' });
});
