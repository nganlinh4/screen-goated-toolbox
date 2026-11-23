import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { mockNarrationInvoke, mockNarrationSourceSnapshot } from './narrationIpcMock';
import { useSubtitleNarration } from '@/hooks/useSubtitleNarration';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { narrationProjectEpoch, narrationRunNamespace, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { readResumableRun } from '@/hooks/resumableJobRegistry';
import type { NarrationProfilePayload } from '@/hooks/useNarrationSettings';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
vi.mock('@/lib/subtitleGenerationPlan', () => ({
  buildSubtitleGenerationPlan: () => ({
    clips: [{ clipId: 'clip', sourcePath: 'source.mp4', sourceDuration: 3, trimSegments: [] }],
    sourceTypeForNative: 'video', replacementRangesByClip: {}, clipTransformsByClip: {},
  }),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function subtitleParams(ref: React.MutableRefObject<string | null>, projectId: string) {
  return {
    projectId, currentProjectIdRef: ref, t: en,
    visibleSubtitles: [{ id: 'cue', text: 'Hello', startTime: 0, endTime: 1 }],
    profile: {} as NarrationProfilePayload,
    onApplyNarrationSegments: vi.fn(), onFinalizeNarrationSegments: vi.fn(),
  };
}

function s2sParams(ref: React.MutableRefObject<string | null>, projectId: string) {
  return {
    projectId, currentProjectIdRef: ref, getNarrationSourceSnapshot: mockNarrationSourceSnapshot, t: en, segment: null, composition: null,
    currentRawVideoPath: 'source.mp4', currentRawMicAudioPath: '', duration: 3,
    sourceType: 'video' as const, targetLanguage: 'en', geminiModel: 'model',
    geminiVoice: 'voice', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25,
    targetSegmentSec: 4, onApplyNarrationSegments: vi.fn(),
    onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn(),
  };
}

beforeEach(() => vi.mocked(invoke).mockReset());
afterEach(() => vi.useRealTimers());

describe('narration terminal ownership', () => {
  it.each(['subtitle', 's2s'] as const)('settles a remounted %s panel when the old completion wins the new poll', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const finalize = deferred<void>();
    const delayedPoll = deferred<unknown>();
    const nextPoll = deferred<unknown>();
    const subtitle = subtitleParams(ref, 'A');
    const s2s = s2sParams(ref, 'A');
    subtitle.onFinalizeNarrationSegments.mockImplementation(() => finalize.promise);
    s2s.onFinalize.mockImplementation(() => finalize.promise);
    const statusCommand = kind === 'subtitle' ? 'get_subtitle_narration_status' : 'get_s2s_narration_status';
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const terminal = kind === 'subtitle'
      ? { state: 'completed', message: 'done', resultsRevision: 0, results: [], errors: [] }
      : { state: 'completed', message: 'done', totalClips: 1, completedClips: 1, resultsRevision: 0, results: [] };
    let polls = 0;
    let starts = 0;
    mockNarrationInvoke((command, args) => {
      if (command === startCommand) return Promise.resolve({ jobId: `reversed-${kind}-${++starts}` });
      if (command === statusCommand) {
        if ((args as { jobId: string }).jobId.endsWith('-2')) return nextPoll.promise;
        return ++polls === 1 ? Promise.resolve(terminal) : delayedPoll.promise;
      }
      return Promise.resolve(undefined);
    });
    const useSubject = () => {
      const a = useSubtitleNarration(subtitle);
      const b = useS2sNarration(s2s);
      return kind === 'subtitle'
        ? { generate: a.handleGenerateNarration, busy: a.isGeneratingNarration, status: a.narrationStatus }
        : { generate: b.handleGenerate, busy: b.isGenerating, status: b.status };
    };
    const first = renderHook(useSubject);
    await act(async () => { await first.result.current.generate(); });
    await waitFor(() => expect(kind === 'subtitle' ? subtitle.onFinalizeNarrationSegments : s2s.onFinalize).toHaveBeenCalledTimes(1));
    first.unmount();
    const second = renderHook(useSubject);
    await waitFor(() => expect(polls).toBe(2));
    expect(second.result.current.busy).toBe(true);
    await act(async () => { finalize.resolve(); await Promise.resolve(); });
    await waitFor(() => expect(second.result.current.busy).toBe(false));
    expect(second.result.current.status?.state).toBe('completed');
    await act(async () => { await second.result.current.generate(); });
    expect(second.result.current.busy).toBe(true);
    await act(async () => { delayedPoll.resolve(terminal); await Promise.resolve(); });
    expect(second.result.current.busy).toBe(true);
    const namespace = narrationRunNamespace('A', kind, narrationProjectEpoch(ref));
    expect(readResumableRun<{ jobId: string }>(namespace)?.jobId).toBe(`reversed-${kind}-2`);
    second.unmount();
    setNarrationProjectId(ref, null);
  });
  it.each(['subtitle', 's2s'] as const)('retries a failed late %s cancellation after project retirement', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const start = deferred<{ jobId: string }>();
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const cancelCommand = kind === 'subtitle' ? 'cancel_subtitle_narration' : 'cancel_s2s_narration';
    let attempts = 0;
    mockNarrationInvoke((command) => {
      if (command === startCommand) return start.promise;
      if (command === cancelCommand && ++attempts === 1) return Promise.reject(new Error('temporary IPC failure'));
      return Promise.resolve(undefined);
    });
    const subtitle = subtitleParams(ref, 'A');
    const s2s = s2sParams(ref, 'A');
    const hook = renderHook(() => ({ subtitle: useSubtitleNarration(subtitle), s2s: useS2sNarration(s2s) }));
    let pending!: Promise<void>;
    await act(async () => {
      pending = kind === 'subtitle' ? hook.result.current.subtitle.handleGenerateNarration() : hook.result.current.s2s.handleGenerate();
      await Promise.resolve();
    });
    act(() => setNarrationProjectId(ref, 'B'));
    await act(async () => { start.resolve({ jobId: `retired-${kind}` }); await pending; });
    hook.unmount();
    await waitFor(() => expect(attempts).toBe(2), { timeout: 2000 });
    expect(invoke).toHaveBeenCalledWith(cancelCommand, { jobId: `retired-${kind}` });
    expect(s2s.onPopulateEmptySubtitles).not.toHaveBeenCalled();
    expect(subtitle.onFinalizeNarrationSegments).not.toHaveBeenCalled();
    setNarrationProjectId(ref, null);
  });

  it.each(['cancelled', 'error'] as const)('publishes %s terminal status after drain and remount', async (state) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const finalize = deferred<void>();
    const terminal = { state, message: `backend ${state}`, resultsRevision: 0, results: [], errors: [] };
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: `terminal-${state}` });
      if (command === 'get_subtitle_narration_status') return Promise.resolve(terminal);
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    options.onFinalizeNarrationSegments.mockImplementation(() => finalize.promise);
    const first = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await first.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(options.onFinalizeNarrationSegments).toHaveBeenCalledTimes(1));
    first.unmount();
    const second = renderHook(() => useSubtitleNarration(options));
    await waitFor(() => expect(second.result.current.narrationStatus?.state).toBe(state));
    await act(async () => { finalize.resolve(); await Promise.resolve(); });
    await waitFor(() => expect(second.result.current.isGeneratingNarration).toBe(false));
    expect(second.result.current.narrationStatus?.message).toBe(`backend ${state}`);
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it.each(['cancelled', 'error'] as const)('publishes %s even when the throttled status update is cancelled by completion', async (state) => {
    const clock = vi.spyOn(performance, 'now').mockReturnValue(100);
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: `throttled-${state}` });
      if (command === 'get_subtitle_narration_status') return Promise.resolve({ state, message: `backend ${state}`, resultsRevision: 0, results: [], errors: [] });
      return Promise.resolve(undefined);
    });
    const hook = renderHook(() => useSubtitleNarration(subtitleParams(ref, 'A')));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(hook.result.current.isGeneratingNarration).toBe(false));
    expect(hook.result.current.narrationStatus?.state).toBe(state);
    expect(hook.result.current.narrationStatus?.message).toBe(`backend ${state}`);
    hook.unmount();
    setNarrationProjectId(ref, null);
    clock.mockRestore();
  });
});
