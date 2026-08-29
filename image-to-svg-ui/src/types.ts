import type { CreationSourceProvenance } from "../../ui-shared/creation-source-provenance";

export type Model = "simple" | "detail";
export type BackgroundMode = "auto" | "transparent" | "opaque";
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
export type Asset = { dataUrl?: string; url?: string; text?: string; sizeBytes?: number };
export type SvgEditDelta =
  | {
      kind: "paint";
      shapePath: number[];
      property: "fill" | "stroke";
      before: string;
      after: string;
    }
  | {
      kind: "delete";
      parentPath: number[];
      childIndex: number;
      markup: string;
    };

export type HistoryEntry = {
  id: string;
  tool: "svg";
  sourcePath: string;
  outputPath: string;
  outputName: string;
  createdAtMs: number;
  metadata?: { model?: Model; backgroundMode?: BackgroundMode };
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
  outputDir: string;
  model: Model;
  backgroundMode: BackgroundMode;
  error?: string;
  progressKey?: string;
  phase?: string;
};

export type Item = {
  id: string;
  batchId: string;
  path: string;
  sourceProvenance: CreationSourceProvenance;
  name: string;
  model: Model;
  backgroundMode: BackgroundMode;
  outputDir: string;
  stage: Stage;
  submitted?: boolean;
  jobId?: string;
  progress?: number;
  progressText?: string;
  outputPath?: string;
  outputName?: string;
  error?: string;
  svgPreviewUrl?: string;
  svgText?: string;
  pathCount?: number;
  progressKey?: string;
  phase?: string;
  operationStartedAt?: number;
  estimatedTotalMs?: number;
  displayedProgress?: number;
  dirty?: boolean;
  saveError?: boolean;
  undoStack?: SvgEditDelta[];
  redoStack?: SvgEditDelta[];
  undoBytes?: number;
  redoBytes?: number;
  undoBaselineLost?: boolean;
  editLimitReached?: boolean;
  originalWidth?: string;
  originalHeight?: string;
  editingUnavailable?: boolean;
  historyId?: string;
  createdAtMs?: number;
  missingStatusPolls?: number;
};
