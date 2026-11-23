import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { mockNarrationInvoke, mockNarrationSourceSnapshot } from './narrationIpcMock';
import { useSubtitleNarration } from '@/hooks/useSubtitleNarration';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import type { NarrationProfilePayload } from '@/hooks/useNarrationSettings';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
vi.mock('@/lib/subtitleGenerationPlan', () => ({ buildSubtitleGenerationPlan: () => ({
  clips: [{ clipId: 'clip', sourcePath: 'source.mp4', sourceDuration: 3, trimSegments: [] }],
  sourceTypeForNative: 'video', replacementRangesByClip: {}, clipTransformsByClip: {},
}) }));

function options(ref: React.MutableRefObject<string | null>) {
  return {
    subtitle: { projectId: 'A', currentProjectIdRef: ref, t: en,
      visibleSubtitles: [{ id: 'cue', text: 'Hello', startTime: 0, endTime: 1 }],
      profile: {} as NarrationProfilePayload, onApplyNarrationSegments: vi.fn(), onFinalizeNarrationSegments: vi.fn() },
    s2s: { projectId: 'A', currentProjectIdRef: ref, getNarrationSourceSnapshot: mockNarrationSourceSnapshot, t: en, segment: null, composition: null,
      currentRawVideoPath: 'source.mp4', currentRawMicAudioPath: '', duration: 3, sourceType: 'video' as const,
      targetLanguage: 'en', geminiModel: 'model', geminiVoice: 'voice', geminiSpeed: '1', parallelRequests: 1,
      groupTextBudget: 25, targetSegmentSec: 4, onApplyNarrationSegments: vi.fn(),
      onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() },
  };
}

beforeEach(() => { localStorage.clear(); vi.mocked(invoke).mockReset(); });
afterEach(() => { localStorage.clear(); vi.restoreAllMocks(); });

describe('narration start recovery', () => {
  it.each(['subtitle', 's2s'] as const)('does not start %s after ownership changes during ticket allocation', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    let resolveTicket!: (value: { jobId: string }) => void;
    const allocation = new Promise<{ jobId: string }>((resolve) => { resolveTicket = resolve; });
    vi.mocked(invoke).mockImplementation((command) => command === 'allocate_narration_job_id' ? allocation : Promise.resolve(undefined));
    const params = options(ref);
    const hook = renderHook(() => ({ subtitle: useSubtitleNarration(params.subtitle), s2s: useS2sNarration(params.s2s) }));
    let pending!: Promise<void>;
    act(() => { pending = kind === 'subtitle' ? hook.result.current.subtitle.handleGenerateNarration() : hook.result.current.s2s.handleGenerate(); });
    act(() => setNarrationProjectId(ref, 'B'));
    await act(async () => { resolveTicket({ jobId: `ticket-test-${kind}-1` }); await pending; });
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    expect(invoke).not.toHaveBeenCalledWith(startCommand, expect.anything());
    expect(params.subtitle.onApplyNarrationSegments).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    hook.unmount();
    setNarrationProjectId(ref, null);
  });
  it.each(['subtitle', 's2s'] as const)('retires a hung %s start immediately on project switch', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const cancelCommand = kind === 'subtitle' ? 'cancel_subtitle_narration' : 'cancel_s2s_narration';
    let resolveStart!: (value: { jobId: string }) => void;
    const startReply = new Promise<{ jobId: string }>((resolve) => { resolveStart = resolve; });
    mockNarrationInvoke((command) => command === startCommand ? startReply : Promise.resolve({ outcome: 'missing' }));
    const params = options(ref);
    const hook = renderHook(() => ({ subtitle: useSubtitleNarration(params.subtitle), s2s: useS2sNarration(params.s2s) }));
    let pending!: Promise<void>;
    await act(async () => { pending = kind === 'subtitle' ? hook.result.current.subtitle.handleGenerateNarration() : hook.result.current.s2s.handleGenerate(); await Promise.resolve(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(startCommand, expect.anything()));
    const id = (vi.mocked(invoke).mock.calls.find(([command]) => command === startCommand)?.[1] as { jobId: string }).jobId;
    act(() => setNarrationProjectId(ref, 'B'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(cancelCommand, { jobId: id }));
    await act(async () => { resolveStart({ jobId: id }); await pending; });
    hook.unmount();
    setNarrationProjectId(ref, null);
  });

  it.each(['subtitle', 's2s'] as const)('retains %s cancellation until acknowledged after a poll IPC failure', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const cancelCommand = kind === 'subtitle' ? 'cancel_subtitle_narration' : 'cancel_s2s_narration';
    let cancels = 0;
    mockNarrationInvoke((command, args) => {
      if (command === startCommand) return Promise.resolve({ jobId: (args as { jobId: string }).jobId });
      if (command.startsWith('get_')) return Promise.reject(new Error('poll unavailable'));
      if (command === cancelCommand && ++cancels === 1) return Promise.reject(new Error('cancel unavailable'));
      return Promise.resolve({ outcome: 'cancelled' });
    });
    const params = options(ref);
    const hook = renderHook(() => ({ subtitle: useSubtitleNarration(params.subtitle), s2s: useS2sNarration(params.s2s) }));
    await act(async () => { await (kind === 'subtitle' ? hook.result.current.subtitle.handleGenerateNarration() : hook.result.current.s2s.handleGenerate()); });
    // Status-fetch failures are retried (STATUS_FETCH_RETRY_DELAYS_MS) before the job is retired.
    await waitFor(() => expect(cancels).toBe(1), { timeout: 4000 });
    expect(localStorage.length).toBe(1);
    await waitFor(() => expect(cancels).toBe(2), { timeout: 2000 });
    await waitFor(() => expect(localStorage.length).toBe(0));
    hook.unmount();
    setNarrationProjectId(ref, null);
  }, 10_000);
  it.each(['subtitle', 's2s'] as const)('recovers a %s start if the page reloads before its reply', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const cancelCommand = kind === 'subtitle' ? 'cancel_subtitle_narration' : 'cancel_s2s_narration';
    let resolveStart!: (result: { jobId: string }) => void;
    const startReply = new Promise<{ jobId: string }>((resolve) => { resolveStart = resolve; });
    mockNarrationInvoke((command) => command === startCommand
      ? startReply : Promise.resolve({ outcome: 'missing' }));
    const params = options(ref);
    const hook = renderHook(() => ({ subtitle: useSubtitleNarration(params.subtitle), s2s: useS2sNarration(params.s2s) }));
    let pending!: Promise<void>;
    await act(async () => {
      pending = kind === 'subtitle' ? hook.result.current.subtitle.handleGenerateNarration() : hook.result.current.s2s.handleGenerate();
      await Promise.resolve();
    });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(startCommand, expect.objectContaining({ jobId: expect.stringMatching(/^ticket-/) })));
    const id = (vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === startCommand)?.[1] as { jobId: string }).jobId;
    expect(localStorage.length).toBe(1);
    hook.unmount();
    vi.resetModules();
    const { resumeRetiredNarrationCancellations } = await import('@/hooks/retiredNarrationCancellation');
    resumeRetiredNarrationCancellations();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(cancelCommand, { jobId: id }));
    await waitFor(() => expect(localStorage.length).toBe(0));
    setNarrationProjectId(ref, null);
    await act(async () => { resolveStart({ jobId: id }); await pending; });
  });

  it.each(['subtitle', 's2s'] as const)('keeps %s ownership across panel remount and cleans it on cancellation', async (kind) => {
    const ref = { current: null as string | null };
    setNarrationProjectId(ref, 'A');
    const startCommand = kind === 'subtitle' ? 'start_subtitle_narration' : 'start_s2s_narration';
    const cancelCommand = kind === 'subtitle' ? 'cancel_subtitle_narration' : 'cancel_s2s_narration';
    mockNarrationInvoke((command, args) => {
      if (command === startCommand) return Promise.resolve({ jobId: (args as { jobId: string }).jobId });
      if (command.startsWith('get_')) return new Promise(() => {});
      return Promise.resolve({ outcome: 'cancelled' });
    });
    const params = options(ref);
    const render = () => renderHook(() => ({ subtitle: useSubtitleNarration(params.subtitle), s2s: useS2sNarration(params.s2s) }));
    const first = render();
    await act(async () => { await (kind === 'subtitle' ? first.result.current.subtitle.handleGenerateNarration() : first.result.current.s2s.handleGenerate()); });
    expect(localStorage.length).toBe(1);
    first.unmount();
    const second = render();
    await waitFor(() => expect(kind === 'subtitle' ? second.result.current.subtitle.isGeneratingNarration : second.result.current.s2s.isGenerating).toBe(true));
    expect(invoke).not.toHaveBeenCalledWith(cancelCommand, expect.anything());
    expect(localStorage.length).toBe(1);
    second.unmount();
    act(() => setNarrationProjectId(ref, null));
    await waitFor(() => expect(localStorage.length).toBe(0));
  });
});
