import { useSyncExternalStore } from 'react';
import { hasPendingSubtitleCompletedSave } from './subtitleCompletedPersistence';
import { hasRetiringSubtitleGeneration } from './retiredSubtitleGeneration';
import { getSubtitleRecovery } from './subtitleRecoveryJournal';
import { readSubtitleStartTicket } from './subtitleStartTicket';
import { hasResumableStart, readResumableRun } from './resumableJobRegistry';

let blocked = false;
let overrideArmed = 0;
let nextOverrideId = 0;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export function useSubtitleShutdownBlocked() { return useSyncExternalStore(subscribe, () => blocked, () => false); }

/** A committed journal record is safe to reopen; unfinished work is not. */
export function hasPendingSubtitleShutdownWork(): boolean {
  if (getSubtitleRecovery().pending || hasPendingSubtitleCompletedSave() || hasRetiringSubtitleGeneration()
    || hasResumableStart('subtitle-generation') || readResumableRun('subtitle-generation')) return true;
  try { return readSubtitleStartTicket() !== null; }
  catch { return true; } // An unreadable ticket cannot prove that no worker owns a result.
}

export function showSubtitleShutdownBlocked() { if (!blocked) { blocked = true; notify(); } }
export function clearSubtitleShutdownBlocked() { if (blocked) { blocked = false; notify(); } }

/** Arm only after the user confirms that unfinished subtitle work can be lost. */
export function armSubtitleShutdownOverride(): boolean {
  if (!blocked || overrideArmed) return false;
  nextOverrideId = nextOverrideId >= 0x7fffffff ? 1 : nextOverrideId + 1;
  overrideArmed = nextOverrideId;
  return true;
}
export function subtitleShutdownOverrideArmed() { return overrideArmed !== 0; }
export function currentSubtitleShutdownOverride() { return overrideArmed; }
export function cancelSubtitleShutdownOverride(expected?: number) { if (expected === undefined || expected === overrideArmed) overrideArmed = 0; }
export function consumeSubtitleShutdownOverride(expected?: number): boolean {
  if (!expected || overrideArmed !== expected) return false;
  overrideArmed = 0;
  return true;
}
