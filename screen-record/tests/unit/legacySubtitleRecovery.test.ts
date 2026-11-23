import { describe, expect, it } from 'vitest';
import { recoverLegacyProjectSubtitleCopies, recoverLegacySubtitleCopies } from '@/lib/legacySubtitleRecovery';
import { editCompositionSubtitleView } from '@/lib/compositionSubtitleEditing';
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';
import {
  createSubtitleTrackStateFromSegments,
  getSubtitleTrackLabel,
  getSubtitleTracks,
  getVisibleSubtitleSegments,
  RECOVERED_SUBTITLE_TRACK_PREFIX,
} from '@/lib/subtitleTracks';
import { deleteSubtitleIdsAcrossTracks } from '@/lib/subtitleTrackMutations';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import type { Project, ProjectComposition, SubtitleSegment, VideoSegment } from '@/types/video';

const RECOVERED_ORIGINAL = `${RECOVERED_SUBTITLE_TRACK_PREFIX}subtitle-track-original`;

function fixture(localCues: SubtitleSegment[] = []) {
  const base = { trimStart: 8, trimEnd: 21,
    trimSegments: [{ id: 'a', startTime: 8, endTime: 9 }, { id: 'b', startTime: 20, endTime: 21 }],
    textSegments: [], ...createSubtitleTrackStateFromSegments([]),
  } as VideoSegment;
  const local = { ...base, ...createSubtitleTrackStateFromSegments(localCues) };
  const root = { ...base, trimStart: 0, trimEnd: 1, trimSegments: [{ id: 'root', startTime: 0, endTime: 1 }] };
  const clip = { id: 'root', name: 'Root', role: 'root' as const, duration: 1, segment: root,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
  };
  const cue: SubtitleSegment = { id: 'shared-cue', startTime: 0.5, endTime: 2.5,
    splitGroupStartTime: 0.5, splitGroupEndTime: 2.5,
    text: 'Canonical', style: defaultSubtitleStyle(), sourceGroup: { kind: 'audio', audioSegmentId: 'owner' },
  };
  const composition: ProjectComposition = { mode: 'unified', focusedClipId: 'clip', selectedClipId: 'clip',
    clips: [clip, { ...clip, id: 'clip', role: 'snapshot' as never, duration: 21, segment: local }],
    globalSegment: { ...root, ...createSubtitleTrackStateFromSegments([cue]) },
  };
  return { cue, local, composition };
}

const clipSegment = (composition: ProjectComposition) => composition.clips.find((clip) => clip.id === 'clip')!.segment;

describe('legacy clip-local copies of sequence captions', () => {
  it('drops exact copies on either clock and unblocks editing the canonical caption', () => {
    const { cue } = fixture();
    const clipClock = { ...cue, startTime: 8, endTime: 20.5, splitGroupStartTime: 8, splitGroupEndTime: 20.5 };
    for (const copy of [cue, clipClock]) {
      const { composition } = fixture([copy]);
      const stored = clipSegment(composition);
      const view = getCompositionClipRenderSegment(composition, 'clip', stored);
      expect(() => editCompositionSubtitleView(composition, 'clip', stored, deleteSubtitleIdsAcrossTracks(view, ['shared-cue'])))
        .toThrow('ownership is ambiguous');

      const recovered = recoverLegacySubtitleCopies(composition);
      const local = clipSegment(recovered);
      expect(getSubtitleTracks(local).flatMap((track) => track.segments)).toEqual([]);
      expect(getVisibleSubtitleSegments(getCompositionClipRenderSegment(recovered, 'clip', local)))
        .toEqual(getVisibleSubtitleSegments(view));
      const edited = editCompositionSubtitleView(recovered, 'clip', local,
        deleteSubtitleIdsAcrossTracks(getCompositionClipRenderSegment(recovered, 'clip', local), ['shared-cue']));
      expect(edited.composition.globalSegment?.subtitleSegments).toEqual([]);
    }
  });

  it('keeps a diverging user edit in a separate recovered track without changing what renders', () => {
    const { composition: plain } = fixture();
    const legacy = { ...plain.globalSegment!.subtitleSegments![0], text: 'Saved user edit', startTime: 8, endTime: 9 };
    const { composition } = fixture([legacy]);
    const before = getCompositionClipRenderSegment(composition, 'clip', clipSegment(composition));

    const recovered = recoverLegacySubtitleCopies(composition);
    const local = clipSegment(recovered);
    const tracks = getSubtitleTracks(local);
    expect(tracks.find((track) => track.id === 'subtitle-track-original')?.segments).toEqual([]);
    const rescue = tracks.find((track) => track.id === RECOVERED_ORIGINAL)!;
    expect(getSubtitleTrackLabel(rescue)).toBe('Recovered (Original)');
    expect(rescue.targetLanguage).toBeNull();
    expect(rescue.segments).toEqual([{ ...legacy, id: 'shared-cue-recovered', sourceGroup: undefined }]);
    expect(rescue.segments[0].sourceGroup).toBeUndefined();
    expect(rescue.segments[0].provenance).toBeUndefined();
    expect(local.activeSubtitleView).toEqual({ kind: 'track', trackId: 'subtitle-track-original' });
    const view = getCompositionClipRenderSegment(recovered, 'clip', local);
    expect(getVisibleSubtitleSegments(view)).toEqual(getVisibleSubtitleSegments(before));

    const edited = editCompositionSubtitleView(recovered, 'clip', local, deleteSubtitleIdsAcrossTracks(view, ['shared-cue']));
    expect(edited.composition.globalSegment?.subtitleSegments).toEqual([]);
    expect(getSubtitleTracks(edited.segment).find((track) => track.id === RECOVERED_ORIGINAL)?.segments)
      .toEqual(rescue.segments);
    expect(recoverLegacySubtitleCopies(recovered)).toBe(recovered);
  });

  it('moves a diverging copy still on the sequence clock onto its clip clock', () => {
    const { cue } = fixture();
    const { composition } = fixture([{ ...cue, text: 'Retyped' }]);
    const local = clipSegment(recoverLegacySubtitleCopies(composition));
    expect(getSubtitleTracks(local).find((track) => track.id === RECOVERED_ORIGINAL)?.segments).toEqual([
      expect.objectContaining({ id: 'shared-cue-recovered', text: 'Retyped', startTime: 8, endTime: 20.5,
        splitGroupStartTime: 8, splitGroupEndTime: 20.5 }),
    ]);
  });

  it('recovers the root clip through project.segment so the editor and composition agree', () => {
    const { cue, composition } = fixture();
    const rootSegment = { ...composition.clips[0].segment, ...createSubtitleTrackStateFromSegments([{ ...cue, text: 'Root edit' }]) };
    const project = { id: 'p', name: 'P', createdAt: 0, lastModified: 0, duration: 1, segment: rootSegment,
      backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [], composition,
    } as unknown as Project;
    const recovered = recoverLegacyProjectSubtitleCopies(project);
    expect(recovered.composition?.clips[0].segment).toBe(recovered.segment);
    expect(getSubtitleTracks(recovered.segment).find((track) => track.id === RECOVERED_ORIGINAL)?.segments)
      .toEqual([expect.objectContaining({ text: 'Root edit', startTime: 0.5, endTime: 1 })]);
    expect(recoverLegacyProjectSubtitleCopies(recovered)).toBe(recovered);
    const untouched = { ...project, segment: composition.clips[0].segment };
    expect(recoverLegacyProjectSubtitleCopies(untouched)).toBe(untouched);
  });
});
