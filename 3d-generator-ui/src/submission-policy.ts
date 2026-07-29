import type { QueueItem } from "./types";
import { canSubmitCreationSource } from "../../ui-shared/creation-source-provenance.ts";

export function canSubmitItem(
  item?: Pick<QueueItem, "sourceProvenance">,
): boolean {
  return Boolean(item && canSubmitCreationSource(item.sourceProvenance, 1, false));
}

export function needsFreshSubmissionSession(
  item: Pick<QueueItem, "state" | "submitted">,
): boolean {
  return item.state !== "queued" || item.submitted;
}

export function freshSubmissionSession(
  source: QueueItem,
  id: string,
  fallbackOutputDir: string,
): QueueItem {
  return {
    id,
    batchId: id,
    path: source.path,
    sourceProvenance: source.sourceProvenance,
    name: source.name,
    extension: source.extension,
    thumbnailUrl: source.thumbnailUrl,
    generationMode: source.generationMode,
    polycount: source.polycount,
    autoSegment: source.autoSegment,
    instruction: source.instruction,
    submitted: true,
    cancelRequested: false,
    state: "queued",
    outputDir: source.outputDir || fallbackOutputDir,
    createdAtMs: Date.now(),
  };
}
