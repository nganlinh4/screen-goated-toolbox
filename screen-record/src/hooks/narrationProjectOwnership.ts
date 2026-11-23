import { takeResumableRuns, takeResumableStarts } from './resumableJobRegistry';
import { retireNarrationJob } from './retiredNarrationCancellation';
import { retireNarrationBatch } from '@/lib/narrationBatchRetirements';
import { invalidateSubtitleSaveEditor } from './subtitleCompletedPersistence';

type NarrationKind = 'subtitle' | 's2s' | 'gemini-translate';
const PREFIX = 'narration-project:';
const ownerEpochs = new WeakMap<React.MutableRefObject<string | null>, number>();
const projectLoads = new WeakMap<React.MutableRefObject<string | null>, { token: object; previousId: string | null }>();
const CANCEL_COMMANDS: Record<NarrationKind, string> = {
  subtitle: 'cancel_subtitle_narration',
  s2s: 'cancel_s2s_narration',
  'gemini-translate': 'cancel_gemini_translate_narration',
};

export function narrationRunNamespace(projectId: string | null, kind: NarrationKind, epoch: number): string {
  return PREFIX + JSON.stringify([projectId, kind, epoch]);
}

export function narrationProjectEpoch(ref: React.MutableRefObject<string | null>): number {
  return ownerEpochs.get(ref) ?? 0;
}

export function isNarrationProjectLoading(ref: React.MutableRefObject<string | null>): boolean {
  return projectLoads.has(ref);
}

function changeNarrationProjectOwner(ref: React.MutableRefObject<string | null>, projectId: string | null): void {
  invalidateSubtitleSaveEditor(ref);
  ref.current = projectId;
  ownerEpochs.set(ref, narrationProjectEpoch(ref) + 1);
  cancelOtherNarrationRuns(projectId, narrationProjectEpoch(ref));
}

/** A project load creates a new owner, even if the same project is reloaded. */
export function activateNarrationProject(ref: React.MutableRefObject<string | null>, projectId: string): void {
  changeNarrationProjectOwner(ref, projectId);
}

/** Explicitly closing an empty editor also invalidates work started in it. */
export function closeNarrationProject(ref: React.MutableRefObject<string | null>): void {
  changeNarrationProjectOwner(ref, null);
}

/** Suspend the editor owner before reading a load snapshot. Superseded attempts cannot restore it. */
export function beginNarrationProjectLoad(ref: React.MutableRefObject<string | null>): (applied: boolean) => void {
  const previousId = projectLoads.get(ref)?.previousId ?? ref.current;
  const token = {};
  projectLoads.set(ref, { token, previousId });
  changeNarrationProjectOwner(ref, null);
  return (applied) => {
    if (projectLoads.get(ref)?.token !== token) return;
    projectLoads.delete(ref);
    if (!applied) changeNarrationProjectOwner(ref, previousId);
  };
}

/** Ordinary synchronization does not replace the current project owner. */
export function setNarrationProjectId(ref: React.MutableRefObject<string | null>, projectId: string | null): void {
  if (projectLoads.has(ref)) return;
  if (ref.current === projectId) return;
  changeNarrationProjectOwner(ref, projectId);
}

/** Retire backend runs for projects no longer open, even while the panel is closed. */
export function cancelOtherNarrationRuns(projectId: string | null, epoch: number): void {
  const isRetired = (namespace: string) => {
    if (!namespace.startsWith(PREFIX)) return false;
    try {
      const [owner, , runEpoch] = JSON.parse(namespace.slice(PREFIX.length)) as [string | null, NarrationKind, number];
      return owner !== projectId || runEpoch !== epoch;
    } catch {
      return false;
    }
  };
  for (const { command, jobId } of takeResumableStarts(isRetired)) retireNarrationJob(command, jobId);
  const retired = takeResumableRuns(isRetired);
  for (const [namespace, snapshot] of retired) {
    const [, kind] = JSON.parse(namespace.slice(PREFIX.length)) as [string | null, NarrationKind, number];
    const jobId = (snapshot as { jobId?: unknown } | null)?.jobId;
    const batchId = (snapshot as { batchId?: unknown } | null)?.batchId;
    if (typeof batchId === 'string') retireNarrationBatch(batchId);
    const savedCommand = (snapshot as { cancelCommand?: unknown } | null)?.cancelCommand;
    const command = typeof savedCommand === 'string' && Object.values(CANCEL_COMMANDS).includes(savedCommand)
      ? savedCommand : CANCEL_COMMANDS[kind];
    if (typeof jobId === 'string' && command) {
      retireNarrationJob(command, jobId);
    }
  }
}
