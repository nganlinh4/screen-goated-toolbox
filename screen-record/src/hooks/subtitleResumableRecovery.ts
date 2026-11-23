import { readResumableRun, saveResumableRun } from './resumableJobRegistry';
import { readResumableStartMetadata, takeResumableStarts } from './resumableJobRegistry';
import { invoke } from '@/lib/ipc';
import type { ResumableSubtitleGenerationRun } from './subtitleGenerationTypes';
import { useCallback, type MutableRefObject } from 'react';
import { readSubtitleStartTicket } from './subtitleStartTicket';
import { retireSubtitleGeneration } from './retiredSubtitleGeneration';

/** Update only the current job's recovery state; retired jobs cannot revive it. */
export function updateSubtitleRecovery(
  jobId: string | null,
  projectKey: string | null,
  patch: Partial<Pick<ResumableSubtitleGenerationRun, 'staleResults' | 'pendingResults'>>,
) {
  const run = readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation');
  if (!jobId || run?.jobId !== jobId || run.projectKey !== projectKey) return;
  saveResumableRun('subtitle-generation', { ...run, ...patch });
}

export function retireSubtitlePendingStart() {
  if (!readResumableStartMetadata('subtitle-generation')) return false;
  try {
    const ticket = readSubtitleStartTicket();
    if (ticket) void retireSubtitleGeneration(ticket);
  } catch { /* A corrupt ticket must remain visible to the next start. */ }
  for (const cancellation of takeResumableStarts((namespace) => namespace === 'subtitle-generation')) {
    void invoke(cancellation.command, { jobId: cancellation.jobId }).catch(() => {});
  }
  return true;
}

export function useSubtitleRecoveryHandlers(
  staleResultsRef: MutableRefObject<boolean>,
  activeJobIdRef: MutableRefObject<string | null>,
  activeProjectKeyRef: MutableRefObject<string | null>,
) {
  const onStaleResults = useCallback(() => {
    staleResultsRef.current = true;
    updateSubtitleRecovery(activeJobIdRef.current, activeProjectKeyRef.current, { staleResults: true });
  }, [activeJobIdRef, activeProjectKeyRef, staleResultsRef]);
  const onQueuedResultsFlushed = useCallback(() => updateSubtitleRecovery(
    activeJobIdRef.current, activeProjectKeyRef.current, { pendingResults: [] },
  ), [activeJobIdRef, activeProjectKeyRef]);
  return { onStaleResults, onQueuedResultsFlushed };
}
