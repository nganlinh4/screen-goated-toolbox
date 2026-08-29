import type { HistoryEntry, JobStatus } from "./types";

export function applyHistoryRevision(entry: HistoryEntry, status: JobStatus) {
  status.projectId = entry.metadata?.projectId;
  status.parentRevisionId = entry.metadata?.parentRevisionId;
  status.revisionKind = entry.metadata?.revisionKind;
  status.supportedActions = entry.metadata?.supportedActions;
  status.availableActions = entry.metadata?.availableActions || [];
  status.isTextured = Boolean(entry.metadata?.isTextured);
  status.isPbr = Boolean(entry.metadata?.isPbr);
  status.isRigged = Boolean(entry.metadata?.isRigged);
  status.rigType = entry.metadata?.rigType;
}

export function historyRevision(entry: HistoryEntry): Partial<JobStatus> {
  const revision: JobStatus = { stage: "done", progressText: "" };
  applyHistoryRevision(entry, revision);
  return revision;
}
