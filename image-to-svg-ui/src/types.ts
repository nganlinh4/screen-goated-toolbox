export type Model = "simple" | "detail";
export type Stage =
  | "draft"
  | "queued"
  | "preparing"
  | "generating"
  | "finalizing"
  | "done"
  | "failed"
  | "cancelled";

export type HostContext = { theme?: "light" | "dark"; language?: string };
export type Asset = { dataUrl?: string; text?: string; sizeBytes?: number };

export type HistoryEntry = {
  id: string;
  tool: "svg";
  sourcePath: string;
  outputPath: string;
  outputName: string;
  createdAtMs: number;
  metadata?: { model?: Model };
};

export type JobStatus = {
  jobId: string;
  stage: Stage;
  progressText: string;
  elapsedMs?: number;
  estimatedTotalMs?: number;
  progressRatio?: number;
  outputPath?: string;
  outputName?: string;
  sourceImagePath: string;
  model: Model;
  error?: string;
  progressKey?: string;
  phase?: string;
  previewPath?: string;
};

export type Item = {
  id: string;
  batchId: string;
  path: string;
  name: string;
  model: Model;
  outputDir: string;
  stage: Stage;
  jobId?: string;
  progress?: number;
  progressText?: string;
  outputPath?: string;
  outputName?: string;
  error?: string;
  thumbnailUrl?: string;
  sourceUrl?: string;
  svgText?: string;
  pathCount?: number;
  progressKey?: string;
  phase?: string;
  previewPath?: string;
  depthUrl?: string;
  operationStartedAt?: number;
  estimatedTotalMs?: number;
  displayedProgress?: number;
  dirty?: boolean;
  saveError?: boolean;
  undoStack?: string[];
  redoStack?: string[];
  savedSvgText?: string;
  originalWidth?: string;
  originalHeight?: string;
  historyId?: string;
  createdAtMs?: number;
};
