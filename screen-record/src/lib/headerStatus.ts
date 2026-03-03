// Global header status badge system.
//
// Any module (React or plain TS) can push/clear status messages.
// The Header component subscribes via useHeaderStatus() and renders
// the highest-priority active message as a small animated pill.
//
// Messages use i18n translation keys (e.g. 'statusPreparingCursors').
// The Header resolves them via the current `t` object so the badge
// switches language live with the rest of the UI.

import { useSyncExternalStore } from 'react';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type StatusType = 'info' | 'success';

export interface StatusEntry {
  id: string;
  /** Translation key into the Translations object (e.g. 'statusPreparingCursors'). */
  messageKey: string;
  type: StatusType;
  /** Auto-dismiss after this many ms (0 = manual clear only). */
  autoDismissMs: number;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

const _entries = new Map<string, StatusEntry>();
const _listeners = new Set<() => void>();

function notify() {
  for (const fn of _listeners) fn();
}

/**
 * Push or update a status message.
 * `messageKey` should be a key in the Translations object (e.g. 'statusPreparingCursors').
 * If a message with the same `id` exists, it is replaced.
 */
export function pushHeaderStatus(
  id: string,
  messageKey: string,
  type: StatusType = 'info',
  autoDismissMs = 0,
): void {
  _entries.set(id, { id, messageKey, type, autoDismissMs });
  notify();

  if (autoDismissMs > 0) {
    setTimeout(() => {
      // Only clear if the entry hasn't been replaced since.
      const current = _entries.get(id);
      if (current && current.messageKey === messageKey) {
        _entries.delete(id);
        notify();
      }
    }, autoDismissMs);
  }
}

/** Remove a status message by id. */
export function clearHeaderStatus(id: string): void {
  if (_entries.delete(id)) notify();
}

// ---------------------------------------------------------------------------
// React hook — subscribe to the current top status entry
// ---------------------------------------------------------------------------

function getSnapshot(): StatusEntry | null {
  if (_entries.size === 0) return null;
  // Return the most recently added entry (last in insertion order).
  let last: StatusEntry | null = null;
  for (const e of _entries.values()) last = e;
  return last;
}

function subscribe(onStoreChange: () => void): () => void {
  _listeners.add(onStoreChange);
  return () => { _listeners.delete(onStoreChange); };
}

/** Returns the current highest-priority status entry, or null. */
export function useHeaderStatus(): StatusEntry | null {
  return useSyncExternalStore(subscribe, getSnapshot);
}
