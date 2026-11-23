import { describe, expect, it } from 'vitest';
import { buildSubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import { buildInsertedSubtitle } from '@/hooks/subtitleGenerationResults';
import { buildNarration, buildSubtitle } from '@/hooks/s2sNarrationSubtitles';
import { getAudioLocalSubtitleTiming, inferAudioSourceGroupAtRange } from '@/lib/subtitleSourceGroups';
import { materializeNarrationGroupTakes } from '@/lib/narrationGroupTakes';
import type { ImportedAudioSegment, ProjectComposition } from '@/types/video';

const audio: ImportedAudioSegment = {
  id: 'audio', rawAudioPath: 'input.wav', name: 'Input', duration: 20,
  startTime: 10, inPoint: 4, outPoint: 12, playbackRate: 2, addedAt: 0,
};

function makePlan(segment: ImportedAudioSegment, range?: { startTime: number; endTime: number }) {
  return buildSubtitleGenerationPlan({
    segment: null,
    composition: { audioSegments: [segment] } as ProjectComposition,
    activeClipId: null,
    currentRawVideoPath: '',
    currentRawMicAudioPath: '',
    duration: 30,
    sourceType: 'audio',
    selectedRange: range ?? null,
  });
}

describe('audio subtitle timing', () => {
  it('maps a timeline selection to the correct source range at both fast and slow rates', () => {
    const fast = makePlan(audio, { startTime: 11, endTime: 13 });
    expect(fast.clips[0].trimSegments).toEqual([{ id: 'subtitle-range-0', startTime: 6, endTime: 10 }]);
    expect(fast.replacementRangesByClip['audio:audio']).toEqual([{ startTime: 11, endTime: 13 }]);
    expect(makePlan(audio, { startTime: 14, endTime: 15 }).clips).toEqual([]);
    const slow = makePlan({ ...audio, playbackRate: 0.5 }, { startTime: 12, endTime: 16 });
    expect(slow.clips[0].trimSegments).toEqual([{ id: 'subtitle-range-0', startTime: 5, endTime: 7 }]);
  });

  it('maps returned cues and split group bounds while retaining original source provenance', () => {
    const plan = makePlan(audio);
    const entry = { startTime: 6, endTime: 10, text: 'words', splitGroupStartTime: 4, splitGroupEndTime: 12 };
    const result = { clipId: 'audio:audio', isPartial: false, segments: [entry] };
    const subtitle = buildInsertedSubtitle(result, entry, 0, { fontSize: 30, color: '#fff', x: 50, y: 50 }, plan.clipTransformsByClip[result.clipId]);
    expect(subtitle).toMatchObject({
      startTime: 11, endTime: 13, splitGroupStartTime: 10, splitGroupEndTime: 14,
      provenance: { sourceLocalStartTime: 2, sourceLocalEndTime: 6 },
    });
    expect(getAudioLocalSubtitleTiming(subtitle, audio)).toEqual({ startTime: 6, endTime: 10 });
    expect(inferAudioSourceGroupAtRange(13.5, 13.9, [audio]).audioSegmentId).toBe('audio');
    expect(inferAudioSourceGroupAtRange(14.1, 15, [audio]).kind).toBe('unassigned');
  });

  it('keeps speech-to-speech captions and grouped narration synchronized at the audio rate', () => {
    const plan = makePlan(audio);
    const result = { id: 'first', clipId: 'audio:audio', sourceText: 'a', targetText: 'b',
      startTime: 6, endTime: 8, narrationStartTime: 6, narrationGroupSourceStartTime: 4,
      path: 'generated.wav', duration: 8, audioInPoint: 2, audioOutPoint: 4, narrationGroupTakeId: 'group' };
    expect(buildSubtitle(result, plan, 'source', 'a')).toMatchObject({ startTime: 11, endTime: 12 });
    const first = buildNarration(result, plan, 'batch', 'method');
    const second = buildNarration({ ...result, id: 'second', startTime: 8, endTime: 10,
      narrationStartTime: 8, audioInPoint: 4, audioOutPoint: 6 }, plan, 'batch', 'method');
    expect(first).toMatchObject({ startTime: 11, playbackRate: 2, narrationGroupSourceStartTime: 10 });
    expect(materializeNarrationGroupTakes([first, second])[0]).toMatchObject({ startTime: 11, inPoint: 2, outPoint: 6, playbackRate: 2 });
  });

  it('treats missing rates from older jobs as normal speed', () => {
    const plan = makePlan({ ...audio, playbackRate: undefined });
    const transform = { ...plan.clipTransformsByClip['audio:audio'], playbackRate: undefined };
    const entry = { startTime: 6, endTime: 8, text: 'words' };
    expect(buildInsertedSubtitle({ clipId: 'audio:audio', isPartial: false, segments: [entry] }, entry, 0,
      { fontSize: 30, color: '#fff', x: 50, y: 50 }, transform)).toMatchObject({ startTime: 12, endTime: 14 });
  });
});
