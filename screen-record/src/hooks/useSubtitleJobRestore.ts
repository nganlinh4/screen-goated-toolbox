import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import { clearResumableRunIfJob, useResumableRun } from './resumableJobRegistry';
import type { ResumableSubtitleGenerationRun, SubtitleClipResult, SubtitleJobContext, SubtitleJobViewStatus } from './subtitleGenerationTypes';
import { hasPendingSubtitleCompletedSave } from './subtitleCompletedPersistence';
import { retireSubtitleGeneration } from './retiredSubtitleGeneration';

interface RestoreOptions {
  jobId: string | null;
  projectResetKey?: string | null;
  projectEpoch: number;
  lastKnownResultsRevisionRef: MutableRefObject<number>;
  lastAppliedResultsKeyRef: MutableRefObject<string>;
  lastStatusViewKeyRef: MutableRefObject<string>;
  staleResultsRef: MutableRefObject<boolean>;
  activeJobIdRef: MutableRefObject<string | null>;
  activeProjectKeyRef: MutableRefObject<string | null>;
  activeProjectEpochRef: MutableRefObject<number>;
  setJobContext: Dispatch<SetStateAction<SubtitleJobContext | null>>;
  setStatus: Dispatch<SetStateAction<SubtitleJobViewStatus | null>>;
  setJobId: Dispatch<SetStateAction<string | null>>;
  queueSubtitleResults: (results: SubtitleClipResult[], immediate: boolean) => void;
}

export function useSubtitleJobRestore({
  jobId, projectResetKey, projectEpoch, lastKnownResultsRevisionRef, lastAppliedResultsKeyRef,
  lastStatusViewKeyRef, staleResultsRef, activeJobIdRef, activeProjectKeyRef, activeProjectEpochRef,
  setJobContext, setStatus, setJobId, queueSubtitleResults,
}: RestoreOptions) {
  const namespace = 'subtitle-generation';
  useResumableRun<ResumableSubtitleGenerationRun>(namespace, jobId, (cached) => {
    if (hasPendingSubtitleCompletedSave()) {
      if (clearResumableRunIfJob(namespace, cached.jobId)) {
        retireSubtitleGeneration(cached);
      }
      return;
    }
    if (typeof cached.projectKey !== 'string' || cached.projectKey.length === 0) {
      if (clearResumableRunIfJob(namespace, cached.jobId)) {
        retireSubtitleGeneration(cached);
      }
      return;
    }
    // Wait for a temporarily unknown project before accepting or retiring its job.
    if (projectResetKey == null) return;
    if (cached.projectKey !== projectResetKey || cached.projectEpoch !== projectEpoch) {
      if (clearResumableRunIfJob(namespace, cached.jobId)) {
        retireSubtitleGeneration(cached);
      }
      return;
    }
    lastKnownResultsRevisionRef.current = cached.lastKnownResultsRevision;
    lastAppliedResultsKeyRef.current = cached.lastAppliedResultsKey;
    lastStatusViewKeyRef.current = cached.lastStatusViewKey;
    staleResultsRef.current = cached.staleResults ?? false;
    setJobContext(cached.jobContext);
    setStatus(cached.status);
    activeJobIdRef.current = cached.jobId;
    activeProjectKeyRef.current = cached.projectKey;
    activeProjectEpochRef.current = cached.projectEpoch;
    setJobId(cached.jobId);
    if (cached.pendingResults?.length) queueSubtitleResults(cached.pendingResults, false);
  }, undefined, `${projectResetKey}:${projectEpoch}`);
}
