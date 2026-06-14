import {
  BackgroundConfig,
  MousePosition,
  VideoSegment,
  ZoomKeyframe,
  BakedOverlayPayload,
  BakedWebcamFrame,
  WebcamConfig,
} from '@/types/video';
import {
  KeystrokeOverlayEditBounds,
  getKeystrokeOverlayConfig,
  getKeystrokeOverlayEditBounds,
} from './keystrokeRenderer';
import {
  TextDragState,
  handleMouseDown as textMouseDown,
  handleMouseMove as textMouseMove,
  handleMouseUp as textMouseUp,
} from './overlayDragHandlers';
import {
  bakeOverlayAtlasAndPaths,
} from './overlayBaker';
import { calculateCurrentZoomStateInternal } from './cameraZoom';
import { createCursorImageSet, ensureCursorAnimations } from './cursorAssets';
import { drawFrame as drawFrameImpl, type RendererState } from './drawFrame';
import { buildBakedWebcamFrames, cloneWebcamConfig } from '@/lib/webcam';

// ---------------------------------------------------------------------------
// Public interfaces
// ---------------------------------------------------------------------------

export interface RenderContext {
  video: HTMLVideoElement;
  webcamVideo?: HTMLVideoElement | null;
  canvas: HTMLCanvasElement;
  tempCanvas: HTMLCanvasElement;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  currentTime: number;
  interactiveBackgroundPreview?: boolean;
}

export interface RenderOptions {
  exportMode?: boolean;
  highQuality?: boolean;
}

// Re-export needed types from submodules
export type { CursorRenderType } from './cursorGraphics';
export type { KeystrokeOverlayEditBounds } from './keystrokeRenderer';

// ---------------------------------------------------------------------------
// VideoRenderer class
// ---------------------------------------------------------------------------

class VideoRenderer {
  // --- Animation ---
  private animationFrame: number | null = null;
  private readonly FRAME_INTERVAL = 1000 / 120; // 120fps target

  // --- Renderer state (shared with drawFrame) ---
  private state: RendererState;

  // --- Zoom state ---
  private lastCalculatedState: ZoomKeyframe | null = null;

  // --- Text drag state ---
  private textDragState: TextDragState;

  // --- Active render context ---
  private activeRenderContext: RenderContext | null = null;

  constructor() {
    const cursorOffscreen = new OffscreenCanvas(128, 128);

    this.state = {
      cursorImages: createCursorImageSet(),
      cursorState: {
        cursorOffscreen,
        cursorOffscreenCtx: cursorOffscreen.getContext('2d')!,
        currentSquishScale: 1.0,
        loggedCursorTypes: new Set(),
        loggedCursorMappings: new Set(),
      },
      gradientCache: {
        renderedCanvasByKey: new Map(),
      },
      customBgCache: {
        customBackgroundPattern: null,
        lastCustomBackground: undefined,
        customBackgroundImage: null,
        customBackgroundCacheKey: undefined,
      },
      keystrokeState: {
        keystrokeLanguage: 'en',
        renderCache: {
          mode: 'off',
          segmentRef: null,
          eventsRef: null,
          visibilityRef: null,
          duration: 0,
          displayEvents: [],
          startTimes: [],
          effectiveEnds: [],
          keyboardStartTimes: [],
          keyboardIndices: [],
          mouseStartTimes: [],
          mouseIndices: [],
          keyboardMaxDuration: 0,
          mouseMaxDuration: 0,
          eventSlots: [],
          eventIdentities: [],
          keyboardSlotRepresentatives: [],
          mouseSlotRepresentatives: [],
        },
        layoutCache: new Map(),
      },
      currentSquishScale: 1.0,
      squishTarget: 1.0,
      squishAnimFrom: 1.0,
      squishAnimProgress: 1,
      squishAnimDuration: 0.15,
      squishHasRoom: true,
      lastHoldTime: -1,
      lastActiveEventId: null,
      blurAccumCanvas: null,
      blurAccumCtx: null,
      blurSubCanvas: null,
      blurSubCtx: null,
      webcamFrameCanvas: null,
      webcamFrameCtx: null,
      webcamFrameReady: false,
      isDrawing: false,
      lastDrawTime: 0,
      latestElapsed: 0,
      processedCursorPositions: null,
      lastMousePositionsRef: null,
      lastCursorProcessSignature: '',
      lastCursorNormalizationSignature: '',
      lastCursorPreviewDebugSignature: '',
      lastCursorPreviewDebugBucket: -1,
      lastCursorPreviewDebugPoint: null,
      calculateCurrentZoomState: (currentTime, segment, viewW, viewH, srcCropW?, srcCropH?, videoScale?) =>
        this.calculateCurrentZoomState(currentTime, segment, viewW, viewH, srcCropW, srcCropH, videoScale),
      requestRedraw: () => {
        if (this.activeRenderContext) this.drawFrame(this.activeRenderContext);
      },
    };

    this.textDragState = {
      isDraggingText: false,
      draggedTextId: null,
      draggedOverlayKind: null,
      dragStartPointer: { x: 0, y: 0 },
      dragTargets: [],
    };
  }

  // ---------------------------------------------------------------------------
  // Public API: accessors
  // ---------------------------------------------------------------------------

  public getLastCalculatedState() { return this.lastCalculatedState; }

  public updateRenderContext(context: RenderContext) {
    this.activeRenderContext = context;
    // Lazy-init animated cursor frames only when the project has wait/appstarting moments.
    const pack = context.backgroundConfig.cursorPack ?? 'screenstudio';
    ensureCursorAnimations(pack, context.mousePositions, this.state.cursorImages);
  }

  public generateBakedWebcamFrames(
    segment: VideoSegment,
    webcamConfig: WebcamConfig | null | undefined,
    outputWidth: number,
    outputHeight: number,
    webcamAspectRatio: number | null | undefined,
    fps: number = 60,
  ): BakedWebcamFrame[] {
    return buildBakedWebcamFrames(
      segment,
      cloneWebcamConfig(webcamConfig),
      outputWidth,
      outputHeight,
      webcamAspectRatio,
      (time) =>
        this.calculateCurrentZoomState(
          time,
          segment,
          outputWidth,
          outputHeight,
        ).zoomFactor,
      fps,
    );
  }

  public sampleZoomCurve(
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    numSamples: number = 200
  ): Array<{ time: number; zoom: number; posX: number; posY: number }> {
    const samples: Array<{ time: number; zoom: number; posX: number; posY: number }> = [];
    const start = segment.trimStart;
    const end = segment.trimEnd;
    for (let i = 0; i <= numSamples; i++) {
      const t = start + (end - start) * (i / numSamples);
      const state = calculateCurrentZoomStateInternal(t, segment, viewW, viewH);
      samples.push({
        time: t - start,
        zoom: state.zoomFactor,
        posX: state.positionX,
        posY: state.positionY
      });
    }
    return samples;
  }

  // ---------------------------------------------------------------------------
  // Animation control
  // ---------------------------------------------------------------------------

  public startAnimation(renderContext: RenderContext) {
    this.stopAnimation();
    // Reset squish state unconditionally — stopAnimation() only resets when animationFrame
    // was running. A fresh startAnimation must always begin with cursor at full scale,
    // regardless of whether the previous session was playing or paused.
    this.state.lastHoldTime = -1;
    this.state.currentSquishScale = 1.0;
    this.state.cursorState.currentSquishScale = 1.0;
    this.state.squishTarget = 1.0;
    this.state.squishAnimFrom = 1.0;
    this.state.squishAnimProgress = 1;
    this.state.squishAnimDuration = 0.15;
    this.state.squishHasRoom = true;
    this.state.lastActiveEventId = null;
    this.state.lastDrawTime = 0;
    this.activeRenderContext = renderContext;
    const pack = renderContext.backgroundConfig.cursorPack ?? 'screenstudio';
    ensureCursorAnimations(pack, renderContext.mousePositions, this.state.cursorImages);

    const animate = () => {
      if (!this.activeRenderContext || this.activeRenderContext.video.paused) {
        this.animationFrame = null;
        return;
      }

      const now = performance.now();
      const elapsed = now - this.state.lastDrawTime;

      if (this.state.lastDrawTime === 0 || elapsed >= this.FRAME_INTERVAL) {
        this.drawFrame(this.activeRenderContext)
          .catch((err: unknown) => console.error('[VideoRenderer] Draw error:', err));
      }

      this.animationFrame = requestAnimationFrame(animate);
    };

    this.animationFrame = requestAnimationFrame(animate);
  }

  public stopAnimation() {
    if (this.animationFrame !== null) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = null;
      this.state.lastDrawTime = 0;
      this.activeRenderContext = null;
      this.state.lastHoldTime = -1;
      this.state.currentSquishScale = 1.0;
      this.state.cursorState.currentSquishScale = 1.0;
      this.state.squishTarget = 1.0;
      this.state.squishAnimFrom = 1.0;
      this.state.squishAnimProgress = 1;
      this.state.squishAnimDuration = 0.15;
      this.state.squishHasRoom = true;
      this.state.lastActiveEventId = null;
    }
  }

  // ---------------------------------------------------------------------------
  // Caching wrappers (instance-level cache management)
  // ---------------------------------------------------------------------------

  private calculateCurrentZoomState(
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,
    srcCropH?: number,
    videoScale?: number
  ): ZoomKeyframe {
    const state = calculateCurrentZoomStateInternal(currentTime, segment, viewW, viewH, srcCropW, srcCropH, videoScale);
    this.lastCalculatedState = state;
    return state;
  }

  // ---------------------------------------------------------------------------
  // drawFrame - delegates to extracted drawFrame module
  // ---------------------------------------------------------------------------

  public drawFrame = async (
    context: RenderContext,
    options: RenderOptions = {}
  ): Promise<void> => {
    return drawFrameImpl(context, options, this.state);
  };

  // ---------------------------------------------------------------------------
  // Keystroke overlay public API (delegates to extracted keystrokeRenderer)
  // ---------------------------------------------------------------------------

  public getKeystrokeOverlayConfig(segment: VideoSegment): { x: number; y: number; scale: number } {
    return getKeystrokeOverlayConfig(segment);
  }

  public getKeystrokeOverlayEditBounds(
    segment: VideoSegment,
    canvas: HTMLCanvasElement,
    currentTime: number,
    duration: number
  ): KeystrokeOverlayEditBounds | null {
    return getKeystrokeOverlayEditBounds(this.state.keystrokeState, segment, canvas, currentTime, duration);
  }

  // ---------------------------------------------------------------------------
  // Text drag handlers (delegates to extracted overlayBaker)
  // ---------------------------------------------------------------------------

  public handleMouseDown(
    e: MouseEvent,
    segment: VideoSegment,
    canvas: HTMLCanvasElement,
    selection?: {
      selectedTextIds?: readonly string[];
      selectedSubtitleIds?: readonly string[];
    },
    currentTime?: number,
  ) {
    return textMouseDown(e, segment, canvas, this.textDragState, selection, currentTime);
  }

  public handleMouseMove(
    e: MouseEvent,
    _segment: VideoSegment,
    canvas: HTMLCanvasElement,
    onTextMove: (moves: Array<{ kind: 'text' | 'subtitle'; id: string; x: number; y: number }>) => void
  ) {
    textMouseMove(e, _segment, canvas, onTextMove, this.textDragState);
  }

  public handleMouseUp() {
    textMouseUp(this.textDragState);
  }

  // ---------------------------------------------------------------------------
  // Overlay baking (delegates to extracted overlayBaker)
  // ---------------------------------------------------------------------------

  public async bakeOverlayAtlasAndPaths(
    segment: VideoSegment,
    outputWidth: number,
    outputHeight: number,
    fps: number = 60
  ): Promise<BakedOverlayPayload> {
    return bakeOverlayAtlasAndPaths(segment, outputWidth, outputHeight, fps, this.state.keystrokeState);
  }
}

export const videoRenderer = new VideoRenderer();
