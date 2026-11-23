import type { Item } from "./types";
import { canSubmitCreationSource } from "../../ui-shared/creation-source-provenance.ts";
import { creationJobIsTerminal } from "../../ui-shared/creation-poll-state.ts";

export function canSubmitItem(item?: Pick<Item, "sourceProvenance">): boolean {
  return Boolean(item && canSubmitCreationSource(item.sourceProvenance, 1, false));
}

export function canActivatePrimaryAction(
  item?: Pick<Item, "sourceProvenance" | "stage">,
): boolean {
  return Boolean(item && (item.stage === "draft" || creationJobIsTerminal(item.stage)) && canSubmitItem(item));
}

export function needsFreshSubmissionSession(item: Item): boolean {
  return item.stage !== "draft";
}

export function freshSubmissionSession(source: Item, id: string): Item {
  return {
    id,
    batchId: id,
    path: source.path,
    sourceProvenance: source.sourceProvenance,
    name: source.name,
    model: source.model,
    backgroundMode: source.backgroundMode,
    outputDir: source.outputDir,
    stage: "queued",
    createdAtMs: Date.now(),
  };
}
