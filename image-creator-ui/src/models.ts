export const MAX_REFERENCES = 20;

export interface JobStatus {
  jobId: string;
  operation: string;
  stage: string;
  progressText: string;
  elapsedMs?: number;
  estimatedTotalMs?: number;
  progressRatio?: number;
  outputPath?: string;
  outputName?: string;
  sourceImagePath?: string;
  sourceImagePaths?: string[];
  prompt: string;
  width?: number;
  height?: number;
  error?: string;
}

export interface HistoryEntry {
  id: string;
  sourcePath: string;
  outputPath: string;
  outputName: string;
  createdAtMs: number;
  metadata: {
    prompt?: string;
    width?: number;
    height?: number;
    sourceImagePaths?: string[];
  };
}

export interface DraftSession {
  key: string;
  referencePaths: string[];
  prompt: string;
}

export interface Selection {
  key: string;
  kind: "draft" | "job" | "history";
  referencePaths: string[];
  output?: string;
  title: string;
  prompt: string;
  width?: number;
  height?: number;
}

export interface DialogState {
  kind: "rename" | "delete";
  entry: HistoryEntry;
  value: string;
}

export function pathName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#039;",
  })[character] || character);
}

export function jobReferences(job: JobStatus): string[] {
  if (Array.isArray(job.sourceImagePaths)) {
    return uniquePaths(job.sourceImagePaths).slice(0, MAX_REFERENCES);
  }
  return job.sourceImagePath ? [job.sourceImagePath] : [];
}

export function historyReferences(entry: HistoryEntry): string[] {
  if (Array.isArray(entry.metadata?.sourceImagePaths)) {
    return uniquePaths(entry.metadata.sourceImagePaths).slice(0, MAX_REFERENCES);
  }
  return entry.sourcePath ? [entry.sourcePath] : [];
}

export function uniquePaths(paths: string[]): string[] {
  const seen = new Set<string>();
  return paths.filter((path) => {
    const trimmed = path.trim();
    const key = trimmed.toLocaleLowerCase();
    if (!trimmed || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
