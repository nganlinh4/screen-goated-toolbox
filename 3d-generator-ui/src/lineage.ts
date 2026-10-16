/**
 * Revision lineage for history items. Pure and dependency-free so it stays
 * testable headlessly; the queue view owns the presentation of the result.
 */

export type LineageItem = {
  id: string;
  historyId?: string;
  result?: {
    projectId?: string | null;
    parentRevisionId?: string | null;
    revisionKind?: string | null;
  };
};

export type LineageInfo = {
  version: number;
  kind: string;
  current: boolean;
};

export function lineageChains(items: readonly LineageItem[]): Map<string, LineageInfo> {
  const lineage = new Map<string, LineageInfo>();
  const historyItems = items.filter((item) => item.historyId && item.result?.projectId);
  const byHistoryId = new Map(historyItems.map((item) => [item.historyId as string, item]));
  const referencedParents = new Set<string>();
  for (const item of historyItems) {
    const parent = item.result?.parentRevisionId;
    if (parent && byHistoryId.has(parent)) referencedParents.add(parent);
  }
  for (const item of historyItems) {
    const historyId = item.historyId as string;
    const projectId = item.result?.projectId;
    const visited = new Set<string>([historyId]);
    let version = 1;
    let node = item;
    for (;;) {
      const parentId = node.result?.parentRevisionId;
      if (!parentId) break;
      const parent = byHistoryId.get(parentId);
      if (!parent || parent.result?.projectId !== projectId || visited.has(parentId)) break;
      visited.add(parentId);
      node = parent;
      version += 1;
    }
    lineage.set(item.id, {
      version,
      kind: item.result?.revisionKind || "",
      current: !referencedParents.has(historyId),
    });
  }
  return lineage;
}
