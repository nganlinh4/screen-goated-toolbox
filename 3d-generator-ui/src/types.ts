import type { GenerationMode } from "./generation-mode";
import type { ModelStats } from "./viewer";

export type Stage =
  | "idle"
  | "runtime_missing"
  | "preparing"
  | "visualizing"
  | "generating"
  | "segmenting"
  | "finalizing"
  | "done"
  | "failed"
  | "cancelled";
export type QueueState = "queued" | "running" | "done" | "failed" | "cancelled";

export type JobStatus = {
  jobId?: string | null;
  stage: Stage;
  progressText: string;
  phase?: string | null;
  workspaceState?: string | null;
  elapsedMs?: number | null;
  estimatedTotalMs?: number | null;
  progressRatio?: number | null;
  timingSampleCount?: number | null;
  outputPath?: string | null;
  outputName?: string | null;
  previewPath?: string | null;
  sourceImagePath?: string | null;
  generationMode?: GenerationMode;
  isSegmented?: boolean;
  canSegment?: boolean;
  error?: string | null;
  runtimeStatus?: string;
};

export type StartJobRequest = {
  imagePath: string;
  outputDir?: string | null;
  polycount: number;
  mode: "topology_mesh";
  generationMode: GenerationMode;
  outputFormat: "glb_plain";
  autoSegment: boolean;
  segmentationMode: "parts" | "none";
};

export type AssetPayload = { dataUrl: string; sizeBytes?: number };
export type HostContext = { theme?: "light" | "dark"; language?: string };
export type HistoryEntry = {
  id: string;
  tool: "3d";
  sourcePath: string;
  outputPath: string;
  outputName: string;
  createdAtMs: number;
  metadata?: { generationMode?: GenerationMode; isSegmented?: boolean };
};

export type QueueItem = {
  id: string;
  batchId: string;
  path: string;
  name: string;
  extension: string;
  thumbnailUrl?: string;
  generationMode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
  submitted: boolean;
  state: QueueState;
  result?: JobStatus;
  loadedDepthPath?: string;
  loadedModelPath?: string;
  modelAssetPath?: string;
  modelAssetPromise?: Promise<AssetPayload>;
  operationStartedAt?: number;
  estimatedTotalMs?: number;
  displayedProgress?: number;
  modelStats?: ModelStats;
  historyId?: string;
  createdAtMs?: number;
};

export type AppState = {
  items: QueueItem[];
  selectedId: string;
  runningIds: Set<string>;
  outputDir: string;
  queueActive: boolean;
  cancelRequested: boolean;
  backendStatus: JobStatus;
  preparationStatus: string;
  preparationTimer: number;
  preparationPollToken: number;
  displayToken: number;
  displayedItemId: string;
  displayedModelPath: string;
  displayRequestKey: string;
  displayPromise?: Promise<void>;
  outline: boolean;
  rotate: boolean;
  grid: boolean;
  wire: boolean;
  historyRefreshing: boolean;
  referencePreviewItemId: string;
  referencePreviewToken: number;
};

export type AppNodes = {
  dragRegion: HTMLElement;
  minimizeButton: HTMLButtonElement;
  closeButton: HTMLButtonElement;
  addImagesButton: HTMLButtonElement;
  queueList: HTMLElement;
  queueFooter: HTMLElement;
  chooseImageButton: HTMLButtonElement;
  chooseFolderButton: HTMLButtonElement;
  showFolderButton: HTMLButtonElement;
  sourceThumb: HTMLElement;
  sourceName: HTMLElement;
  sourceMeta: HTMLElement;
  folderName: HTMLElement;
  polycountRange: HTMLInputElement;
  polycountValue: HTMLOutputElement;
  modeButtons: HTMLButtonElement[];
  autoSegmentSection: HTMLElement;
  autoSegmentInput: HTMLInputElement;
  generateButton: HTMLButtonElement;
  generateLabel: HTMLElement;
  cancelButton: HTMLButtonElement;
  cancelLabel: HTMLElement;
  segmentButton: HTMLButtonElement;
  statusTitle: HTMLElement;
  statusDetail: HTMLElement;
  statusEta: HTMLElement;
  progressTrack: HTMLElement;
  progressFill: HTMLElement;
  statusMark: HTMLElement;
  stageStatus: HTMLElement;
  readiness: HTMLElement;
  readinessText: HTMLElement;
  emptyCopy: HTMLElement;
  modelStats: HTMLElement;
  resultSummary: HTMLElement;
  resultName: HTMLElement;
  resultMeta: HTMLElement;
  canvas: HTMLCanvasElement;
  stage: HTMLElement;
  viewerToolbar: HTMLElement;
  outlineButton: HTMLButtonElement;
  rotateButton: HTMLButtonElement;
  gridButton: HTMLButtonElement;
  wireButton: HTMLButtonElement;
  fitButton: HTMLButtonElement;
  shadingButtons: HTMLButtonElement[];
  referencePreview: HTMLElement;
  referencePreviewName: HTMLElement;
  referencePreviewImage: HTMLImageElement;
  referencePreviewClose: HTMLButtonElement;
  confirmDialog: HTMLElement;
  confirmMessage: HTMLElement;
  confirmCancel: HTMLButtonElement;
  confirmAccept: HTMLButtonElement;
  appToast: HTMLElement;
};
