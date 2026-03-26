export interface ExportCapabilities {
  pipeline?: string;
  mfH264Available?: boolean;
  nvencAvailable: boolean;
  hevcNvencAvailable: boolean;
  sfeSupported: boolean;
  maxBFrames: number;
  driverVersion?: string;
  reasonIfDisabled?: string;
}

export interface ExportRuntimeDiagnostics {
  backend?: string;
  encoder?: string;
  codec?: string;
  turbo?: boolean;
  sfe?: boolean;
  preRenderPolicy?: string;
  qualityGatePercent?: number;
  actualTotalBitrateKbps?: number;
  expectedTotalBitrateKbps?: number;
  bitrateDeviationPercent?: number;
  readbackRingSize?: number;
  decodeQueueCapacity?: number;
  decodeRecycleCapacity?: number;
  writerQueueCapacity?: number;
  writerRecycleCapacity?: number;
  decodeWaitSecs?: number;
  composeRenderSecs?: number;
  readbackWaitSecs?: number;
  writerBlockSecs?: number;
  maxDecodeInflight?: number;
  maxWriterInflight?: number;
  maxPendingReadbacks?: number;
  fallbackUsed?: boolean;
  fallbackAttempts?: number;
  fallbackErrors?: string[];
}
