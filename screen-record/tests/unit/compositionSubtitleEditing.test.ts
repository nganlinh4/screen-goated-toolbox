import { describe, expect, it } from 'vitest';
import { editCompositionSubtitleView } from '@/lib/compositionSubtitleEditing';
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';
import { createSubtitleTrackStateFromSegments, getSubtitleTracks, normalizeSubtitleTrackState } from '@/lib/subtitleTracks';
import { deleteSubtitleIdsAcrossTracks, updateSubtitleStylesAcrossTracks } from '@/lib/subtitleTrackMutations';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import type { ProjectComposition, VideoSegment } from '@/types/video';

function fixture() {
  const local = { trimStart: 8, trimEnd: 21,
    trimSegments: [{ id: 'a', startTime: 8, endTime: 9 }, { id: 'b', startTime: 20, endTime: 21 }],
    textSegments: [], ...createSubtitleTrackStateFromSegments([]),
  } as VideoSegment;
  const root = { ...local, trimStart: 0, trimEnd: 1, trimSegments: [{ id: 'root', startTime: 0, endTime: 1 }] };
  const clip = { id: 'root', name: 'Root', role: 'root' as const, duration: 1, segment: root,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
  };
  const cue = { id: 'shared-cue', startTime: 0.5, endTime: 2.5,
    splitGroupStartTime: 0.5, splitGroupEndTime: 2.5,
    text: 'Before', style: defaultSubtitleStyle(), sourceGroup: { kind: 'audio' as const, audioSegmentId: 'owner' },
  };
  const composition: ProjectComposition = { mode: 'unified', focusedClipId: 'clip', selectedClipId: 'clip',
    clips: [clip, { ...clip, id: 'clip', duration: 21, segment: local }],
    globalSegment: { ...root, ...createSubtitleTrackStateFromSegments([cue]) },
  };
  return { local, composition, view: getCompositionClipRenderSegment(composition, 'clip', local) };
}

describe('explicit edits of sequence-owned subtitle views', () => {
  it('preserves ambiguous legacy local copies and refuses to infer their owner from a shared ID', () => {
    const { local, composition } = fixture();
    const legacy = { ...composition.globalSegment!.subtitleSegments![0], text: 'Saved user edit', startTime: 8, endTime: 9 };
    const stored = { ...local, ...createSubtitleTrackStateFromSegments([legacy]) };
    const view = getCompositionClipRenderSegment(composition, 'clip', stored);
    const next = editCompositionSubtitleView(composition, 'clip', stored, { ...view, textSegments: [{ id: 'other' }] as VideoSegment['textSegments'] });
    expect(next.segment.subtitleSegments).toEqual([legacy]);
    expect(next.composition.globalSegment?.subtitleSegments?.[0].text).toBe('Before');
    expect(() => editCompositionSubtitleView(composition, 'clip', stored, deleteSubtitleIdsAcrossTracks(view, ['shared-cue'])))
      .toThrow('ownership is ambiguous');
    expect(stored.subtitleSegments).toEqual([legacy]);
  });
  it('preserves full crossing-cue bounds when text or style changes', () => {
    const { local, composition, view } = fixture();
    const styled = updateSubtitleStylesAcrossTracks(view, new Set(['shared-cue']), (cue) => ({
      ...cue, style: { ...cue.style, x: 17 },
    }));
    const changed = normalizeSubtitleTrackState({ ...styled, subtitleTracks: getSubtitleTracks(styled).map((track) => ({
      ...track, segments: track.segments.map((cue) => ({ ...cue, text: 'After' })),
    })) });
    const next = editCompositionSubtitleView(composition, 'clip', local, changed);
    expect(next.composition.globalSegment?.subtitleSegments).toEqual([
      expect.objectContaining({ text: 'After', startTime: 0.5, endTime: 2.5,
        splitGroupStartTime: 0.5, splitGroupEndTime: 2.5, style: expect.objectContaining({ x: 17 }) }),
    ]);
    expect(next.segment.subtitleSegments).toEqual([]);
    expect(getCompositionClipRenderSegment(next.composition, 'root', composition.clips[0].segment).subtitleSegments?.[0].text).toBe('After');
    expect(composition.globalSegment?.subtitleSegments?.[0].text).toBe('Before');
  });

  it('maps only explicitly changed timing edges back to sequence time', () => {
    const { local, composition, view } = fixture();
    const edited = normalizeSubtitleTrackState({ ...view, subtitleTracks: getSubtitleTracks(view).map((track) => ({ ...track,
      segments: track.segments.map((cue) => ({ ...cue, startTime: 8.25 })),
    })) });
    const next = editCompositionSubtitleView(composition, 'clip', local, edited);
    expect(next.composition.globalSegment?.subtitleSegments?.[0]).toMatchObject({ startTime: 1.25, endTime: 2.5,
      splitGroupStartTime: 0.5, splitGroupEndTime: 2.5 });
  });

  it('deletes the canonical cue without putting any projected cue in local storage', () => {
    const { local, composition, view } = fixture();
    const next = editCompositionSubtitleView(composition, 'clip', local, deleteSubtitleIdsAcrossTracks(view, ['shared-cue']));
    expect(next.composition.globalSegment?.subtitleSegments).toEqual([]);
    expect(next.segment.subtitleSegments).toEqual([]);
  });

  it('keeps the full canonical range when a translated track reuses source IDs', () => {
    const { local, composition, view } = fixture();
    const edited = normalizeSubtitleTrackState({ ...view, subtitleTracks: [
      ...getSubtitleTracks(view), { id: 'translated', kind: 'translation', targetLanguage: 'en',
        segments: view.subtitleSegments!.map((cue) => ({ ...cue, text: 'Translated' })) },
    ], activeSubtitleView: { kind: 'track', trackId: 'translated' } });
    const next = editCompositionSubtitleView(composition, 'clip', local, edited);
    expect(getSubtitleTracks(next.composition.globalSegment).find((track) => track.id === 'translated')?.segments).toEqual([
      expect.objectContaining({ text: 'Translated', startTime: 0.5, endTime: 2.5 }),
    ]);
    expect(getSubtitleTracks(next.segment).find((track) => track.id === 'translated')).toBeUndefined();
  });

  it('does not publish an unchanged projection', () => {
    const { local, composition, view } = fixture();
    const next = editCompositionSubtitleView(composition, 'clip', local, view);
    expect(next.segment).toBe(local);
    expect(next.composition).toBe(composition);
  });

  it('rejects missing owners before any mutation', () => {
    const { local, composition, view } = fixture();
    expect(() => editCompositionSubtitleView(composition, 'retired', local, view)).toThrow('no longer available');
  });
});
