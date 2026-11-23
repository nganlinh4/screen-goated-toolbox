import type { ProjectComposition, SubtitleSegment, SubtitleTrack, VideoSegment } from '@/types/video';
import { getSubtitleTracks, normalizeSubtitleTrackState, ORIGINAL_SUBTITLE_TRACK_ID } from '@/lib/subtitleTracks';
import { getCompositionClipRenderSegment, isSequenceAudioSubtitle } from '@/lib/compositionSubtitles';
import { buildSequenceTimeline, clipSourceTimeToSequenceTime, getSequenceClipById } from '@/lib/sequenceTimeline';
import { updateCompositionClip } from '@/lib/projectComposition';

const equal = (left: unknown, right: unknown) => JSON.stringify(left) === JSON.stringify(right);

/** Commit an explicit clip-view edit to its separate local and sequence owners. */
export function editCompositionSubtitleView(
  composition: ProjectComposition,
  clipId: string,
  local: VideoSegment,
  edited: VideoSegment,
): { composition: ProjectComposition; segment: VideoSegment } {
  const timeline = buildSequenceTimeline(composition);
  const clip = timeline && getSequenceClipById(timeline, clipId);
  if (!clip) throw new Error('The subtitle clip is no longer available');
  const global = composition.globalSegment;
  if (!global) return { composition, segment: edited };
  const globalTracks = getSubtitleTracks(global);
  const globalAudio = globalTracks.flatMap((track) => track.segments.filter(isSequenceAudioSubtitle));
  if (!globalAudio.length) return { composition, segment: edited };

  const before = getCompositionClipRenderSegment(composition, clipId, local);
  const beforeTracks = getSubtitleTracks(before);
  const editedTracks = getSubtitleTracks(edited);
  const localTracks = getSubtitleTracks(local);
  const localTrackIds = new Set(localTracks.map((track) => track.id));
  const localCueIds = new Set(localTracks.flatMap((track) => track.segments.map((cue) => cue.id)));
  const globalIds = new Set(globalAudio.map((cue) => cue.id));
  const ambiguousIds = new Set([...localCueIds].filter((id) => globalIds.has(id)));
  // Older saved projects can contain a local copy with an unknown clock and
  // independent user edits. Keep it intact until its owner is resolved explicitly.
  for (const track of localTracks) {
    if (track.segments.some((cue) => ambiguousIds.has(cue.id))
      && !editedTracks.some((entry) => entry.id === track.id)) throw new Error('Subtitle ownership is ambiguous');
  }
  for (const track of beforeTracks) {
    const previous = track.segments.filter((cue) => ambiguousIds.has(cue.id));
    const next = editedTracks.find((entry) => entry.id === track.id)?.segments
      .filter((cue) => ambiguousIds.has(cue.id)) ?? [];
    if (!equal(previous, next)) throw new Error('Subtitle ownership is ambiguous');
  }
  const isOwned = (cue: SubtitleSegment) => globalIds.has(cue.id)
    || (isSequenceAudioSubtitle(cue) && !localCueIds.has(cue.id));
  const byTrack = new Map(globalTracks.map((track) => [track.id, track]));
  const editedByTrack = new Map(editedTracks.map((track) => [track.id, track]));

  // Removing a view track is an explicit track operation, unlike editing a
  // visible cue: it retires that track, including its off-clip audio cues.
  for (const track of beforeTracks) {
    if (!editedByTrack.has(track.id)) byTrack.delete(track.id);
  }
  for (const track of editedTracks) {
    const original = globalTracks.find((entry) => entry.id === track.id);
    const visible = beforeTracks.find((entry) => entry.id === track.id)?.segments ?? [];
    const visibleIds = new Set(visible.filter(isOwned).map((cue) => cue.id));
    const nextCues = new Map(track.segments.filter(isOwned).map((cue) => [cue.id, cue]));
    const retained = (original?.segments ?? []).filter((cue) =>
      !isSequenceAudioSubtitle(cue) || !visibleIds.has(cue.id));
    for (const cue of nextCues.values()) {
      const priorView = visible.find((entry) => entry.id === cue.id)
        ?? beforeTracks.flatMap((entry) => entry.segments).find((entry) => entry.id === cue.id);
      const canonical = original?.segments.find((entry) => entry.id === cue.id)
        ?? globalAudio.find((entry) => entry.id === cue.id);
      const time = (key: 'startTime' | 'endTime' | 'splitGroupStartTime' | 'splitGroupEndTime') => {
        const value = cue[key];
        if (canonical && priorView && value === priorView[key]) return canonical[key];
        return value === undefined ? undefined : clipSourceTimeToSequenceTime(value, clip);
      };
      const updated = { ...cue, startTime: time('startTime')!, endTime: time('endTime')!,
        splitGroupStartTime: time('splitGroupStartTime'), splitGroupEndTime: time('splitGroupEndTime'),
      };
      if (!Number.isFinite(updated.startTime) || !Number.isFinite(updated.endTime)
        || updated.endTime <= updated.startTime) throw new Error('Subtitle timing is invalid');
      // A newly translated track can reuse a source cue's ID. Preserve its
      // complete canonical range even when the editor sees only one clipped part.
      const existing = retained.findIndex((entry) => entry.id === cue.id);
      if (existing >= 0) retained.splice(existing, 1);
      retained.push(updated);
    }
    if (original || nextCues.size > 0) byTrack.set(track.id, { ...original, ...track,
      segments: retained.sort((left, right) => left.startTime - right.startTime),
    });
  }
  const nextGlobal = normalizeSubtitleTrackState({ ...global,
    subtitleTracks: [...byTrack.values()],
    activeSubtitleView: edited.activeSubtitleView,
    subtitleCustomChain: edited.subtitleCustomChain,
  });
  const nextLocalTracks: SubtitleTrack[] = editedTracks.flatMap((track) => {
    const preserved = localTracks.find((entry) => entry.id === track.id)?.segments
      .filter((cue) => ambiguousIds.has(cue.id)) ?? [];
    const segments = [...track.segments.filter((cue) => !isOwned(cue)), ...preserved];
    return track.id === ORIGINAL_SUBTITLE_TRACK_ID || localTrackIds.has(track.id) || segments.length
      ? [{ ...track, segments }] : [];
  });
  const nextLocal = normalizeSubtitleTrackState({ ...edited, subtitleTracks: nextLocalTracks });
  const segment = equal(nextLocal, local) ? local : nextLocal;
  let nextComposition = equal(nextGlobal, global) ? composition : { ...composition, globalSegment: nextGlobal };
  if (segment !== local) nextComposition = updateCompositionClip(nextComposition, clipId, { segment });
  return { composition: nextComposition, segment };
}
