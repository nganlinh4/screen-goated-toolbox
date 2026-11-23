import { useSyncExternalStore } from 'react';
import { invoke } from '@/lib/ipc';
import { retainSubtitleRecovery, subtitleRecoveryAcknowledgement, wasSubtitleRecoveryAcknowledged } from './subtitleRecoveryJournal';
import type { ResumableSubtitleGenerationRun, SubtitleJobStatus } from './subtitleGenerationTypes';
import { clearSubtitleStartTicket } from './subtitleStartTicket';

export interface SubtitleRetirement { jobId: string; projectId: string | null }
type Outcome = 'cancelled' | 'recovered' | 'released';
interface Entry { item: SubtitleRetirement; promise: Promise<Outcome>; release: () => void }
const retiring = new Map<string, Entry>();
let snapshot: SubtitleRetirement[] = [];
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const getSnapshot = () => snapshot;
export const getRetiringSubtitleGenerations = getSnapshot;
function changed() { snapshot = [...retiring.values()].map((entry) => entry.item); listeners.forEach((listener) => listener()); }
export function hasRetiringSubtitleGeneration() { return retiring.size > 0; }
export function useRetiringSubtitleGenerations() { return useSyncExternalStore(subscribe, getSnapshot, getSnapshot); }
export function releaseRetiringSubtitleGeneration(item: SubtitleRetirement) {
  const entry = retiring.get(item.jobId);
  if (!entry || entry.item !== item) return false;
  try { entry.release(); return true; } catch { return false; }
}

/** Retire an editor's job without discarding a result completed before cancellation. */
export function retireSubtitleGeneration(run: ResumableSubtitleGenerationRun): Promise<Outcome> {
  const existing = retiring.get(run.jobId);
  if (existing) return existing.promise;
  const item = { jobId: run.jobId, projectId: run.projectKey };
  const acknowledgement = subtitleRecoveryAcknowledgement(run.jobId);
  let release = () => {};
  let start = () => {};
  const promise = new Promise<Outcome>((resolve) => {
    let finished = false;
    let timer: number | null = null;
    let attempts = 0;
    const finish = (outcome: Outcome) => {
      if (finished) return;
      clearSubtitleStartTicket(run.jobId);
      finished = true;
      if (timer !== null) window.clearTimeout(timer);
      if (retiring.get(run.jobId)?.item === item) { retiring.delete(run.jobId); changed(); }
      resolve(outcome);
    };
    release = () => finish('released');
    const attempt = async () => {
      try {
        const status = await invoke<SubtitleJobStatus | null>('cancel_subtitle_generation', { jobId: run.jobId });
        if (finished) return;
        if (status?.state === 'completed' && status.results.length
          && subtitleRecoveryAcknowledgement(run.jobId) === acknowledgement) {
          if (await wasSubtitleRecoveryAcknowledged(run.jobId)) { finish('cancelled'); return; }
          if (finished) return;
          const retained = await retainSubtitleRecovery({ version: 1, jobId: run.jobId, projectId: run.projectKey,
            context: run.jobContext, results: status.results }, acknowledgement);
          if (finished) return;
          finish(retained ? 'recovered' : 'cancelled');
        } else finish('cancelled');
      } catch {
        if (!finished) timer = window.setTimeout(() => { timer = null; void attempt(); }, Math.min(30_000, 250 * 2 ** Math.min(attempts++, 7)));
      }
    };
    start = () => { void attempt(); };
  });
  retiring.set(run.jobId, { item, promise, release });
  changed();
  start();
  return promise;
}
