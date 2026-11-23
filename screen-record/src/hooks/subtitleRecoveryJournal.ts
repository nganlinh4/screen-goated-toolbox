import { useEffect, useRef, useSyncExternalStore } from 'react';
import { APP_META_STORE, openProjectDB } from '@/lib/projectStorage';
import type { SubtitleClipResult, SubtitleJobContext } from './subtitleGenerationTypes';
import { clearSubtitleStartTicket } from './subtitleStartTicket';

const KEY = 'subtitle-completed-recovery-v1';
const ACK_KEY = 'subtitle-recovery-acknowledged-v1';
const MAX_BYTES = 16 * 1024 * 1024;

export interface SubtitleRecoveryRecord {
  version: 1;
  jobId: string;
  projectId: string | null;
  context: SubtitleJobContext | null;
  results: SubtitleClipResult[];
}

interface RecoveryState {
  loaded: boolean;
  record: SubtitleRecoveryRecord | null;
  pending: SubtitleRecoveryRecord | null;
  error: string | null;
}

let state: RecoveryState = { loaded: false, record: null, pending: null, error: null };
// A delayed retirement reply must not recreate a recovery already acknowledged
// during this WebView's lifetime. Worker job IDs are unique.
const acknowledgedJobs = new Map<string, number>();
function acknowledgeJob(jobId: string) {
  acknowledgedJobs.set(jobId, (acknowledgedJobs.get(jobId) ?? 0) + 1);
  if (acknowledgedJobs.size > 1024) acknowledgedJobs.delete(acknowledgedJobs.keys().next().value!);
}
export function subtitleRecoveryAcknowledgement(jobId: string) { return acknowledgedJobs.get(jobId) ?? 0; }

/** Durable fence for a worker snapshot replayed after the recovery was cleared. */
export async function wasSubtitleRecoveryAcknowledged(jobId: string): Promise<boolean> {
  const db = await openProjectDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(APP_META_STORE, 'readonly');
    const request = tx.objectStore(APP_META_STORE).get(ACK_KEY);
    request.onsuccess = () => resolve(request.result === jobId);
    request.onerror = () => reject(request.error ?? new Error('Subtitle acknowledgement read failed'));
  });
}
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const getSnapshot = () => state;
function update(patch: Partial<RecoveryState>) {
  state = { ...state, ...patch };
  listeners.forEach((listener) => listener());
}

function decode(value: unknown): SubtitleRecoveryRecord | null {
  if (value == null) return null;
  if (typeof value !== 'string' || new TextEncoder().encode(value).length > MAX_BYTES) throw new Error('Invalid subtitle recovery record');
  const record: unknown = JSON.parse(value);
  if (!record || typeof record !== 'object') throw new Error('Invalid subtitle recovery record');
  const candidate = record as SubtitleRecoveryRecord;
  if (candidate.version !== 1 || typeof candidate.jobId !== 'string' || !candidate.jobId
    || (candidate.projectId !== null && (typeof candidate.projectId !== 'string' || !candidate.projectId))
    || (candidate.context !== null && (!candidate.context || candidate.context.projectKey !== candidate.projectId))
    || !Array.isArray(candidate.results) || candidate.results.some((result) => !result || typeof result.clipId !== 'string'
      || !Array.isArray(result.segments) || result.segments.some((segment) => !segment || typeof segment.text !== 'string'
        || !Number.isFinite(segment.startTime) || !Number.isFinite(segment.endTime)))) throw new Error('Invalid subtitle recovery record');
  return candidate;
}

let loading: Promise<void> | null = null;
export function loadSubtitleRecovery(force = false): Promise<void> {
  if (state.loaded && !force) return Promise.resolve();
  if (loading) return loading;
  loading = (async () => {
    try {
      const db = await openProjectDB();
      const value = await new Promise<unknown>((resolve, reject) => {
        const tx = db.transaction(APP_META_STORE, 'readonly');
        const request = tx.objectStore(APP_META_STORE).get(KEY);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      update({ loaded: true, record: decode(value), error: null });
    } catch (error) {
      update({ loaded: false, error: String(error) });
      throw error;
    } finally { loading = null; }
  })();
  return loading;
}

function writeRecord(record: SubtitleRecoveryRecord, signal: AbortSignal, admitted: () => boolean): Promise<void> {
  const serialized = JSON.stringify(record);
  if (new TextEncoder().encode(serialized).length > MAX_BYTES) return Promise.reject(new Error('Subtitle recovery exceeds the 16 MiB limit'));
  try { decode(serialized); } catch (error) { return Promise.reject(error); }
  if (signal.aborted) return Promise.reject(new Error('Subtitle recovery write cancelled'));
  return openProjectDB().then((db) => new Promise<void>((resolve, reject) => {
    if (signal.aborted) { reject(new Error('Subtitle recovery write cancelled')); return; }
    const tx = db.transaction(APP_META_STORE, 'readwrite');
    const abort = () => { try { tx.abort(); } catch { /* Already committed or aborted. */ } };
    signal.addEventListener('abort', abort, { once: true });
    const done = () => signal.removeEventListener('abort', abort);
    const store = tx.objectStore(APP_META_STORE);
    const request = store.get(KEY);
    let failure: Error | null = null;
    request.onsuccess = () => {
      try {
        if (!admitted()) throw new Error('Subtitle recovery was cleared');
        const existing = decode(request.result);
        if (existing && (existing.jobId !== record.jobId || existing.projectId !== record.projectId
          || JSON.stringify(existing) !== serialized)) throw new Error('Another subtitle recovery is awaiting attention');
        if (!existing) store.put(serialized, KEY);
      } catch (error) { failure = error instanceof Error ? error : new Error(String(error)); try { tx.abort(); } catch { /* Already aborted. */ } }
    };
    tx.oncomplete = () => { done(); resolve(); };
    tx.onerror = () => { done(); reject(failure ?? tx.error ?? new Error('Subtitle recovery write failed')); };
    tx.onabort = () => { done(); reject(failure ?? tx.error ?? new Error('Subtitle recovery write aborted')); };
    if (signal.aborted) abort();
  }));
}

let writing: { record: SubtitleRecoveryRecord; promise: Promise<boolean>; retry: () => void; cancel: () => Promise<boolean> } | null = null;
export function retainSubtitleRecovery(record: SubtitleRecoveryRecord, acknowledgement = subtitleRecoveryAcknowledgement(record.jobId)): Promise<boolean> {
  if (subtitleRecoveryAcknowledgement(record.jobId) !== acknowledgement) return Promise.resolve(false);
  if (writing?.record.jobId === record.jobId && JSON.stringify(writing.record) === JSON.stringify(record)) return writing.promise;
  if (writing) return writing.promise.then(() => retainSubtitleRecovery(record, acknowledgement));
  update({ pending: record });
  const admitted = () => subtitleRecoveryAcknowledgement(record.jobId) === acknowledgement;
  let retry = () => {};
  let cancel = async () => false;
  const promise = new Promise<boolean>((resolve) => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    let attempt = 0;
    let cancelled = false;
    let cancelling = false;
    let committedWhileCancelling = false;
    let activeAttempt: Promise<void> | null = null;
    const controller = new AbortController();
    const run = () => {
      if (activeAttempt || cancelled) return;
      activeAttempt = (async () => {
        try {
          await loadSubtitleRecovery();
          await writeRecord(record, controller.signal, admitted);
          if (cancelled) { committedWhileCancelling = true; return; }
          update({ loaded: true, record, pending: null, error: null });
          writing = null;
          resolve(true);
        } catch (error) {
          if (cancelled) return;
          if (!admitted()) {
            if (state.pending === record) update({ pending: null });
            if (writing?.record === record) writing = null;
            resolve(false);
            return;
          }
          update({ error: String(error) });
          attempt += 1;
          timer = setTimeout(() => { timer = null; run(); }, Math.min(30_000, 1000 * 2 ** Math.min(attempt - 1, 5)));
        } finally { activeAttempt = null; }
      })();
    };
    retry = () => { if (timer !== null) { clearTimeout(timer); timer = null; } run(); };
    cancel = async () => {
      if (cancelling) return false;
      cancelling = true;
      try {
        // Retire the durable replay identity before admitting the explicit
        // discard, including when IndexedDB itself is over quota.
        clearSubtitleStartTicket(record.jobId);
        cancelled = true;
        if (timer !== null) { clearTimeout(timer); timer = null; }
        controller.abort();
        await activeAttempt;
        const sameStoredRecord = state.record?.jobId === record.jobId && JSON.stringify(state.record) === JSON.stringify(record);
        if (committedWhileCancelling || sameStoredRecord) await clearMatchingSubtitleRecovery(record.jobId, JSON.stringify(record));
        acknowledgeJob(record.jobId);
        if (state.pending === record) update({ pending: null, error: null });
        if (writing?.record === record) writing = null;
        resolve(false);
        return true;
      } finally { cancelling = false; }
    };
    queueMicrotask(run);
  });
  writing = { record, promise, retry, cancel };
  return promise;
}

/** Cancel only the exact in-memory record the user confirmed. */
export async function discardPendingSubtitleRecovery(record: SubtitleRecoveryRecord): Promise<boolean> {
  const current = writing;
  if (!current || current.record !== record || state.pending !== record) return false;
  return current.cancel();
}

export function retrySubtitleRecovery() { writing?.retry(); if (!state.loaded) void loadSubtitleRecovery().catch(() => {}); }

/** Delete only the confirmed record; never delete a different job's recovery. */
async function clearMatchingSubtitleRecovery(jobId: string, expected?: string): Promise<boolean> {
  // Fence responses and queued writes as soon as the confirmed clear begins.
  if (expected === undefined || (state.record?.jobId === jobId && JSON.stringify(state.record) === expected)) {
    clearSubtitleStartTicket(jobId);
    acknowledgeJob(jobId);
  }
  const db = await openProjectDB();
  const removed = await new Promise<boolean>((resolve, reject) => {
    const tx = db.transaction(APP_META_STORE, 'readwrite');
    const store = tx.objectStore(APP_META_STORE);
    const request = store.get(KEY);
    let matched = false;
    request.onsuccess = () => {
      try {
        const existing = decode(request.result);
        if (existing?.jobId === jobId && (expected === undefined || JSON.stringify(existing) === expected)) {
          matched = true;
          store.put(jobId, ACK_KEY);
          store.delete(KEY);
        }
      } catch { tx.abort(); }
    };
    tx.oncomplete = () => resolve(matched);
    tx.onerror = () => reject(tx.error ?? new Error('Subtitle recovery removal failed'));
    tx.onabort = () => reject(tx.error ?? new Error('Subtitle recovery removal aborted'));
  });
  if (removed && state.record?.jobId === jobId && (expected === undefined || JSON.stringify(state.record) === expected)) update({ record: null, error: null });
  return removed;
}
export async function clearSubtitleRecovery(jobId: string, expected?: SubtitleRecoveryRecord): Promise<boolean> {
  return clearMatchingSubtitleRecovery(jobId, expected && JSON.stringify(expected));
}

export function getSubtitleRecovery() { return state; }
export async function requireSubtitleRecoveryClear(unavailable: string, pending: string) {
  try { await loadSubtitleRecovery(true); } catch { throw new Error(unavailable); }
  if (state.record || state.pending) throw new Error(pending);
}
export function useSubtitleCompletionBarrier() {
  const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  return async (record: SubtitleRecoveryRecord) => {
    const retained = await retainSubtitleRecovery(record);
    return !retained ? (mounted.current ? 'discarded' as const : 'discarded-unmounted' as const)
      : !mounted.current ? 'unmounted' as const : 'ready' as const;
  };
}
export function useSubtitleRecovery() {
  useEffect(() => { void loadSubtitleRecovery(true).catch(() => {}); }, []);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
