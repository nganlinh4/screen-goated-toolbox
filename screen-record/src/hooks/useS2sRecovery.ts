import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { SubtitleSegment } from '@/types/video';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { SubtitleSourceOwner } from './subtitleSourceOwnership';
import type { S2sNarrationStatus, S2sSubtitleStateSnapshot } from './s2sNarrationSubtitles';
import type { ActiveS2sRun, S2sNarrationRunState } from './s2sNarrationRunState';
import { hasResumableStart, useResumableRun } from './resumableJobRegistry';

interface Refs {
  shared: MutableRefObject<S2sNarrationRunState>; plan: MutableRefObject<SubtitleGenerationPlan | null>;
  owners: MutableRefObject<Record<string, SubtitleSourceOwner>>; batch: MutableRefObject<string>;
  language: MutableRefObject<string>; source: MutableRefObject<SubtitleSegment[]>;
  target: MutableRefObject<SubtitleSegment[]>; baseSource: MutableRefObject<SubtitleSegment[]>;
  baseTarget: MutableRefObject<SubtitleSegment[]>; snapshot: MutableRefObject<S2sSubtitleStateSnapshot | null>;
  revision: MutableRefObject<number>; signature: MutableRefObject<string>;
  progress: MutableRefObject<number>; stall: MutableRefObject<number>;
}

export function useS2sRecovery(
  namespace: string, jobId: string | null, isCurrentProject: () => boolean, refs: Refs,
  setStatus: Dispatch<SetStateAction<S2sNarrationStatus | null>>,
  setJobId: Dispatch<SetStateAction<string | null>>,
) {
  useResumableRun<ActiveS2sRun>(namespace, jobId, (cached) => {
    if (!isCurrentProject()) return;
    refs.shared.current = cached.shared; refs.plan.current = cached.plan;
    refs.owners.current = cached.sourceOwnersByClip ?? {}; refs.batch.current = cached.batchId;
    refs.language.current = cached.targetLanguage; refs.source.current = cached.sourceSubtitleResults;
    refs.target.current = cached.targetSubtitleResults; refs.baseSource.current = cached.baseSourceSubtitleResults;
    refs.baseTarget.current = cached.baseTargetSubtitleResults; refs.snapshot.current = cached.baseSubtitleSnapshot;
    refs.revision.current = cached.resultsRevision; refs.signature.current = cached.lastProgressSignature;
    refs.progress.current = cached.lastProgressAt; refs.stall.current = cached.lastStallLogAt;
    setStatus(cached.status); setJobId(cached.jobId);
  }, (retiredJobId) => {
    const retired = refs.shared.current;
    if (!isCurrentProject() || !retired.retired) return;
    if (retired.terminalStatus && !hasResumableStart(namespace)) setStatus(retired.terminalStatus);
    setJobId((current) => current === retiredJobId ? null : current);
  });
}
