import type { GenerationMode } from "./generation-mode";
import type { ModelStats } from "./viewer";
import type { CreationSourceProvenance } from "../../ui-shared/creation-source-provenance";

export type Stage =
  | "idle"
  | "runtime_missing"
  | "queued"
  | "preparing"
  | "generating"
  | "segmenting"
  | "refining"
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
  elapsedMs?: number | null;
  estimatedTotalMs?: number | null;
  progressRatio?: number | null;
  timingSampleCount?: number | null;
  outputPath?: string | null;
  outputName?: string | null;
  downloadPath?: string | null;
  downloadName?: string | null;
  sourceImagePath?: string | null;
  outputDir?: string | null;
  generationMode?: GenerationMode | null;
  polycount?: number | null;
  autoSegment?: boolean | null;
  instruction?: string | null;
  projectId?: string | null;
  parentRevisionId?: string | null;
  revisionKind?: string | null;
  supportedActions?: string[];
  availableActions?: string[];
  isTextured?: boolean;
  isPbr?: boolean;
  isRigged?: boolean;
  rigType?: string | null;
  canRefine?: boolean;
  isSegmented?: boolean;
  canSegment?: boolean;
  error?: string | null;
  runtimeStatus?: string;
};

export type RefineRequest = {
  continuationId: string;
  kind: "separate_parts" | "optimize_mesh" | "add_materials" | "generate_pbr" | "rig" | "animate";
  segmentationLevel?: "simple" | "balanced" | "detailed";
  topology?: "triangle" | "quad";
  faceLimit?: number;
  animation?: "idle" | "walk" | "run" | "jump" | "wave_goodbye_01";
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
  instruction?: string;
};

export type AssetPayload = { dataUrl: string; sizeBytes?: number };
export type ModelAssetPayload = { url: string };
export type ExportedResult = { directory: string; paths: string[]; names: string[] };
export type HostContext = { theme?: "light" | "dark"; language?: string };
export type HistoryEntry = {
  id: string;
  tool: "3d";
  sourcePath: string;
  outputPath: string;
  outputName: string;
  createdAtMs: number;
  metadata?: {
    generationMode?: GenerationMode;
    polycount?: number;
    autoSegment?: boolean;
    instruction?: string;
    outputDir?: string;
    isSegmented?: boolean;
    projectId?: string;
    parentRevisionId?: string;
    revisionKind?: string;
    supportedActions?: string[];
    availableActions?: string[];
    isTextured?: boolean;
    isPbr?: boolean;
    isRigged?: boolean;
    rigType?: string;
    projectThumbnail?: string;
    download?: {
      path: string;
      name: string;
    };
  };
};

export type QueueItem = {
  id: string;
  batchId: string;
  path: string;
  sourceProvenance: CreationSourceProvenance;
  name: string;
  extension: string;
  thumbnailUrl?: string;
  generationMode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
  instruction?: string;
  submitted: boolean;
  cancelRequested?: boolean;
  state: QueueState;
  result?: JobStatus;
  outputDir?: string;
  loadedModelPath?: string;
  modelAssetPath?: string;
  modelAssetPromise?: Promise<ModelAssetPayload>;
  operationStartedAt?: number;
  estimatedTotalMs?: number;
  displayedProgress?: number;
  progressRange?: {
    start: number;
    end: number;
  };
  progressContinues?: boolean;
  modelStats?: ModelStats;
  historyId?: string;
  createdAtMs?: number;
  exportedNames?: string[];
};

export type AppState = {
  items: QueueItem[];
  selectedId: string;
  runningIds: Set<string>;
  outputDir: string;
  queueActive: boolean;
  cancelRequested: boolean;
  selectedStatus: JobStatus;
  preparationStatus: string;
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
  generationCapabilities: {
    ready: boolean;
    optionalInstruction: Record<GenerationMode, boolean>;
  };
};

export type AppNodes = {
  dragRegion: HTMLElement;
  minimizeButton: HTMLButtonElement;
  closeButton: HTMLButtonElement;
  addImagesButton: HTMLButtonElement;
  queueList: HTMLElement;
  queueFooter: HTMLElement;
  chooseImageButton: HTMLButtonElement;
  controlRail: HTMLElement;
  sourceThumb: HTMLElement;
  sourceName: HTMLElement;
  sourceMeta: HTMLElement;
  polycountRange: HTMLInputElement;
  polycountValue: HTMLOutputElement;
  modeButtons: HTMLButtonElement[];
  autoSegmentSection: HTMLElement;
  autoSegmentInput: HTMLInputElement;
  instructionSection: HTMLElement;
  instructionInput: HTMLTextAreaElement;
  generateButton: HTMLButtonElement;
  generateLabel: HTMLElement;
  cancelButton: HTMLButtonElement;
  cancelLabel: HTMLElement;
  segmentButton: HTMLButtonElement;
  refinementPanel: HTMLElement;
  refinementButtons: HTMLButtonElement[];
  segmentationLevel: HTMLSelectElement;
  topologySelect: HTMLSelectElement;
  faceLimitInput: HTMLInputElement;
  animationSelect: HTMLSelectElement;
  downloadButton: HTMLButtonElement;
  downloadLabel: HTMLElement;
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
  appToast: HTMLElement;
};
