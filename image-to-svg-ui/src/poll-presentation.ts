import type { Item, JobStatus } from "./types.ts";

export function svgHistoryPresentationSignature(items: Item[], selectedId: string) {
  return JSON.stringify({
    selectedId,
    items: items.map((item) => [
      item.id,
      item.historyId || "",
      item.outputPath || "",
      item.outputName || "",
      item.createdAtMs || 0,
    ]),
  });
}

export function svgStatusChangesPresentation(item: Item, status: JobStatus) {
  return item.stage !== status.stage
    || item.outputPath !== status.outputPath
    || item.outputName !== status.outputName
    || item.error !== status.error
    || item.progressKey !== status.progressKey
    || item.phase !== status.phase;
}
