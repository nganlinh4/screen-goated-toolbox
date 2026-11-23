import type {
  Project,
  ProjectComposition,
  ProjectCompositionClip,
  SubtitleSegment,
  SubtitleTrack,
  VideoSegment,
} from '@/types/video';
import {
  getSubtitleTrackLabel,
  getSubtitleTracks,
  normalizeSubtitleTrackState,
  RECOVERED_SUBTITLE_TRACK_PREFIX,
} from '@/lib/subtitleTracks';
import { isSequenceAudioSubtitle } from '@/lib/compositionSubtitles';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  sequenceTimeToClipSourceTime,
  type SequenceTimelineClip,
} from '@/lib/sequenceTimeline';

// Older builds wrote the merged sequence view (and sequence-timed narration
// captions) into clip storage. Those clip-local copies share IDs with the
// canonical sequence-audio captions in globalSegment, are shadowed in every
// view, and block edits of the canonical caption because their owner cannot be
// inferred. Recovery runs once per load: an exact copy (in either clock) carries
// nothing the canonical caption lacks and is dropped; any copy that differs is
// kept, under a fresh ID, in a separate non-active "Recovered" track so the
// user can review it. No caption text, style or timing is ever discarded.

const TIME_KEYS = ['startTime', 'endTime', 'splitGroupStartTime', 'splitGroupEndTime'] as const;
const EPSILON = 1e-6;

type TimeMap = (time: number) => number;
type CanonicalIndex = { byTrack: Map<string, Map<string, SubtitleSegment>>; byId: Map<string, SubtitleSegment> };

function stableKey(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableKey).join(',')}]`;
  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, entry]) => entry !== undefined)
      .sort(([left], [right]) => left.localeCompare(right));
    return `{${entries.map(([key, entry]) => `${JSON.stringify(key)}:${stableKey(entry)}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function contentKey(cue: SubtitleSegment): string {
  const rest: Record<string, unknown> = { ...cue };
  for (const key of TIME_KEYS) delete rest[key];
  return stableKey(rest);
}

function sameTimes(cue: SubtitleSegment, canonical: SubtitleSegment, map: TimeMap): boolean {
  return TIME_KEYS.every((key) => {
    const expected = canonical[key];
    const actual = cue[key];
    if (expected === undefined || actual === undefined) return expected === actual;
    return Math.abs(map(expected) - actual) <= EPSILON;
  });
}

function indexCanonicalAudio(global: VideoSegment): CanonicalIndex {
  const byTrack = new Map<string, Map<string, SubtitleSegment>>();
  const byId = new Map<string, SubtitleSegment>();
  for (const track of getSubtitleTracks(global)) {
    const cues = new Map<string, SubtitleSegment>();
    for (const cue of track.segments.filter(isSequenceAudioSubtitle)) {
      cues.set(cue.id, cue);
      if (!byId.has(cue.id)) byId.set(cue.id, cue);
    }
    byTrack.set(track.id, cues);
  }
  return { byTrack, byId };
}

function uniqueId(base: string, taken: Set<string>): string {
  let candidate = `${base}-recovered`;
  for (let index = 2; taken.has(candidate); index += 1) candidate = `${base}-recovered-${index}`;
  taken.add(candidate);
  return candidate;
}

function recoveredCue(
  cue: SubtitleSegment,
  canonical: SubtitleSegment,
  toClip: TimeMap | null,
  taken: Set<string>,
): SubtitleSegment {
  // A copy still on the sequence clock is moved onto its clip's clock; a copy
  // whose timing matches neither clock keeps exactly what was saved.
  const projected = toClip && sameTimes(cue, canonical, (time) => time)
    ? Object.fromEntries(TIME_KEYS.map((key) => [key, cue[key] === undefined ? undefined : toClip(cue[key]!)]))
    : null;
  const times = projected && projected.endTime! > projected.startTime! ? projected : {};
  // Without its sequence-audio markers the copy is plain clip-owned text, so no
  // audio-lane regeneration or ownership rule can claim or replace it.
  const { sourceGroup: _sourceGroup, provenance: _provenance, ...rest } = cue;
  return { ...rest, ...times, id: uniqueId(cue.id, taken) };
}

function recoverSegment(
  segment: VideoSegment,
  canonical: CanonicalIndex,
  toClip: TimeMap | null,
  taken: Set<string>,
): VideoSegment {
  const tracks = getSubtitleTracks(segment);
  if (!tracks.some((track) => track.segments.some((cue) => canonical.byId.has(cue.id)))) return segment;
  const recovered = new Map<string, SubtitleSegment[]>();
  const nextTracks: SubtitleTrack[] = tracks.map((track) => {
    const kept: SubtitleSegment[] = [];
    for (const cue of track.segments) {
      const owner = canonical.byTrack.get(track.id)?.get(cue.id) ?? canonical.byId.get(cue.id);
      if (!owner) {
        kept.push(cue);
        continue;
      }
      const redundant = contentKey(cue) === contentKey(owner)
        && (sameTimes(cue, owner, (time) => time) || (toClip !== null && sameTimes(cue, owner, toClip)));
      if (redundant) continue;
      recovered.set(track.id, [...(recovered.get(track.id) ?? []), recoveredCue(cue, owner, toClip, taken)]);
    }
    return { ...track, segments: kept };
  });
  for (const [sourceTrackId, cues] of recovered) {
    const id = `${RECOVERED_SUBTITLE_TRACK_PREFIX}${sourceTrackId}`;
    const existing = nextTracks.find((track) => track.id === id);
    if (existing) {
      existing.segments = [...existing.segments, ...cues].sort((left, right) => left.startTime - right.startTime);
      continue;
    }
    const source = tracks.find((track) => track.id === sourceTrackId)!;
    nextTracks.push({ id, kind: 'translation', targetLanguage: null,
      slotLabel: `Recovered (${getSubtitleTrackLabel(source)})`,
      segments: [...cues].sort((left, right) => left.startTime - right.startTime),
    });
  }
  return normalizeSubtitleTrackState({ ...segment, subtitleTracks: nextTracks });
}

function collectCueIds(segments: readonly (VideoSegment | undefined)[]): Set<string> {
  return new Set(segments.flatMap((segment) => getSubtitleTracks(segment)
    .flatMap((track) => track.segments.map((cue) => cue.id))));
}

/** Resolve legacy clip-local copies of canonical sequence-audio captions without losing any edit. */
export function recoverLegacySubtitleCopies(composition: ProjectComposition): ProjectComposition {
  const global = composition.globalSegment;
  if (!global) return composition;
  const canonical = indexCanonicalAudio(global);
  if (canonical.byId.size === 0) return composition;
  const retained = composition.retainedRemovedClips ?? [];
  const taken = collectCueIds([global, ...composition.clips.map((clip) => clip.segment),
    ...retained.map((clip) => clip.segment)]);
  const timeline = buildSequenceTimeline(composition);
  let changed = false;
  const recover = (clip: ProjectCompositionClip, timelineClip: SequenceTimelineClip | null) => {
    const toClip = timelineClip ? (time: number) => sequenceTimeToClipSourceTime(time, timelineClip) : null;
    const segment = recoverSegment(clip.segment, canonical, toClip, taken);
    if (segment === clip.segment) return clip;
    changed = true;
    return { ...clip, segment };
  };
  const clips = composition.clips.map((clip) => recover(clip, getSequenceClipById(timeline, clip.id)));
  const retainedRemovedClips = composition.retainedRemovedClips?.map((clip) => recover(clip, null));
  return changed ? { ...composition, clips, retainedRemovedClips } : composition;
}

/** Load-time entry point: project.segment is the root clip's authoritative segment. */
export function recoverLegacyProjectSubtitleCopies(project: Project): Project {
  const composition = project.composition;
  if (!composition?.globalSegment || !project.segment) return project;
  const withRoot: ProjectComposition = { ...composition,
    clips: composition.clips.map((clip) => clip.role === 'root' ? { ...clip, segment: project.segment } : clip),
  };
  const recovered = recoverLegacySubtitleCopies(withRoot);
  if (recovered === withRoot) return project;
  const root = recovered.clips.find((clip) => clip.role === 'root');
  return { ...project, segment: root?.segment ?? project.segment, composition: recovered };
}
