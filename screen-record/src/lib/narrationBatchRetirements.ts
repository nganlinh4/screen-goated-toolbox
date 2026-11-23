import type { NarrationSegment } from '@/types/video';

interface ActiveBatch {
  projectId: string;
  deleted: Set<string>;
  retired: boolean;
  cancel: () => void;
}

const MAX_DELETED_TAKES = 8192;
const active = new Map<string, ActiveBatch>();
const takeKey = (take: NarrationSegment) => take.narrationGroupTakeId
  ? JSON.stringify(['group', take.narrationGroupTakeId])
  : JSON.stringify(['take', take.id]);

/** Keep the numeric suffix used by existing batch-family reconciliation. */
export function newNarrationBatchId(family: string): string {
  return `${family}-${BigInt(`0x${crypto.randomUUID().replace(/-/g, '')}`).toString()}`;
}

export function registerNarrationBatch(projectId: string, batchId: string, cancel: () => void) {
  active.set(batchId, { projectId, deleted: new Set(), retired: false, cancel });
}

export function retireNarrationBatch(batchId: string | null | undefined) {
  if (batchId) active.delete(batchId);
}

/** Preserve deletions until the exact generating batch is retired. */
export function markNarrationTakesDeleted(projectId: string, takes: readonly NarrationSegment[]) {
  for (const take of takes) {
    const batch = take.narrationBatchId ? active.get(take.narrationBatchId) : undefined;
    if (!batch || batch.projectId !== projectId || batch.retired) continue;
    batch.deleted.add(takeKey(take));
    if (batch.deleted.size > MAX_DELETED_TAKES) {
      batch.retired = true;
      batch.deleted.clear();
      batch.cancel();
    }
  }
}

export function excludeDeletedNarrationTakes(projectId: string, takes: readonly NarrationSegment[]) {
  return takes.filter((take) => {
    const batch = take.narrationBatchId ? active.get(take.narrationBatchId) : undefined;
    return !batch || batch.projectId !== projectId || (!batch.retired && !batch.deleted.has(takeKey(take)));
  });
}
