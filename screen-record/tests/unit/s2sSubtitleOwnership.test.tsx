import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import en from '@/i18n/en';
import { invoke } from '@/lib/ipc';
import { useS2sNarration, populateEmptyS2sSubtitleTracks } from '@/hooks/useS2sNarration';
import { setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import type { ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => { localStorage.clear(); vi.mocked(invoke).mockReset(); });
afterEach(() => { localStorage.clear(); vi.restoreAllMocks(); });

describe('S2S subtitle ownership', () => {
  it.each(['completed', 'cancel'] as const)(
    'preserves a user edit after a partial write across remount and %s', async (terminal) => {
      const owner = { current: null as string | null };
      const projectId = `subtitle-owner-${terminal}`;
      setNarrationProjectId(owner, projectId);
      let segment = { trimStart: 0, trimEnd: 3 } as VideoSegment;
      const composition = { mode: 'separate', clips: [
        { id: 'a', duration: 3, segment, rawVideoPath: 'a.mp4' },
      ] } as ProjectComposition;
      const audioApply = deferred<void>();
      const waitingPoll = deferred<unknown>();
      let polls = 0;
      vi.mocked(invoke).mockImplementation((command) => {
        if (command === 'allocate_narration_job_id') return Promise.resolve({ jobId: `${terminal}-ticket` });
        if (command === 'start_s2s_narration') return Promise.resolve({ jobId: `${terminal}-job` });
        if (command === 'get_s2s_narration_status') {
          if (polls++ > 0) return waitingPoll.promise;
          return Promise.resolve({ state: terminal === 'completed' ? 'completed' : 'running', message: 'ok',
            totalClips: 1, completedClips: terminal === 'completed' ? 1 : 0, resultsRevision: 1,
            results: [{ clipId: 'a', isPartial: terminal !== 'completed', segments: [{ id: 's', clipId: 'a',
              startTime: 0, endTime: 1, sourceText: 'source', targetText: 'target', path: 'take.wav', duration: 1 }] }] });
        }
        return Promise.resolve(undefined);
      });
      const apply = vi.fn(() => terminal === 'completed' ? audioApply.promise : Promise.resolve());
      const populate = vi.fn((...args: Parameters<typeof populateEmptyS2sSubtitleTracks> extends [unknown, ...infer Rest] ? Rest : never) => {
        segment = populateEmptyS2sSubtitleTracks(segment, ...args);
        return segment;
      });
      const finalize = vi.fn();
      const options = { projectId, currentProjectIdRef: owner, t: en,
        getNarrationSourceSnapshot: () => ({ composition, segment, loadedClipId: 'a', videoPath: 'a.mp4', micPath: '' }),
        duration: 3, sourceType: 'video' as const, targetLanguage: 'en', geminiModel: 'm', geminiVoice: 'v',
        geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
        onApplyNarrationSegments: apply, onPopulateEmptySubtitles: populate, onFinalize: finalize };
      const first = renderHook(() => useS2sNarration(options));
      await act(async () => { await first.result.current.handleGenerate(); });
      await waitFor(() => expect(apply).toHaveBeenCalledOnce());
      segment = { ...segment, subtitleTracks: segment.subtitleTracks?.map((track) => ({ ...track,
        segments: track.segments.map((cue) => ({ ...cue, text: 'user edit' })) })) };
      const edited = JSON.stringify(segment.subtitleTracks);
      first.unmount();
      const second = renderHook(() => useS2sNarration(options));
      if (terminal === 'completed') {
        await act(async () => { audioApply.resolve(); });
      } else {
        await act(async () => { await second.result.current.handleCancel(); });
      }
      await waitFor(() => expect(second.result.current.status?.state).toBe('error'));
      expect(second.result.current.status?.message).toBe(en.narrationS2sDestinationChanged);
      expect(JSON.stringify(segment.subtitleTracks)).toBe(edited);
      expect(populate).toHaveBeenCalledOnce();
      expect(finalize).not.toHaveBeenCalled();
      second.unmount(); setNarrationProjectId(owner, null);
    },
  );
});
