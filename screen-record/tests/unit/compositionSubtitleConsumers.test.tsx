import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { buildCompositionTranslationItems, applyCompositionTranslationResults } from '@/hooks/subtitleTranslationHelpers';
import { useSubtitleNarrationTargets } from '@/hooks/useSubtitleNarrationTargets';
import { projectSubtitleForNarration } from '@/lib/compositionSubtitleNarration';
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';
import { createSubtitleTrackStateFromSegments, getSubtitleTracks } from '@/lib/subtitleTracks';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import type { ProjectComposition, VideoSegment } from '@/types/video';

function fixture(): ProjectComposition {
  const video = { id: 'video', startTime: 8.2, endTime: 8.4, text: 'Video', style: defaultSubtitleStyle(),
    sourceGroup: { kind: 'video' as const } };
  const audio = { id: 'audio', startTime: 0.5, endTime: 2.5, text: 'Audio', style: defaultSubtitleStyle(),
    sourceGroup: { kind: 'audio' as const, audioSegmentId: 'owner' } };
  const root = { trimStart: 0, trimEnd: 1, trimSegments: [{ id: 'root', startTime: 0, endTime: 1 }],
    ...createSubtitleTrackStateFromSegments([]),
  } as VideoSegment;
  const clip = { id: 'root', role: 'root' as const, name: 'Root', duration: 1, segment: root,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
  };
  return { mode: 'unified', focusedClipId: 'clip', selectedClipId: 'clip', clips: [clip,
    { ...clip, id: 'clip', duration: 10, segment: { ...root, trimStart: 8, trimEnd: 10,
      trimSegments: [{ id: 'local', startTime: 8, endTime: 10 }], ...createSubtitleTrackStateFromSegments([video]) } },
  ], globalSegment: { ...root, trimEnd: 3, ...createSubtitleTrackStateFromSegments([audio,
    { ...video, startTime: 1.2, endTime: 1.4, text: 'Stale video mirror' },
  ]) } };
}

describe('sequence subtitle consumers', () => {
  it('collects canonical audio once without a synthetic root-clip owner', () => {
    const composition = fixture();
    const all = buildCompositionTranslationItems(composition, [], null, 'all');
    expect(all.map((item) => item.text)).toEqual(['Video', 'Audio']);
    expect(all.find((item) => item.id === 'audio')?.clipId).toBeNull();
    expect(buildCompositionTranslationItems(composition, ['audio'], null, 'all')).toHaveLength(1);
    expect(buildCompositionTranslationItems(composition, [], null, 'audio').map((item) => item.id)).toEqual(['audio']);
    expect(JSON.parse(JSON.stringify(all)).find((item: { id: string }) => item.id === 'audio').clipId).toBeNull();
  });

  it('applies mixed clip and global results without changing the audio cue clock', () => {
    const composition = fixture();
    const next = applyCompositionTranslationResults(composition, [
      { id: 'audio', clipId: null, translatedText: 'Translated audio' },
      { id: 'video', clipId: 'clip', translatedText: 'Translated video' },
    ], { targetLanguage: 'en', targetTrackId: null });
    const globalTrack = getSubtitleTracks(next.globalSegment).find((track) => track.kind === 'translation');
    expect(globalTrack?.segments.find((cue) => cue.id === 'audio')).toMatchObject({ text: 'Translated audio', startTime: 0.5, endTime: 2.5 });
    const clipTrack = getSubtitleTracks(next.clips[1].segment).find((track) => track.kind === 'translation');
    expect(clipTrack?.segments).toEqual([expect.objectContaining({ id: 'video', text: 'Translated video', startTime: 8.2, endTime: 8.4 })]);
    expect(getCompositionClipRenderSegment(next, 'clip', next.clips[1].segment).subtitleSegments?.map((cue) => cue.text))
      .toEqual(['Translated audio', 'Translated video']);
  });

  it('selects in the editor clock then constructs project-wide narration targets', () => {
    const composition = fixture();
    const view = getCompositionClipRenderSegment(composition, 'clip', composition.clips[1].segment);
    const { result } = renderHook(() => useSubtitleNarrationTargets(view.subtitleSegments!, [],
      { startTime: 8, endTime: 8.6, source: 'selection' }, false,
      (subtitle) => projectSubtitleForNarration(composition, 'clip', subtitle)));
    expect(result.current.requestItems).toEqual([
      expect.objectContaining({ id: 'audio', startTime: 0.5, endTime: 2.5, text: 'Audio' }),
      expect.objectContaining({ id: 'video', text: 'Video' }),
    ]);
    expect(result.current.requestItems[1].startTime).toBeCloseTo(1.2);
    expect(result.current.requestItems[1].endTime).toBeCloseTo(1.4);
  });

  it('drops an unresolved clip target rather than guessing another clip clock', () => {
    const composition = fixture();
    const cue = composition.clips[1].segment.subtitleSegments![0];
    expect(projectSubtitleForNarration(composition, 'retired', cue)).toBeNull();
    expect(projectSubtitleForNarration(null, null, cue)).toBe(cue);
  });
});
