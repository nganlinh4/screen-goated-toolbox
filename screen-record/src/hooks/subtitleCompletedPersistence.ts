import { useSyncExternalStore } from 'react';
import type { ProjectComposition, VideoSegment } from '@/types/video';
import { clearSubtitleRecovery, getSubtitleRecovery } from './subtitleRecoveryJournal';

/** Acknowledged saves survive the subtitle hook's unmount within this WebView. */
export interface RetainedSubtitleSave {
  projectId: string;
  composition: ProjectComposition | null;
  segment: VideoSegment | null;
  loadedClipId: string | null;
}
interface PendingSave {
  jobId: string;
  ownerState: () => 'ready' | 'waiting' | 'superseded';
  save: (isCurrent: () => boolean) => Promise<boolean>;
  attempt: number;
  timer: ReturnType<typeof setTimeout> | null;
  inFlight: boolean;
  superseded: boolean;
  unconfirmed: boolean;
  snapshot: RetainedSubtitleSave;
  retainRecovery: boolean;
}

const pending = new Map<string, PendingSave>();
let activeEditor: React.MutableRefObject<string | null> | null = null;
const invalidationListeners = new Map<React.MutableRefObject<string | null>, Set<() => void>>();
const listeners = new Set<() => void>();
const changed = () => listeners.forEach((listener) => listener());
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const currentState = () => {
  const entry = pending.values().next().value as PendingSave | undefined;
  return !entry ? 'none' : entry.superseded ? 'superseded' : entry.unconfirmed ? 'unconfirmed' : 'pending';
};

export function hasPendingSubtitleCompletedSave() { return pending.size > 0; }
export function useSubtitleCompletedSaveState() { return useSyncExternalStore(subscribe, currentState, () => 'none'); }
export function retainedSubtitleSave(): RetainedSubtitleSave | null {
  return (pending.values().next().value as PendingSave | undefined)?.snapshot ?? null;
}

export function invalidateSubtitleSaveEditor(owner: React.MutableRefObject<string | null>) {
  invalidationListeners.get(owner)?.forEach((listener) => listener());
}

export function subscribeSubtitleSaveInvalidation(owner: React.MutableRefObject<string | null>, listener: () => void) {
  const listeners = invalidationListeners.get(owner) ?? new Set<() => void>();
  listeners.add(listener);
  invalidationListeners.set(owner, listeners);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) invalidationListeners.delete(owner);
  };
}

/** Identify the mounted editor, not just the values held by an old editor's refs. */
export function registerSubtitleSaveEditor(owner: React.MutableRefObject<string | null>) {
  if (activeEditor && activeEditor !== owner) invalidateSubtitleSaveEditor(activeEditor);
  activeEditor = owner;
  return () => {
    if (activeEditor === owner) {
      invalidateSubtitleSaveEditor(owner);
      activeEditor = null;
    }
  };
}

export function isSubtitleSaveEditorActive(owner: React.MutableRefObject<string | null>) {
  return activeEditor === owner;
}

function schedule(entry: PendingSave, delay: number) {
  if (entry.timer !== null || entry.inFlight || entry.superseded) return;
  entry.timer = setTimeout(() => {
    entry.timer = null;
    void attemptSave(entry);
  }, delay);
}

async function attemptSave(entry: PendingSave) {
  if (pending.get(entry.jobId) !== entry || entry.inFlight || entry.superseded) return;
  const owner = entry.ownerState();
  if (owner === 'waiting') {
    schedule(entry, 250);
    return;
  }
  if (owner === 'superseded') {
    // A reopened project can contain newer edits or an older disk snapshot.
    // Never replay an old editor snapshot into a different activation.
    entry.superseded = true;
    changed();
    console.warn('[SubtitleGen][Persist] Completed save retained after its project activation changed.');
    return;
  }
  entry.inFlight = true;
  const watchdog = setTimeout(() => {
    if (entry.inFlight && pending.get(entry.jobId) === entry) {
      entry.unconfirmed = true;
      changed();
    }
  }, 30_000);
  try {
    if (await entry.save(() => pending.get(entry.jobId) === entry && !entry.superseded)) {
      if (!entry.retainRecovery && getSubtitleRecovery().record?.jobId === entry.jobId) await clearSubtitleRecovery(entry.jobId);
      if (pending.get(entry.jobId) === entry) { pending.delete(entry.jobId); changed(); }
      return;
    }
  } catch (error) {
    console.warn('[SubtitleGen][Persist] Completed result save failed:', error);
  } finally {
    clearTimeout(watchdog);
    entry.inFlight = false;
  }
  if (pending.get(entry.jobId) !== entry) return;
  entry.attempt += 1;
  entry.unconfirmed = true;
  changed();
  schedule(entry, Math.min(30_000, 250 * 2 ** Math.min(entry.attempt - 1, 7)));
}

export function requestSubtitleCompletedSave(jobId: string, snapshot: RetainedSubtitleSave, ownerState: PendingSave['ownerState'], save: PendingSave['save'], retainRecovery = false): boolean {
  if (pending.size > 0) return false;
  const entry: PendingSave = { jobId, snapshot, ownerState, save, retainRecovery, attempt: 0, timer: null, inFlight: false, superseded: false, unconfirmed: false };
  pending.set(jobId, entry);
  changed();
  // The callback captures refs and checks ownership at dispatch; unmounting a
  // panel does not cancel this task or redirect it to a later project.
  schedule(entry, 0);
  return true;
}

/** Retire the matching in-memory task only after the user explicitly clears its journal. */
export function discardSubtitleCompletedSave(jobId: string) {
  const entry = pending.get(jobId);
  if (!entry) return;
  entry.superseded = true;
  if (entry.timer !== null) clearTimeout(entry.timer);
  if (activeEditor) invalidateSubtitleSaveEditor(activeEditor);
  pending.delete(jobId);
  changed();
}

export function wakeSubtitleCompletedSaves() {
  for (const entry of pending.values()) {
    if (entry.timer !== null) {
      clearTimeout(entry.timer);
      entry.timer = null;
    }
    schedule(entry, 0);
  }
}
