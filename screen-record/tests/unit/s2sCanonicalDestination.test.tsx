import { act, renderHook, waitFor } from '@testing-library/react';
import { useRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCompositionSubtitleEditor } from '@/hooks/useCompositionSubtitleEditor';
import { useS2sNarration } from '@/hooks/useS2sNarration';
import { narrationProjectEpoch, narrationRunNamespace, setNarrationProjectId } from '@/hooks/narrationProjectOwnership';
import { readResumableRun } from '@/hooks/resumableJobRegistry';
import { createSubtitleTrackStateFromSegments, getSubtitleTracks } from '@/lib/subtitleTracks';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { invoke } from '@/lib/ipc';
import en from '@/i18n/en';
import type { NarrationSegment, Project, ProjectComposition, VideoSegment } from '@/types/video';

vi.mock('@/lib/ipc', () => ({ invoke: vi.fn(), logToHost: vi.fn() }));
beforeEach(() => { localStorage.clear(); vi.mocked(invoke).mockReset(); });
afterEach(() => { localStorage.clear(); });

function harness(sourceType: 'audio' | 'video', failWrite = false) {
  const local = { trimStart: 8, trimEnd: 10, textSegments: [],
    ...createSubtitleTrackStateFromSegments([]) } as VideoSegment;
  const initial = { mode: 'separate', clips: [{ id: 'clip', duration: 10, segment: local, rawVideoPath: 'video.mp4' }],
    audioSegments: [{ id: 'audio', name: 'Audio', rawAudioPath: 'audio.wav', duration: 8,
      inPoint: 2, outPoint: 6, startTime: 0.5, playbackRate: 2, addedAt: 1 }],
    globalSegment: { ...local, ...createSubtitleTrackStateFromSegments([
      { id: 'existing', text: 'Existing', startTime: 0.25, endTime: 0.5, style: defaultSubtitleStyle(),
        sourceGroup: { kind: 'audio', audioSegmentId: 'audio' } },
    ]) },
  } as ProjectComposition;
  const owner = { current: null as string | null };
  setNarrationProjectId(owner, `canonical-${sourceType}-${failWrite}`);
  const finalize = vi.fn();
  const applyAudio = vi.fn<(segments: NarrationSegment[], ids: string[]) => void>();
  let rejectWrite = failWrite;
  vi.mocked(invoke).mockImplementation((command) => {
    if (command === 'allocate_narration_job_id' || command === 'start_s2s_narration') return Promise.resolve({ jobId: 'job' });
    if (command === 'get_s2s_narration_status') {
      const startTime = sourceType === 'audio' ? 2 : 8.2;
      const clipId = sourceType === 'audio' ? 'audio:audio' : 'clip';
      return Promise.resolve({ state: 'completed', message: 'Done', totalClips: 1, completedClips: 1,
        resultsRevision: 1, results: [{ clipId, isPartial: false, segments: [{ id: 'cue', clipId,
          startTime, endTime: startTime + 1, sourceText: 'Source', targetText: 'Target', path: 'take.wav', duration: 1 }] }] });
    }
    return Promise.resolve(undefined);
  });
  const hook = renderHook(() => {
    const segmentRef = useRef<VideoSegment | null>(local);
    const projectRef = useRef<Project | null>({ id: owner.current, composition: initial } as Project);
    const setComposition = (next: ProjectComposition | null) => {
      if (rejectWrite) throw new Error('Destination unavailable');
      projectRef.current = { ...projectRef.current!, composition: next ?? undefined };
    };
    const editor = useCompositionSubtitleEditor({ segmentRef, segment: segmentRef.current,
      composition: initial, currentProjectDataRef: projectRef, currentProjectIdRef: owner,
      loadedClipIdRef: { current: 'clip' }, setSegment: (next) => {
        segmentRef.current = typeof next === 'function' ? next(segmentRef.current) : next;
      }, setComposition, setCompositionSilently: setComposition });
    const s2s = useS2sNarration({ projectId: owner.current, currentProjectIdRef: owner, t: en,
      getNarrationSourceSnapshot: () => ({ composition: projectRef.current!.composition!,
        segment: editor.segmentRef.current, localSegment: segmentRef.current, loadedClipId: 'clip', videoPath: 'video.mp4', micPath: '' }),
      segment: editor.segment, composition: initial, duration: 10, sourceType, targetLanguage: 'en',
      geminiModel: 'm', geminiVoice: 'v', geminiSpeed: '1', parallelRequests: 1, groupTextBudget: 25, targetSegmentSec: 4,
      onApplyNarrationSegments: applyAudio, onPopulateEmptySubtitles: editor.populateS2sSubtitles, onFinalize: finalize });
    return { s2s, segmentRef, projectRef, editor };
  });
  return { hook, finalize, applyAudio, owner, permitWrite: () => { rejectWrite = false; } };
}

describe('S2S canonical subtitle destination', () => {
  it.each(['audio', 'video'] as const)('applies %s results through the real poll and destination writer', async (sourceType) => {
    const { hook, finalize, applyAudio, owner } = harness(sourceType);
    await act(async () => { await hook.result.current.s2s.handleGenerate(); });
    await waitFor(() => expect(finalize).toHaveBeenCalledOnce());
    const composition = hook.result.current.projectRef.current!.composition!;
    const destination = sourceType === 'audio' ? composition.globalSegment! : hook.result.current.segmentRef.current!;
    const cue = getSubtitleTracks(destination).find((track) => track.kind === 'original')!.segments[0];
    expect(cue).toMatchObject({ id: 'cue-source', text: 'Source', startTime: sourceType === 'audio' ? 0.5 : 8.2,
      endTime: sourceType === 'audio' ? 1 : 9.2 });
    if (sourceType === 'audio') {
      expect(hook.result.current.segmentRef.current?.subtitleSegments).toEqual([]);
      expect(composition.clips[0].segment.subtitleSegments).toEqual([]);
    } else {
      expect(composition.globalSegment?.subtitleSegments?.[0].id).toBe('existing');
    }
    expect(applyAudio).toHaveBeenCalledOnce();
    expect(applyAudio.mock.calls[0][0][0].startTime).toBeCloseTo(sourceType === 'audio' ? 0.5 : 0.2);
    expect(hook.result.current.s2s.isGenerating).toBe(false);
    hook.unmount(); setNarrationProjectId(owner, null);
  });

  it('retains a failed canonical write and acknowledges only after retry stores it', async () => {
    const { hook, finalize, applyAudio, owner, permitWrite } = harness('audio', true);
    await act(async () => { await hook.result.current.s2s.handleGenerate(); });
    await waitFor(() => expect(hook.result.current.s2s.applyRetryable).toBe(true));
    const namespace = narrationRunNamespace(owner.current, 's2s', narrationProjectEpoch(owner));
    expect(readResumableRun(namespace)).toBeDefined();
    expect(finalize).not.toHaveBeenCalled();
    expect(applyAudio).not.toHaveBeenCalled();
    expect(hook.result.current.projectRef.current!.composition!.globalSegment!.subtitleSegments![0].id).toBe('existing');
    permitWrite();
    await act(async () => { await hook.result.current.s2s.handleRetry(); });
    await waitFor(() => expect(finalize).toHaveBeenCalledOnce());
    expect(readResumableRun(namespace)).toBeUndefined();
    expect(hook.result.current.projectRef.current!.composition!.globalSegment!.subtitleSegments![0].id).toBe('cue-target');
    hook.unmount(); setNarrationProjectId(owner, null);
  });
});
