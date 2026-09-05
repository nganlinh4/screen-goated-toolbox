import type { QueueItem } from "./types";

/**
 * The output path whose bytes are final for this item: a finished result, a
 * result still being segmented in place, or the last artifact a failed or
 * cancelled attempt left behind.
 */
export function settledModelPath(item: QueueItem): string | undefined {
  const outputPath = item.result?.outputPath;
  if (!outputPath) return undefined;
  const settled =
    item.state === "done"
    || item.state === "failed"
    || item.state === "cancelled"
    || item.result?.stage === "done"
    || item.result?.stage === "segmenting";
  return settled ? outputPath : undefined;
}

/**
 * The model the viewer should show for this item right now. While a child
 * revision is still being created, the parent artifact it was started from
 * stays on screen instead of the empty placeholder.
 */
export function previewModelPath(item: QueueItem): string | undefined {
  return settledModelPath(item) ?? (item.state === "running" ? item.retainedModelPath : undefined);
}
