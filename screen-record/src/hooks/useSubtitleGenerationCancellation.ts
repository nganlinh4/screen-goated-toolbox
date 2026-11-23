import { useCallback, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import { clearResumableRunIfJob, readResumableRun } from './resumableJobRegistry';
import { retireSubtitleGeneration } from './retiredSubtitleGeneration';
import type { ResumableSubtitleGenerationRun, SubtitleJobContext, SubtitleJobViewStatus } from './subtitleGenerationTypes';

interface CancellationOptions {
  activeJobIdRef: MutableRefObject<string | null>;
  cancelledJobIdRef: MutableRefObject<string | null>;
  startEpochRef: MutableRefObject<number>;
  startingEpochRef: MutableRefObject<number | null>;
  lastAppliedResultsKeyRef: MutableRefObject<string>;
  lastKnownResultsRevisionRef: MutableRefObject<number>;
  lastStatusViewKeyRef: MutableRefObject<string>;
  clearQueuedSubtitleResults: () => void;
  cancelPendingStart: () => boolean;
  setStatus: Dispatch<SetStateAction<SubtitleJobViewStatus | null>>;
  setJobId: Dispatch<SetStateAction<string | null>>;
  setJobContext: Dispatch<SetStateAction<SubtitleJobContext | null>>;
  setIsStartingSubtitleJob: Dispatch<SetStateAction<boolean>>;
  cancelledMessage: string;
  unconfirmedMessage: string;
}

export function useSubtitleGenerationCancellation(options: CancellationOptions) {
  const {
    activeJobIdRef, cancelledJobIdRef, startEpochRef, startingEpochRef,
    lastAppliedResultsKeyRef, lastKnownResultsRevisionRef, lastStatusViewKeyRef,
    clearQueuedSubtitleResults, cancelPendingStart, setStatus, setJobId, setJobContext,
    setIsStartingSubtitleJob, cancelledMessage, unconfirmedMessage,
  } = options;
  const handleCancelSubtitleGeneration = useCallback(async () => {
    const pendingStartCancelled = cancelPendingStart();
    if (startingEpochRef.current !== null || pendingStartCancelled) {
      startEpochRef.current += 1;
      startingEpochRef.current = null;
      setIsStartingSubtitleJob(false);
      clearQueuedSubtitleResults();
      setJobContext(null);
      setStatus({
        state: 'error', message: unconfirmedMessage,
        messageKey: 'subtitleStatusCancelUnconfirmed', progress: 0,
        activeClipId: null, totalClips: 0, completedClips: 0,
        resultsRevision: 0, skipped: [], error: unconfirmedMessage,
      });
    }
    const jobId = activeJobIdRef.current;
    if (!jobId || cancelledJobIdRef.current === jobId) return;
    // Retire pending and future results before awaiting backend cancellation.
    cancelledJobIdRef.current = jobId;
    clearQueuedSubtitleResults();
    const run = readResumableRun<ResumableSubtitleGenerationRun>('subtitle-generation');
    clearResumableRunIfJob('subtitle-generation', jobId);
    const finishCancellation = (confirmed: boolean) => {
      if (activeJobIdRef.current !== jobId || cancelledJobIdRef.current !== jobId) return;
      clearResumableRunIfJob('subtitle-generation', jobId);
      clearQueuedSubtitleResults();
      const message = confirmed ? cancelledMessage : unconfirmedMessage;
      setStatus((prev) => ({
        ...(prev ?? { progress: 0, totalClips: 0, completedClips: 0, resultsRevision: 0, skipped: [] }),
        state: confirmed ? 'cancelled' : 'error', message,
        messageKey: confirmed ? 'subtitleStatusCancelled' : 'subtitleStatusCancelUnconfirmed',
        messageParams: {}, activeClipId: null, error: confirmed ? null : message,
      }));
      lastAppliedResultsKeyRef.current = '';
      lastKnownResultsRevisionRef.current = 0;
      lastStatusViewKeyRef.current = '';
      activeJobIdRef.current = null;
      setJobId(null);
      setJobContext(null);
    };
    if (!run || run.jobId !== jobId) { finishCancellation(false); return; }
    // Release the editor immediately; the detached owner retries and keeps a
    // completed snapshot until the recovery journal acknowledges it.
    const retirement = retireSubtitleGeneration(run);
    finishCancellation(false);
    void retirement.then((outcome) => {
      if (outcome === 'cancelled' && activeJobIdRef.current === null && cancelledJobIdRef.current === jobId) {
        setStatus((prev) => prev?.messageKey === 'subtitleStatusCancelUnconfirmed'
          ? { ...prev, state: 'cancelled', message: cancelledMessage, messageKey: 'subtitleStatusCancelled', error: null }
          : prev);
      }
    });
  }, [activeJobIdRef, cancelPendingStart, cancelledJobIdRef, cancelledMessage, clearQueuedSubtitleResults,
    lastAppliedResultsKeyRef, lastKnownResultsRevisionRef, lastStatusViewKeyRef,
    setIsStartingSubtitleJob, setJobContext, setJobId, setStatus, startingEpochRef,
    startEpochRef, unconfirmedMessage]);

  const retireSubtitleForHistory = useCallback(() => {
    void handleCancelSubtitleGeneration();
  }, [handleCancelSubtitleGeneration]);
  return { handleCancelSubtitleGeneration, retireSubtitleForHistory };
}
