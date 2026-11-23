// Uncommitted imported-audio edits (rate/trim drafts from the Audio panel).
//
// The panel writes drafts here while a slider is moving; the timeline track
// merges them into its segments and renders the preview declaratively. Drafts
// are cleared on commit, so the committed project state is the only source
// once the gesture ends.

import { useSyncExternalStore } from 'react';
import type { ImportedAudioSegment } from '@/types/video';

export type ImportedAudioDraft = Partial<Pick<ImportedAudioSegment, 'playbackRate' | 'inPoint' | 'outPoint'>>;
export type ImportedAudioDrafts = ReadonlyMap<string, ImportedAudioDraft>;

const EMPTY_DRAFTS: ImportedAudioDrafts = new Map();
let drafts: ImportedAudioDrafts = EMPTY_DRAFTS;
const listeners = new Set<() => void>();

function publish(next: Map<string, ImportedAudioDraft>) {
  drafts = next.size === 0 ? EMPTY_DRAFTS : next;
  for (const listener of listeners) listener();
}

export function setImportedAudioDraft(id: string, patch: ImportedAudioDraft): void {
  const next = new Map(drafts);
  next.set(id, { ...(drafts.get(id) ?? {}), ...patch });
  publish(next);
}

export function clearImportedAudioDrafts(ids: Iterable<string>): void {
  const next = new Map(drafts);
  let changed = false;
  for (const id of ids) changed = next.delete(id) || changed;
  if (changed) publish(next);
}

export function retainImportedAudioDrafts(liveIds: ReadonlySet<string>): void {
  clearImportedAudioDrafts([...drafts.keys()].filter((id) => !liveIds.has(id)));
}

export function getImportedAudioDrafts(): ImportedAudioDrafts {
  return drafts;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useImportedAudioDrafts(): ImportedAudioDrafts {
  return useSyncExternalStore(subscribe, getImportedAudioDrafts, getImportedAudioDrafts);
}

/** Returns `segments` unchanged (same identity) when no segment has a draft. */
export function applyImportedAudioDrafts(
  segments: ImportedAudioSegment[],
  activeDrafts: ImportedAudioDrafts,
): ImportedAudioSegment[] {
  if (activeDrafts.size === 0) return segments;
  let changed = false;
  const next = segments.map((segment) => {
    const draft = activeDrafts.get(segment.id);
    if (!draft) return segment;
    changed = true;
    return { ...segment, ...draft };
  });
  return changed ? next : segments;
}
