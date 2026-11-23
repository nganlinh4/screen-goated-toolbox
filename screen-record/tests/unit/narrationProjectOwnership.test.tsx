import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { mockNarrationInvoke, mockNarrationSourceSnapshot } from './narrationIpcMock';
import { useSubtitleNarration } from '@/hooks/useSubtitleNarration';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { narrationProjectEpoch, narrationRunNamespace, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { readResumableRun, saveResumableRun } from '@/hooks/resumableJobRegistry';
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

describe('narration project ownership', () => {
  it('cancels a late subtitle start even if the original project is reopened', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const start = deferred<{ jobId: string }>();
    mockNarrationInvoke((command) =>
      command === 'start_subtitle_narration' ? start.promise : Promise.resolve(undefined));
    const options = subtitleParams(ref, 'A');
    const hook = renderHook(() => useSubtitleNarration(options));
    let pending!: Promise<void>;
    await act(async () => { pending = hook.result.current.handleGenerateNarration(); await Promise.resolve(); });
    act(() => { setNarrationProjectId(ref, 'B'); setNarrationProjectId(ref, 'A'); });
    await act(async () => { start.resolve({ jobId: 'old' }); await pending; });
    expect(invoke).toHaveBeenCalledWith('cancel_subtitle_narration', { jobId: 'old' });
    expect(invoke).not.toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything());
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1);
    expect(options.onFinalizeNarrationSegments).not.toHaveBeenCalled();
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('cancels a late S2S start and never applies it to the replacement project', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const start = deferred<{ jobId: string }>();
    mockNarrationInvoke((command) =>
      command === 'start_s2s_narration' ? start.promise : Promise.resolve(undefined));
    const options = s2sParams(ref, 'A');
    const hook = renderHook(() => useS2sNarration(options));
    let pending!: Promise<void>;
    await act(async () => { pending = hook.result.current.handleGenerate(); await Promise.resolve(); });
    act(() => setNarrationProjectId(ref, 'B'));
    await act(async () => { start.resolve({ jobId: 'old-s2s' }); await pending; });
    expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: 'old-s2s' });
    expect(options.onPopulateEmptySubtitles).not.toHaveBeenCalled();
    expect(options.onApplyNarrationSegments).not.toHaveBeenCalled();
    expect(options.onFinalize).not.toHaveBeenCalled();
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('does not adopt another project’s saved run and cancels it on transition', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const namespace = narrationRunNamespace('A', 'subtitle', narrationProjectEpoch(ref));
    saveResumableRun(namespace, { jobId: 'owned-job' });
    vi.mocked(invoke).mockResolvedValue(undefined);
    setNarrationProjectId(ref, 'B');
    const hook = renderHook(() => useSubtitleNarration(subtitleParams(ref, 'B')));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('cancel_subtitle_narration', { jobId: 'owned-job' }));
    expect(readResumableRun(namespace)).toBeUndefined();
    expect(hook.result.current.isGeneratingNarration).toBe(false);
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('re-adopts an active job when the panel remounts for the same project', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'resume-A' });
      if (command === 'get_subtitle_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    const first = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await first.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything()));
    first.unmount();
    const second = renderHook(() => useSubtitleNarration(options));
    await waitFor(() => expect(second.result.current.isGeneratingNarration).toBe(true));
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'get_subtitle_narration_status')).toHaveLength(2);
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it.each(['subtitle', 's2s'] as const)('owns a pending %s start across panel remounts', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const start = deferred<{ jobId: string }>();
    const status = deferred<unknown>();
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const statusCommand = kind === 'subtitle' ? 'get_subtitle_narration_status' : 'get_s2s_narration_status';
    mockNarrationInvoke((command) => {
      if (command === startCommand) return start.promise;
      if (command === statusCommand) return status.promise;
      return Promise.resolve(undefined);
    });
    const subtitleOptions = subtitleParams(ref, 'A');
    const s2sOptions = s2sParams(ref, 'A');
    const useSubject = () => {
      const subtitle = useSubtitleNarration(subtitleOptions);
      const s2s = useS2sNarration(s2sOptions);
      return kind === 'subtitle'
        ? { generate: subtitle.handleGenerateNarration, busy: subtitle.isGeneratingNarration }
        : { generate: s2s.handleGenerate, busy: s2s.isGenerating };
    };
    const first = renderHook(useSubject);
    let pending!: Promise<void>;
    await act(async () => { pending = first.result.current.generate(); await Promise.resolve(); });
    first.unmount();
    const second = renderHook(useSubject);
    expect(second.result.current.busy).toBe(true);
    await act(async () => { await second.result.current.generate(); });
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === startCommand)).toHaveLength(1);
    await act(async () => { start.resolve({ jobId: `started-${kind}` }); await pending; });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(statusCommand, expect.objectContaining({ jobId: `started-${kind}` })));
    expect(second.result.current.busy).toBe(true);
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it('keeps an S2S job bound to its backend while the selected mode changes', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const start = deferred<{ jobId: string }>();
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_s2s_narration') return start.promise;
      if (command === 'get_s2s_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    const hook = renderHook(({ mode }) => useS2sNarration({ ...options, backendMode: mode }), {
      initialProps: { mode: 's2s' as 's2s' | 'gemini-translate' },
    });
    let pending!: Promise<void>;
    act(() => { pending = hook.result.current.handleGenerate(); });
    hook.rerender({ mode: 'gemini-translate' });
    expect(hook.result.current.isGenerating).toBe(true);
    await act(async () => { await hook.result.current.handleGenerate(); });
    expect(invoke).not.toHaveBeenCalledWith('start_gemini_translate_narration', expect.anything());
    await act(async () => { start.resolve({ jobId: 'mode-job' }); await pending; });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.objectContaining({ jobId: 'mode-job' })));
    expect(invoke).not.toHaveBeenCalledWith('get_gemini_translate_narration_status', expect.anything());
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('shares an in-flight S2S apply across remount and cannot resurrect a completed job', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const apply = deferred<void>();
    const results = [{ clipId: 'clip', segments: [{ id: 'seg', clipId: 'clip', sourceText: 'source', targetText: 'target', path: 'voice.wav', duration: 1, startTime: 0, endTime: 1 }] }];
    let polls = 0;
    mockNarrationInvoke((command) => {
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'shared-s2s' });
      if (command === 'get_s2s_narration_status') {
        return Promise.resolve({ state: ++polls === 1 ? 'running' : 'completed', message: '', totalClips: 1, completedClips: 1, resultsRevision: 1, results: polls === 1 ? results : [] });
      }
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    options.onApplyNarrationSegments.mockImplementation(() => apply.promise);
    const first = renderHook(() => useS2sNarration(options));
    await act(async () => { await first.result.current.handleGenerate(); });
    await waitFor(() => expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1));
    first.unmount();
    const second = renderHook(() => useS2sNarration(options));
    await waitFor(() => expect(polls).toBeGreaterThanOrEqual(2));
    await act(async () => { apply.resolve(); await Promise.resolve(); });
    await waitFor(() => expect(options.onFinalize).toHaveBeenCalledTimes(1));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1);
    expect(second.result.current.isGenerating).toBe(false);
    const namespace = narrationRunNamespace('A', 's2s', narrationProjectEpoch(ref));
    expect(readResumableRun(namespace)).toBeUndefined();
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it('retains a failed S2S apply for retry before completing', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const result = { clipId: 'clip', segments: [{ id: 'seg', clipId: 'clip', sourceText: 'source', targetText: 'target', path: 'voice.wav', duration: 1, startTime: 0, endTime: 1 }] };
    mockNarrationInvoke((command) => {
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'retry-s2s' });
      if (command === 'get_s2s_narration_status') return Promise.resolve({ state: 'completed', message: '', totalClips: 1, completedClips: 1, resultsRevision: 1, results: [result] });
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    options.onApplyNarrationSegments.mockImplementationOnce(() => { throw new Error('temporary failure'); })
      .mockImplementation(() => undefined);
    const hook = renderHook(() => useS2sNarration(options));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(hook.result.current.applyRetryable).toBe(true));
    expect(options.onFinalize).not.toHaveBeenCalled();
    await act(async () => { await hook.result.current.handleRetry(); });
    await waitFor(() => expect(options.onFinalize).toHaveBeenCalledTimes(1));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(2);
    expect(hook.result.current.isGenerating).toBe(false);
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('notifies a remounted S2S panel of a delayed apply failure and retries the same result', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const apply = deferred<void>();
    const result = { clipId: 'clip', segments: [{ id: 'seg', clipId: 'clip', sourceText: 'source', targetText: 'target', path: 'voice.wav', duration: 1, startTime: 0, endTime: 1 }] };
    mockNarrationInvoke((command) => {
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 's2s-remount-fail' });
      if (command === 'get_s2s_narration_status') return Promise.resolve({ state: 'completed', message: '', totalClips: 1, completedClips: 1, resultsRevision: 1, results: [result] });
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    options.onApplyNarrationSegments.mockImplementationOnce(() => apply.promise).mockImplementation(() => undefined);
    const first = renderHook(() => useS2sNarration(options));
    await act(async () => { await first.result.current.handleGenerate(); });
    await waitFor(() => expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1));
    first.unmount();
    const second = renderHook(() => useS2sNarration(options));
    await act(async () => { apply.reject(new Error('temporary failure')); await Promise.resolve(); });
    await waitFor(() => expect(second.result.current.applyRetryable).toBe(true));
    await act(async () => { await second.result.current.handleRetry(); });
    await waitFor(() => expect(options.onFinalize).toHaveBeenCalledTimes(1));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(2);
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it('does not let an old S2S cancel acknowledgement clear a newer job', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const oldStatus = deferred<unknown>();
    const newStatus = deferred<unknown>();
    const cancel = deferred<unknown>();
    let starts = 0;
    mockNarrationInvoke((command, args) => {
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: ++starts === 1 ? 'old-s2s' : 'new-s2s' });
      if (command === 'get_s2s_narration_status') return (args as { jobId: string }).jobId === 'old-s2s' ? oldStatus.promise : newStatus.promise;
      if (command === 'cancel_s2s_narration') return cancel.promise;
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    const hook = renderHook(() => useS2sNarration(options));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.objectContaining({ jobId: 'old-s2s' })));
    let pendingCancel!: Promise<void>;
    act(() => { pendingCancel = hook.result.current.handleCancel(); });
    await act(async () => { oldStatus.resolve({ state: 'completed', message: '', totalClips: 1, completedClips: 1, resultsRevision: 0, results: [] }); await Promise.resolve(); });
    await waitFor(() => expect(hook.result.current.isGenerating).toBe(false));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.objectContaining({ jobId: 'new-s2s' })));
    await act(async () => { cancel.resolve(undefined); await pendingCancel; });
    expect(hook.result.current.isGenerating).toBe(true);
    const namespace = narrationRunNamespace('A', 's2s', narrationProjectEpoch(ref));
    expect(readResumableRun<{ jobId: string }>(namespace)?.jobId).toBe('new-s2s');
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('drops an already queued audio result when project ownership changes before its timer', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'queued-A' });
      if (command === 'get_subtitle_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    const hook = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything()));
    await act(async () => {
      status.resolve({ state: 'running', resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'old.wav', text: 'old', duration: 1, startTime: 0, endTime: 1 }], errors: [] });
      await Promise.resolve();
    });
    act(() => setNarrationProjectId(ref, 'B'));
    await new Promise((resolve) => setTimeout(resolve, 170));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1);
    expect(options.onFinalizeNarrationSegments).not.toHaveBeenCalled();
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it.each(['running', 'completed'] as const)(
    'preserves a queued %s result through same-project panel remount',
    async (state) => {
      const ref = { current: null as string | null };
      setNarrationProjectId(ref, 'A');
      const status = deferred<unknown>();
      mockNarrationInvoke((command) => {
        if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: `remount-${state}` });
        if (command === 'get_subtitle_narration_status') return status.promise;
        return Promise.resolve(undefined);
      });
      const options = subtitleParams(ref, 'A');
      const first = renderHook(() => useSubtitleNarration(options));
      await act(async () => { await first.result.current.handleGenerateNarration(); });
      await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything()));
      await act(async () => {
        status.resolve({ state, resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'voice.wav', text: 'voice', duration: 1, startTime: 0, endTime: 1 }], errors: [] });
        await Promise.resolve();
      });
      first.unmount();
      const second = renderHook(() => useSubtitleNarration(options));
      await waitFor(() => expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(2));
      expect(options.onApplyNarrationSegments.mock.calls[1][0][0].rawAudioPath).toBe('voice.wav');
      if (state === 'completed') await waitFor(() => expect(options.onFinalizeNarrationSegments).toHaveBeenCalledTimes(1));
      second.unmount();
      setNarrationProjectId(ref, null);
    },
  );

  it('drains received narration before acknowledging cancellation', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'cancel-A' });
      if (command === 'get_subtitle_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    const hook = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything()));
    await act(async () => {
      status.resolve({ state: 'running', resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'voice.wav', text: 'voice', duration: 1, startTime: 0, endTime: 1 }], errors: [] });
      await Promise.resolve();
    });
    await act(async () => { await hook.result.current.handleCancelNarration(); });
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(2);
    expect(options.onFinalizeNarrationSegments).toHaveBeenCalledTimes(1);
    expect(hook.result.current.isGeneratingNarration).toBe(false);
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('retains a failed result and retries it before finalizing', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const terminal = { state: 'completed', resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'voice.wav', text: 'voice', duration: 1, startTime: 0, endTime: 1 }], errors: [] };
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'retry-A' });
      if (command === 'get_subtitle_narration_status') return Promise.resolve(terminal);
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    options.onApplyNarrationSegments.mockImplementationOnce(() => undefined)
      .mockImplementationOnce(() => { throw new Error('write failed'); })
      .mockImplementation(() => undefined);
    const hook = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(hook.result.current.applyRetryable).toBe(true));
    expect(options.onFinalizeNarrationSegments).not.toHaveBeenCalled();
    await act(async () => { await hook.result.current.handleRetryNarration(); });
    await waitFor(() => expect(options.onFinalizeNarrationSegments).toHaveBeenCalledTimes(1));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(3);
    expect(hook.result.current.isGeneratingNarration).toBe(false);
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('notifies a remounted panel when its in-flight apply fails', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const pendingApply = deferred<void>();
    const terminal = { state: 'completed', resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'voice.wav', text: 'voice', duration: 1, startTime: 0, endTime: 1 }], errors: [] };
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'remount-failure' });
      if (command === 'get_subtitle_narration_status') return Promise.resolve(terminal);
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    options.onApplyNarrationSegments.mockImplementationOnce(() => undefined)
      .mockImplementationOnce(() => pendingApply.promise)
      .mockImplementation(() => undefined);
    const first = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await first.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(2));
    first.unmount();
    const second = renderHook(() => useSubtitleNarration(options));
    await act(async () => { pendingApply.reject(new Error('temporary write failure')); await Promise.resolve(); });
    await waitFor(() => expect(second.result.current.applyRetryable).toBe(true));
    await act(async () => { await second.result.current.handleRetryNarration(); });
    await waitFor(() => expect(options.onFinalizeNarrationSegments).toHaveBeenCalledTimes(1));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(3);
    second.unmount();
    setNarrationProjectId(ref, null);
  });

  it('does not let an old cancel acknowledgement clear a newer subtitle run', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const oldStatus = deferred<unknown>();
    const newStatus = deferred<unknown>();
    const cancel = deferred<unknown>();
    let starts = 0;
    mockNarrationInvoke((command, args) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: ++starts === 1 ? 'old' : 'new' });
      if (command === 'get_subtitle_narration_status') return (args as { jobId: string }).jobId === 'old' ? oldStatus.promise : newStatus.promise;
      if (command === 'cancel_subtitle_narration') return cancel.promise;
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    const hook = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.objectContaining({ jobId: 'old' })));
    let pendingCancel!: Promise<void>;
    act(() => { pendingCancel = hook.result.current.handleCancelNarration(); });
    await act(async () => { oldStatus.resolve({ state: 'completed', resultsRevision: 0, results: [], errors: [] }); await Promise.resolve(); });
    await waitFor(() => expect(hook.result.current.isGeneratingNarration).toBe(false));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.objectContaining({ jobId: 'new' })));
    await act(async () => { cancel.resolve(undefined); await pendingCancel; });
    expect(hook.result.current.isGeneratingNarration).toBe(true);
    const namespace = narrationRunNamespace('A', 'subtitle', narrationProjectEpoch(ref));
    expect(readResumableRun<{ jobId: string }>(namespace)?.jobId).toBe('new');
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('ignores a completed subtitle poll and finalization after a project switch', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_subtitle_narration') return Promise.resolve({ jobId: 'job-A' });
      if (command === 'get_subtitle_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = subtitleParams(ref, 'A');
    const hook = renderHook(() => useSubtitleNarration(options));
    await act(async () => { await hook.result.current.handleGenerateNarration(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_subtitle_narration_status', expect.anything()));
    act(() => setNarrationProjectId(ref, 'B'));
    await act(async () => {
      status.resolve({ state: 'completed', resultsRevision: 1, results: [{ subtitleId: 'cue', path: 'old.wav', text: 'old', duration: 1, startTime: 0, endTime: 1 }], errors: [] });
      await Promise.resolve();
    });
    await new Promise((resolve) => setTimeout(resolve, 170));
    expect(options.onApplyNarrationSegments).toHaveBeenCalledTimes(1);
    expect(options.onFinalizeNarrationSegments).not.toHaveBeenCalled();
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it('ignores S2S results and subtitle restoration after project ownership changes', async () => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const status = deferred<unknown>();
    mockNarrationInvoke((command) => {
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'job-S' });
      if (command === 'get_s2s_narration_status') return status.promise;
      return Promise.resolve(undefined);
    });
    const options = s2sParams(ref, 'A');
    const hook = renderHook(() => useS2sNarration(options));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.anything()));
    act(() => setNarrationProjectId(ref, 'B'));
    await act(async () => {
      status.resolve({ state: 'completed', resultsRevision: 1, results: [{ segments: [] }] });
      await Promise.resolve();
    });
    expect(options.onPopulateEmptySubtitles).not.toHaveBeenCalled();
    expect(options.onApplyNarrationSegments).not.toHaveBeenCalled();
    expect(options.onFinalize).not.toHaveBeenCalled();
    hook.unmount();
    setNarrationProjectId(ref, null);
  });
});
