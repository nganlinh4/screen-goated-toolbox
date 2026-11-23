import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import type { ImportedAudioSegment, ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => { localStorage.clear(); vi.mocked(invoke).mockReset(); });
afterEach(() => { localStorage.clear(); vi.restoreAllMocks(); });

describe('S2S source ownership', () => {
  it('never applies a delayed clip A result into newly loaded clip B', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'destination-switch');
    const a = { trimStart: 0, trimEnd: 3 } as VideoSegment;
    const b = { trimStart: 0, trimEnd: 4 } as VideoSegment;
    const composition = { mode: 'separate', audioSegments: [{ id: 'source-audio', rawAudioPath: 'audio.wav',
      name: 'Audio', duration: 3, startTime: 0, inPoint: 0, outPoint: 3, addedAt: 0 }], clips: [
      { id: 'a', duration: 3, segment: a, rawVideoPath: 'a.mp4' },
      { id: 'b', duration: 4, segment: b, rawVideoPath: 'b.mp4' },
    ] } as ProjectComposition;
    let current = { composition, segment: a, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' };
    const poll = deferred<unknown>();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: 'destination-ticket' });
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'destination-job' });
      if (command === 'get_s2s_narration_status') return poll.promise;
      return Promise.resolve(undefined);
    });
    const options = { projectId: 'destination-switch', currentProjectIdRef: owner, t: en, segment: a, composition,
      getNarrationSourceSnapshot: () => current, currentRawVideoPath: 'a.mp4', currentRawMicAudioPath: '',
      activeClipId: 'a', duration: 3, sourceType: 'video' as const, targetLanguage: 'en', geminiModel: 'm',
      geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
      onApplyNarrationSegments: vi.fn(), onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() };
    const hook = renderHook(() => useS2sNarration(options));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.anything()));
    current = { composition, segment: b, loadedClipId: 'b', videoPath: 'b.mp4', micPath: '' };
    await act(async () => { poll.resolve({ state: 'completed', message: 'done', totalClips: 1,
      completedClips: 1, resultsRevision: 1, results: [{ clipId: 'a', isPartial: false,
        segments: [{ id: 's', clipId: 'a', startTime: 1, endTime: 2, sourceText: 'a', targetText: 'b',
          path: 'generated.wav', duration: 1 }] }] }); });
    await waitFor(() => expect(hook.result.current.status?.state).toBe('error'));
    expect(hook.result.current.status?.message).toBe(en.narrationS2sDestinationChanged);
    expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: 'destination-job' });
    expect(options.onPopulateEmptySubtitles).not.toHaveBeenCalled();
    expect(options.onApplyNarrationSegments).not.toHaveBeenCalled();
    expect(options.onFinalize).not.toHaveBeenCalled();
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it.each(['video', 'audio'] as const)('rejects unified multi-clip %s output before starting a job', async (sourceType) => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'unified-destination');
    const a = { trimStart: 0, trimEnd: 3 } as VideoSegment;
    const composition = { mode: 'unified', clips: [
      { id: 'a', duration: 3, segment: a, rawVideoPath: 'a.mp4' },
      { id: 'b', duration: 4, segment: { trimStart: 0, trimEnd: 4 }, rawVideoPath: 'b.mp4' },
    ] } as ProjectComposition;
    const source = { composition, segment: a, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' };
    const hook = renderHook(() => useS2sNarration({ projectId: 'unified-destination', currentProjectIdRef: owner,
      t: en, segment: a, composition, getNarrationSourceSnapshot: () => source, currentRawVideoPath: 'a.mp4',
      currentRawMicAudioPath: '', duration: 3, sourceType, targetLanguage: 'en', geminiModel: 'm',
      geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
      onApplyNarrationSegments: vi.fn(), onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() }));
    await act(async () => { await hook.result.current.handleGenerate(); });
    expect(hook.result.current.status?.state).toBe('error');
    expect(hook.result.current.status?.message).toBe(en.narrationS2sRequiresSingleClip);
    expect(invoke).not.toHaveBeenCalledWith('allocate_narration_job_id', expect.anything());
    expect(invoke).not.toHaveBeenCalledWith('start_s2s_narration', expect.anything());
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it('rejects project-wide audio when multiple separate clips have no safe subtitle destination mapping', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'audio-destination');
    const segment = { trimStart: 0, trimEnd: 3 } as VideoSegment;
    const composition = { mode: 'separate', clips: [
      { id: 'a', duration: 3, segment, rawVideoPath: 'a.mp4' },
      { id: 'b', duration: 3, segment, rawVideoPath: 'b.mp4' },
    ], audioSegments: [{ id: 'audio', name: 'Audio', rawAudioPath: 'audio.wav', duration: 8,
      startTime: 0, inPoint: 0, outPoint: 8, playbackRate: 1, addedAt: 0 }] } as ProjectComposition;
    const source = { composition, segment, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' };
    const hook = renderHook(() => useS2sNarration({ projectId: 'audio-destination', currentProjectIdRef: owner,
      t: en, segment, composition, getNarrationSourceSnapshot: () => source, currentRawVideoPath: 'a.mp4',
      currentRawMicAudioPath: '', duration: 3, sourceType: 'audio', targetLanguage: 'en', geminiModel: 'm',
      geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
      onApplyNarrationSegments: vi.fn(), onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() }));
    await act(async () => { await hook.result.current.handleGenerate(); });
    expect(hook.result.current.status?.message).toBe(en.narrationS2sRequiresSingleClip);
    expect(invoke).not.toHaveBeenCalledWith('start_s2s_narration', expect.anything());
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it('retires an audio job if a second clip is added before its result returns', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'audio-topology');
    const segment = { trimStart: 0, trimEnd: 3 } as VideoSegment;
    const clip = { id: 'a', duration: 3, segment, rawVideoPath: 'a.mp4' };
    const audio = { id: 'audio', name: 'Audio', rawAudioPath: 'audio.wav', duration: 3,
      startTime: 0, inPoint: 0, outPoint: 3, playbackRate: 1, addedAt: 0 };
    let composition = { mode: 'separate', clips: [clip], audioSegments: [audio] } as ProjectComposition;
    const poll = deferred<unknown>();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: 'topology-ticket' });
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'topology-job' });
      if (command === 'get_s2s_narration_status') return poll.promise;
      return Promise.resolve(undefined);
    });
    const onApply = vi.fn(); const onSubtitles = vi.fn();
    const hook = renderHook(() => useS2sNarration({ projectId: 'audio-topology', currentProjectIdRef: owner, t: en,
      segment, composition, getNarrationSourceSnapshot: () => ({ composition, segment, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' }),
      currentRawVideoPath: 'a.mp4', currentRawMicAudioPath: '', duration: 3, sourceType: 'audio', targetLanguage: 'en',
      geminiModel: 'm', geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
      onApplyNarrationSegments: onApply, onPopulateEmptySubtitles: onSubtitles, onFinalize: vi.fn() }));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.anything()));
    composition = { ...composition, clips: [clip, { ...clip, id: 'b', rawVideoPath: 'b.mp4' }] } as ProjectComposition;
    await act(async () => { poll.resolve({ state: 'completed', message: 'done', totalClips: 1, completedClips: 1,
      resultsRevision: 1, results: [{ clipId: 'audio:audio', isPartial: false, segments: [{ id: 's', clipId: 'audio:audio',
        startTime: 0, endTime: 1, sourceText: 'a', targetText: 'b', path: 'generated.wav', duration: 1 }] }] }); });
    await waitFor(() => expect(hook.result.current.status?.message).toBe(en.narrationS2sDestinationChanged));
    expect(onApply).not.toHaveBeenCalled(); expect(onSubtitles).not.toHaveBeenCalled();
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it('retires a remounted job when the source rate changes before a delayed final result', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'project');
    const audio: ImportedAudioSegment = { id: 'audio', name: 'Audio', rawAudioPath: 'audio.wav',
      duration: 8, startTime: 2, inPoint: 1, outPoint: 7, playbackRate: 1, addedAt: 1 };
    const segment = { trimStart: 0, trimEnd: 12, trimSegments: [{ id: 't', startTime: 0, endTime: 12 }] } as VideoSegment;
    let composition = { mode: 'separate', audioSegments: [audio], clips: [{ id: 'root', segment }] } as ProjectComposition;
    const poll = deferred<unknown>();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: 'ticket' });
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'job' });
      if (command === 'get_s2s_narration_status') return poll.promise;
      return Promise.resolve(undefined);
    });
    const options = { projectId: 'project', currentProjectIdRef: owner, t: en, segment, composition,
      getNarrationSourceSnapshot: () => ({ composition, segment, loadedClipId: 'root', videoPath: '', micPath: '' }),
      currentRawVideoPath: '', currentRawMicAudioPath: '', duration: 12, sourceType: 'audio' as const,
      targetLanguage: 'en', geminiModel: 'm', geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1,
      groupTextBudget: 25, targetSegmentSec: 4, onApplyNarrationSegments: vi.fn(),
      onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() };
    const first = renderHook(() => useS2sNarration(options));
    await act(async () => { await first.result.current.handleGenerate(); });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_s2s_narration_status', expect.anything()));
    first.unmount();
    composition = { ...composition, audioSegments: [{ ...audio, playbackRate: 2 }] };
    const second = renderHook(() => useS2sNarration(options));
    await act(async () => { poll.resolve({ state: 'completed', message: 'done', totalClips: 1,
      completedClips: 1, resultsRevision: 1, results: [{ clipId: 'audio:audio', isPartial: false,
        segments: [{ id: 's', clipId: 'audio:audio', startTime: 2, endTime: 3, sourceText: 'a',
          targetText: 'b', path: 'generated.wav', duration: 1 }] }] }); });
    await waitFor(() => expect(second.result.current.status?.state).toBe('error'));
    expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: 'job' });
    expect(options.onPopulateEmptySubtitles).not.toHaveBeenCalled();
    expect(options.onApplyNarrationSegments).not.toHaveBeenCalled();
    expect(options.onFinalize).not.toHaveBeenCalled();
    second.unmount();
    setNarrationProjectId(owner, null);
  });

  it('does not finalize an admitted partial after its source changes during deferred apply', async () => {
    const owner = { current: null as string | null };
    setNarrationProjectId(owner, 'project');
    const audio: ImportedAudioSegment = { id: 'audio', name: 'Audio', rawAudioPath: 'audio.wav',
      duration: 8, startTime: 2, inPoint: 1, outPoint: 7, playbackRate: 1, addedAt: 1 };
    const segment = { trimStart: 0, trimEnd: 12 } as VideoSegment;
    let composition = { mode: 'separate', audioSegments: [audio], clips: [{ id: 'root', segment }] } as ProjectComposition;
    const apply = deferred<void>();
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: 'ticket-2' });
      if (command === 'start_s2s_narration') return Promise.resolve({ jobId: 'job-2' });
      if (command === 'get_s2s_narration_status') return Promise.resolve({ state: 'completed', message: 'done',
        totalClips: 1, completedClips: 1, resultsRevision: 1, results: [{ clipId: 'audio:audio', isPartial: false,
          segments: [{ id: 's', clipId: 'audio:audio', startTime: 2, endTime: 3, sourceText: 'a', targetText: 'b',
            path: 'generated.wav', duration: 1 }] }] });
      return Promise.resolve(undefined);
    });
    const options = { projectId: 'project', currentProjectIdRef: owner, t: en, segment, composition,
      getNarrationSourceSnapshot: () => ({ composition, segment, loadedClipId: 'root', videoPath: '', micPath: '' }),
      currentRawVideoPath: '', currentRawMicAudioPath: '', duration: 12, sourceType: 'audio' as const,
      targetLanguage: 'en', geminiModel: 'm', geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1,
      groupTextBudget: 25, targetSegmentSec: 4, onApplyNarrationSegments: vi.fn(() => apply.promise),
      onPopulateEmptySubtitles: vi.fn(), onFinalize: vi.fn() };
    const hook = renderHook(() => useS2sNarration(options));
    await act(async () => { await hook.result.current.handleGenerate(); });
    await waitFor(() => expect(options.onApplyNarrationSegments).toHaveBeenCalledOnce());
    composition = { ...composition, audioSegments: [{ ...audio, startTime: 5 }] };
    await act(async () => { apply.resolve(); });
    await waitFor(() => expect(hook.result.current.status?.state).toBe('error'));
    expect(invoke).toHaveBeenCalledWith('cancel_s2s_narration', { jobId: 'job-2' });
    expect(options.onFinalize).not.toHaveBeenCalled();
    expect(options.onPopulateEmptySubtitles).toHaveBeenCalledOnce();
    hook.unmount();
    setNarrationProjectId(owner, null);
  });
});
