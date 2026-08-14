import type { MousePosition, VideoSegment, ZoomKeyframe } from "@/types/video";
import type { CursorImageSet, CursorRenderState } from "./cursorGraphics";
import type { CustomBgCache, GradientCache } from "./gradientGenerator";
import type { KeystrokeState } from "./keystrokeRenderer";

export interface RendererState {
  cursorImages: CursorImageSet;
  cursorState: CursorRenderState;
  gradientCache: GradientCache;
  customBgCache: CustomBgCache;
  keystrokeState: KeystrokeState;
  currentSquishScale: number;
  squishTarget: number;
  squishAnimFrom: number;
  squishAnimProgress: number;
  squishAnimDuration: number;
  squishHasRoom: boolean;
  lastHoldTime: number;
  lastActiveEventId: string | null;
  blurAccumCanvas: OffscreenCanvas | null;
  blurAccumCtx: OffscreenCanvasRenderingContext2D | null;
  blurSubCanvas: OffscreenCanvas | null;
  blurSubCtx: OffscreenCanvasRenderingContext2D | null;
  webcamFrameCanvas: OffscreenCanvas | null;
  webcamFrameCtx: OffscreenCanvasRenderingContext2D | null;
  webcamFrameReady: boolean;
  webcamFrameSource: string | null;
  isDrawing: boolean;
  lastDrawTime: number;
  latestElapsed: number;
  processedCursorPositions: MousePosition[] | null;
  lastMousePositionsRef: MousePosition[] | null;
  lastCursorProcessSignature: string;
  lastCursorNormalizationSignature: string;
  lastCursorPreviewDebugSignature: string;
  lastCursorPreviewDebugBucket: number;
  lastCursorPreviewDebugPoint: { x: number; y: number } | null;
  calculateCurrentZoomState: (
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,
    srcCropH?: number,
    videoScale?: number,
  ) => ZoomKeyframe;
  requestRedraw: () => void;
}
