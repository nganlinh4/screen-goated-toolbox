import { BackgroundConfig, MousePosition, VideoSegment, ZoomKeyframe, TextSegment, BakedCameraFrame, BakedCursorFrame, BakedTextOverlay } from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';

// --- CONFIGURATION ---
// Increased offset slightly so the eye leads the cursor (more natural reading)
const CURSOR_OFFSET_SEC = 0.15;

export interface RenderContext {
  video: HTMLVideoElement;
  canvas: HTMLCanvasElement;
  tempCanvas: HTMLCanvasElement;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  currentTime: number;
}

export interface RenderOptions {
  exportMode?: boolean;
  highQuality?: boolean;
}

export class VideoRenderer {
  private animationFrame: number | null = null;
  private isDrawing: boolean = false;
  private lastDrawTime: number = 0;
  private latestElapsed: number = 0;
  private readonly FRAME_INTERVAL = 1000 / 120; // 120fps target
  private backgroundConfig: BackgroundConfig | null = null;
  private pointerImage: HTMLImageElement;
  private customBackgroundPattern: CanvasPattern | null = null;
  private lastCustomBackground: string | undefined = undefined;

  private readonly DEFAULT_STATE: ZoomKeyframe = {
    time: 0,
    duration: 0,
    zoomFactor: 1,
    positionX: 0.5,
    positionY: 0.5,
    easingType: 'linear' as const
  };

  private lastCalculatedState: ZoomKeyframe | null = null;
  public getLastCalculatedState() { return this.lastCalculatedState; }

  private smoothedPositions: MousePosition[] | null = null;
  private lastMousePositionsRef: MousePosition[] | null = null;
  private cachedBakedPath: BakedCameraFrame[] | null = null;
  private lastBakeSignature: string = '';
  private lastBakeSegment: VideoSegment | null = null;
  private lastBakeViewW: number = 0;
  private lastBakeViewH: number = 0;

  /**
   * Apply font-variation-settings as CSS on the canvas element.
   * Canvas 2D has no native API for font-variation-settings — the only
   * working workaround is setting it on the element's CSS style so the
   * context inherits it during font resolution for fillText/measureText.
   */
  private applyFontVariations(ctx: CanvasRenderingContext2D, vars: TextSegment['style']['fontVariations']) {
    const parts: string[] = [];
    const wdth = vars?.wdth ?? 100;
    const slnt = vars?.slnt ?? 0;
    const rond = vars?.ROND ?? 0;
    if (wdth !== 100) parts.push(`'wdth' ${wdth}`);
    if (slnt !== 0) parts.push(`'slnt' ${slnt}`);
    if (rond !== 0) parts.push(`'ROND' ${rond}`);
    ctx.canvas.style.fontVariationSettings = parts.length > 0 ? parts.join(', ') : 'normal';
  }

  private isDraggingText = false;
  private draggedTextId: string | null = null;
  private dragOffset = { x: 0, y: 0 };

  private currentSquishScale = 1.0;
  private lastHoldTime = -1;
  private readonly CLICK_FUSE_THRESHOLD = 0.15;
  private readonly SQUISH_SPEED = 0.015;
  private readonly RELEASE_SPEED = 0.01;
  private cursorOffscreen: OffscreenCanvas;
  private cursorOffscreenCtx: OffscreenCanvasRenderingContext2D;

  constructor() {
    this.pointerImage = new Image();
    this.pointerImage.src = '/pointer.svg';
    this.pointerImage.onload = () => { };
    this.cursorOffscreen = new OffscreenCanvas(128, 128);
    this.cursorOffscreenCtx = this.cursorOffscreen.getContext('2d')!;
  }

  private activeRenderContext: RenderContext | null = null;

  public updateRenderContext(context: RenderContext) {
    this.activeRenderContext = context;
  }

  // --- Easing Functions ---

  // Perlin's smootherStep: zero velocity AND zero acceleration at both endpoints.
  // The speed curve (derivative) is 30t²(1-t)² — touches zero as a smooth parabola,
  // not a sharp V. This eliminates the visible "corner" at keyframe boundaries.
  private easeCameraMove(t: number): number {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    return t * t * t * (t * (t * 6 - 15) + 10);
  }

  // --- Viewport-center-space blending for drift-free camera motion ---
  // posX/Y are zoom anchor params whose visual effect depends on zoom level.
  // Blending them directly causes sliding. Instead, blend the actual visible
  // center on screen, then convert back to anchor params.

  private toViewportCenter(zoom: number, posX: number, posY: number) {
    if (zoom <= 1.0) return { cx: 0.5, cy: 0.5 };
    return {
      cx: posX + (0.5 - posX) / zoom,
      cy: posY + (0.5 - posY) / zoom
    };
  }

  private fromViewportCenter(zoom: number, cx: number, cy: number) {
    if (zoom <= 1.001) return { posX: cx, posY: cy };
    const s = 1 - 1 / zoom;
    return {
      posX: (cx - 0.5 / zoom) / s,
      posY: (cy - 0.5 / zoom) / s
    };
  }

  // Blend two zoom states with log-space zoom + viewport-center-space position
  private blendZoomStates(
    stateA: ZoomKeyframe,
    stateB: ZoomKeyframe,
    t: number // 0 = stateA, 1 = stateB
  ): { zoom: number; posX: number; posY: number } {
    const zA = Math.max(0.1, stateA.zoomFactor);
    const zB = Math.max(0.1, stateB.zoomFactor);
    // Log-space zoom for perceptually uniform scaling
    const zoom = zA * Math.pow(zB / zA, t);
    // Viewport-center-space position for drift-free motion
    const cA = this.toViewportCenter(zA, stateA.positionX, stateA.positionY);
    const cB = this.toViewportCenter(zB, stateB.positionX, stateB.positionY);
    const cx = cA.cx + (cB.cx - cA.cx) * t;
    const cy = cA.cy + (cB.cy - cA.cy) * t;
    const { posX, posY } = this.fromViewportCenter(zoom, cx, cy);
    return { zoom, posX, posY };
  }

  // --- BAKED CURSOR PATH GENERATION ---
  public generateBakedCursorPath(
    segment: VideoSegment,
    mousePositions: MousePosition[],
    fps: number = 60
  ): BakedCursorFrame[] {
    const baked: BakedCursorFrame[] = [];
    const step = 1 / fps;
    const start = segment.trimStart;
    const end = segment.trimEnd;

    const smoothed = this.smoothMousePositions(mousePositions);

    let simSquishScale = 1.0;
    let simLastHoldTime = -1;
    const simRatio = 2.0;

    for (let t = start; t <= end; t += step) {
      const cursorT = t + CURSOR_OFFSET_SEC;
      const pos = this.interpolateCursorPositionInternal(cursorT, smoothed);

      if (!pos) {
        if (baked.length > 0) {
          const last = baked[baked.length - 1];
          baked.push({ ...last, time: t - start });
        } else {
          baked.push({ time: t - start, x: 0, y: 0, scale: 1, isClicked: false, type: 'default', opacity: 1 });
        }
        continue;
      }

      const isClicked = pos.isClicked;
      const timeSinceLastHold = cursorT - simLastHoldTime;
      const shouldBeSquished = isClicked || (simLastHoldTime >= 0 && timeSinceLastHold < this.CLICK_FUSE_THRESHOLD && timeSinceLastHold > 0);

      if (isClicked) {
        simLastHoldTime = cursorT;
      }

      const targetScale = shouldBeSquished ? 0.75 : 1.0;

      if (simSquishScale > targetScale) {
        simSquishScale = Math.max(targetScale, simSquishScale - this.SQUISH_SPEED * simRatio);
      } else if (simSquishScale < targetScale) {
        simSquishScale = Math.min(targetScale, simSquishScale + this.RELEASE_SPEED * simRatio);
      }

      const cursorVis = getCursorVisibility(t, segment.cursorVisibilitySegments);

      baked.push({
        time: t - start,
        x: pos.x,
        y: pos.y,
        scale: Number((simSquishScale * cursorVis.scale).toFixed(3)),
        isClicked: isClicked,
        type: pos.cursor_type || 'default',
        opacity: Number(cursorVis.opacity.toFixed(3)),
      });
    }

    return baked;
  }

  // --- BAKED CAMERA PATH GENERATION ---
  public generateBakedPath(
    segment: VideoSegment,
    videoWidth: number,
    videoHeight: number,
    fps: number = 60
  ): BakedCameraFrame[] {
    const bakedPath: BakedCameraFrame[] = [];
    const step = 1 / fps;
    const start = segment.trimStart;
    const end = segment.trimEnd;

    const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const croppedW = videoWidth * crop.width;
    const croppedH = videoHeight * crop.height;
    const cropOffsetX = videoWidth * crop.x;
    const cropOffsetY = videoHeight * crop.y;

    for (let t = start; t <= end; t += step) {
      // Pass CROPPED dimensions — calculateCurrentZoomStateInternal's crop
      // conversion assumes viewW/viewH are crop-region pixel dimensions
      const state = this.calculateCurrentZoomStateInternal(t, segment, croppedW, croppedH);

      const globalX = cropOffsetX + (state.positionX * croppedW);
      const globalY = cropOffsetY + (state.positionY * croppedH);

      bakedPath.push({
        time: t - start,
        x: globalX,
        y: globalY,
        zoom: state.zoomFactor
      });
    }

    return bakedPath;
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
      const state = this.calculateCurrentZoomStateInternal(t, segment, viewW, viewH);
      samples.push({
        time: t - start,
        zoom: state.zoomFactor,
        posX: state.positionX,
        posY: state.positionY
      });
    }
    return samples;
  }

  public startAnimation(renderContext: RenderContext) {
    this.stopAnimation();
    this.lastDrawTime = 0;
    // Don't reset cursor smoothing cache — it's invalidated by reference check
    // in interpolateCursorPosition when mouse data actually changes
    this.activeRenderContext = renderContext;

    const animate = () => {
      if (!this.activeRenderContext || this.activeRenderContext.video.paused) {
        this.animationFrame = null;
        return;
      }

      const now = performance.now();
      const elapsed = now - this.lastDrawTime;

      if (this.lastDrawTime === 0 || elapsed >= this.FRAME_INTERVAL) {
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
      this.lastDrawTime = 0;
      this.activeRenderContext = null;
      this.lastHoldTime = -1;
      this.currentSquishScale = 1.0;
    }
  }

  public drawFrame = async (
    context: RenderContext,
    options: RenderOptions = {}
  ): Promise<void> => {
    if (this.isDrawing) return;

    const { video, canvas, tempCanvas, segment, backgroundConfig, mousePositions } = context;
    if (!video || !canvas || !segment) return;
    if (video.readyState < 2) return;

    const isExportMode = options.exportMode || false;
    const quality = options.highQuality || isExportMode ? 'high' : 'medium';

    const ctx = canvas.getContext('2d', {
      alpha: false,
      willReadFrequently: false
    });
    if (!ctx) return;

    this.isDrawing = true;
    ctx.imageSmoothingQuality = quality as ImageSmoothingQuality;

    const now = performance.now();
    this.latestElapsed = this.lastDrawTime === 0 ? 1000 / 60 : now - this.lastDrawTime;
    this.lastDrawTime = now;

    const vidW = video.videoWidth;
    const vidH = video.videoHeight;

    if (!vidW || !vidH) {
      this.isDrawing = false;
      return;
    }

    const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const srcX = vidW * crop.x;
    const srcY = vidH * crop.y;
    const srcW = vidW * crop.width;
    const srcH = vidH * crop.height;

    // Canvas dimensions: custom overrides auto (crop-based)
    const useCustomCanvas = backgroundConfig.canvasMode === 'custom' && backgroundConfig.canvasWidth && backgroundConfig.canvasHeight;
    const canvasW = useCustomCanvas ? backgroundConfig.canvasWidth! : Math.round(srcW);
    const canvasH = useCustomCanvas ? backgroundConfig.canvasHeight! : Math.round(srcH);

    if (canvas.width !== canvasW || canvas.height !== canvasH) {
      canvas.width = canvasW;
      canvas.height = canvasH;
    }

    if (!isExportMode) {
      canvas.style.aspectRatio = `${canvasW} / ${canvasH}`;
    }

    try {
      const legacyCrop = (backgroundConfig.cropBottom || 0) / 100;
      const scale = backgroundConfig.scale / 100;

      // Contain-fit: fit the cropped video into the canvas maintaining aspect ratio
      const effectiveSrcH = srcH * (1 - legacyCrop);
      const srcAspect = srcW / effectiveSrcH;
      const canvasAspect = canvasW / canvasH;
      let fitW: number, fitH: number;
      if (srcAspect > canvasAspect) {
        fitW = canvasW;
        fitH = canvasW / srcAspect;
      } else {
        fitH = canvasH;
        fitW = canvasH * srcAspect;
      }
      const scaledWidth = fitW * scale;
      const scaledHeight = fitH * scale;
      const x = (canvasW - scaledWidth) / 2;
      const y = (canvasH - scaledHeight) / 2;

      const zoomState = this.calculateCurrentZoomState(video.currentTime, segment, canvas.width, canvas.height);

      // Supersample only during export to keep preview responsive
      const zf = zoomState?.zoomFactor ?? 1;
      const ss = isExportMode && zf > 1 ? Math.min(Math.ceil(zf), 3) : 1;

      ctx.save();

      if (zoomState && zoomState.zoomFactor !== 1) {
        const zoomedWidth = canvas.width * zoomState.zoomFactor;
        const zoomedHeight = canvas.height * zoomState.zoomFactor;
        const zoomOffsetX = (canvas.width - zoomedWidth) * zoomState.positionX;
        const zoomOffsetY = (canvas.height - zoomedHeight) * zoomState.positionY;

        ctx.translate(zoomOffsetX, zoomOffsetY);
        ctx.scale(zoomState.zoomFactor, zoomState.zoomFactor);
      }

      ctx.fillStyle = this.getBackgroundStyle(
        ctx,
        backgroundConfig.backgroundType,
        backgroundConfig.customBackground
      );
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      const tempW = canvasW * ss;
      const tempH = canvasH * ss;
      if (tempCanvas.width !== tempW || tempCanvas.height !== tempH) {
        tempCanvas.width = tempW;
        tempCanvas.height = tempH;
      }
      const tempCtx = tempCanvas.getContext('2d', { alpha: true, willReadFrequently: false });
      if (!tempCtx) return;

      tempCtx.clearRect(0, 0, tempW, tempH);
      tempCtx.save();
      tempCtx.imageSmoothingEnabled = true;
      tempCtx.imageSmoothingQuality = 'high';
      if (ss > 1) tempCtx.scale(ss, ss);

      const radius = backgroundConfig.borderRadius;
      const offset = 0.5;

      if (backgroundConfig.shadow) {
        tempCtx.save();
        tempCtx.shadowColor = 'rgba(0, 0, 0, 0.5)';
        tempCtx.shadowBlur = backgroundConfig.shadow;
        tempCtx.shadowOffsetY = backgroundConfig.shadow * 0.5;

        tempCtx.beginPath();
        tempCtx.moveTo(x + radius + offset, y + offset);
        tempCtx.lineTo(x + scaledWidth - radius - offset, y + offset);
        tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + offset, x + scaledWidth - offset, y + radius + offset);
        tempCtx.lineTo(x + scaledWidth - offset, y + scaledHeight - radius - offset);
        tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + scaledHeight - offset, x + scaledWidth - radius - offset, y + scaledHeight - offset);
        tempCtx.lineTo(x + radius + offset, y + scaledHeight - offset);
        tempCtx.quadraticCurveTo(x + offset, y + scaledHeight - offset, x + offset, y + scaledHeight - radius - offset);
        tempCtx.lineTo(x + offset, y + radius + offset);
        tempCtx.quadraticCurveTo(x + offset, y + offset, x + radius + offset, y + offset);
        tempCtx.closePath();

        tempCtx.fillStyle = '#fff';
        tempCtx.fill();
        tempCtx.restore();
      }

      tempCtx.beginPath();
      tempCtx.moveTo(x + radius + offset, y + offset);
      tempCtx.lineTo(x + scaledWidth - radius - offset, y + offset);
      tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + offset, x + scaledWidth - offset, y + radius + offset);
      tempCtx.lineTo(x + scaledWidth - offset, y + scaledHeight - radius - offset);
      tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + scaledHeight - offset, x + scaledWidth - radius - offset, y + scaledHeight - offset);
      tempCtx.lineTo(x + radius + offset, y + scaledHeight - offset);
      tempCtx.quadraticCurveTo(x + offset, y + scaledHeight - offset, x + offset, y + scaledHeight - radius - offset);
      tempCtx.lineTo(x + offset, y + radius + offset);
      tempCtx.quadraticCurveTo(x + offset, y + offset, x + radius + offset, y + offset);
      tempCtx.closePath();

      tempCtx.clip();

      try {
        tempCtx.drawImage(
          video,
          srcX, srcY, srcW, srcH * (1 - legacyCrop),
          x, y, scaledWidth, scaledHeight
        );
      } catch (e) {
      }

      tempCtx.strokeStyle = 'rgba(0, 0, 0, 0.1)';
      tempCtx.lineWidth = 1;
      tempCtx.stroke();
      tempCtx.restore();

      ctx.drawImage(tempCanvas, 0, 0, canvasW, canvasH);

      const cursorTime = video.currentTime + CURSOR_OFFSET_SEC;
      const interpolatedPosition = this.interpolateCursorPosition(
        cursorTime,
        mousePositions
      );

      if (interpolatedPosition) {
        // Cursor visibility (smart pointer hiding)
        const cursorVis = getCursorVisibility(video.currentTime, segment.cursorVisibilitySegments);

        if (cursorVis.opacity > 0.001) {
          ctx.save();
          ctx.setTransform(1, 0, 0, 1, 0, 0);

          const mX = interpolatedPosition.x;
          const mY = interpolatedPosition.y;

          const relX = (mX - srcX) / srcW;
          const relY = (mY - srcY) / (srcH * (1 - legacyCrop));

          let cursorX = x + (relX * scaledWidth);
          let cursorY = y + (relY * scaledHeight);

          if (zoomState && zoomState.zoomFactor !== 1) {
            cursorX = cursorX * zoomState.zoomFactor + (canvas.width - canvas.width * zoomState.zoomFactor) * zoomState.positionX;
            cursorY = cursorY * zoomState.zoomFactor + (canvas.height - canvas.height * zoomState.zoomFactor) * zoomState.positionY;
          }

          const sizeRatio = Math.min(canvas.width / srcW, canvas.height / srcH);
          const cursorSizeScale = (backgroundConfig.cursorScale || 2) * sizeRatio * (zoomState?.zoomFactor || 1) * cursorVis.scale;

          const isActuallyClicked = interpolatedPosition.isClicked;
          const timeSinceLastHold = video.currentTime - this.lastHoldTime;
          const shouldBeSquished = isActuallyClicked || (this.lastHoldTime >= 0 && timeSinceLastHold < this.CLICK_FUSE_THRESHOLD && timeSinceLastHold > 0);

          if (isActuallyClicked) {
            this.lastHoldTime = video.currentTime;
          }

          const targetScale = shouldBeSquished ? 0.75 : 1.0;
          if (this.currentSquishScale > targetScale) {
            this.currentSquishScale = Math.max(targetScale, this.currentSquishScale - this.SQUISH_SPEED * (this.latestElapsed / (1000 / 120)));
          } else if (this.currentSquishScale < targetScale) {
            this.currentSquishScale = Math.min(targetScale, this.currentSquishScale + this.RELEASE_SPEED * (this.latestElapsed / (1000 / 120)));
          }

          ctx.globalAlpha = cursorVis.opacity;
          this.drawMouseCursor(
            ctx,
            cursorX,
            cursorY,
            shouldBeSquished,
            cursorSizeScale,
            interpolatedPosition.cursor_type || 'default'
          );

          ctx.restore();
        }
      }

      this.backgroundConfig = context.backgroundConfig;

      if (segment.textSegments) {
        const FADE_DURATION = 0.3;
        const isPlaying = !video.paused;
        for (const textSegment of segment.textSegments) {
          if (video.currentTime >= textSegment.startTime && video.currentTime <= textSegment.endTime) {
            let fadeAlpha = 1.0;
            if (isPlaying) {
              const elapsed = video.currentTime - textSegment.startTime;
              const remaining = textSegment.endTime - video.currentTime;
              if (elapsed < FADE_DURATION) fadeAlpha = elapsed / FADE_DURATION;
              if (remaining < FADE_DURATION) fadeAlpha = Math.min(fadeAlpha, remaining / FADE_DURATION);
            }
            this.drawTextOverlay(ctx, textSegment, canvas.width, canvas.height, fadeAlpha);
          }
        }
        // Reset font-variation-settings so it doesn't leak into non-text rendering
        canvas.style.fontVariationSettings = 'normal';
      }

    } finally {
      this.isDrawing = false;
      ctx.restore();
    }
  };

  private getBackgroundStyle(
    ctx: CanvasRenderingContext2D,
    type: BackgroundConfig['backgroundType'],
    customBackground?: string
  ): string | CanvasGradient | CanvasPattern {
    switch (type) {
      case 'gradient1': {
        const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
        gradient.addColorStop(0, '#2563eb');
        gradient.addColorStop(1, '#7c3aed');
        return gradient;
      }
      case 'gradient2': {
        const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
        gradient.addColorStop(0, '#fb7185');
        gradient.addColorStop(1, '#fdba74');
        return gradient;
      }
      case 'gradient3': {
        const gradient = ctx.createLinearGradient(0, 0, ctx.canvas.width, 0);
        gradient.addColorStop(0, '#10b981');
        gradient.addColorStop(1, '#2dd4bf');
        return gradient;
      }
      case 'custom': {
        if (customBackground) {
          if (this.lastCustomBackground !== customBackground || !this.customBackgroundPattern) {
            const img = new Image();
            img.src = customBackground;

            if (img.complete) {
              const tempCanvas = document.createElement('canvas');
              const tempCtx = tempCanvas.getContext('2d');

              if (tempCtx) {
                const targetWidth = Math.min(1920, window.innerWidth);
                const scale = targetWidth / img.width;
                const targetHeight = img.height * scale;

                tempCanvas.width = targetWidth;
                tempCanvas.height = targetHeight;
                tempCtx.imageSmoothingEnabled = true;
                tempCtx.imageSmoothingQuality = 'high';
                tempCtx.drawImage(img, 0, 0, targetWidth, targetHeight);
                this.customBackgroundPattern = ctx.createPattern(tempCanvas, 'repeat');
                this.lastCustomBackground = customBackground;
                tempCanvas.remove();
              }
            }
          }

          if (this.customBackgroundPattern) {
            this.customBackgroundPattern.setTransform(new DOMMatrix());
            const scale = Math.max(
              ctx.canvas.width / window.innerWidth,
              ctx.canvas.height / window.innerHeight
            ) * 1.1;
            const matrix = new DOMMatrix().scale(scale);
            this.customBackgroundPattern.setTransform(matrix);
            return this.customBackgroundPattern;
          }
        }
        return '#000000';
      }
      case 'solid': {
        const gradient = ctx.createLinearGradient(0, 0, 0, ctx.canvas.height);
        gradient.addColorStop(0, '#0a0a0a');
        gradient.addColorStop(0.5, '#000000');
        gradient.addColorStop(1, '#0a0a0a');

        const centerX = ctx.canvas.width / 2;
        const centerY = ctx.canvas.height / 2;
        const radialGradient = ctx.createRadialGradient(
          centerX, centerY, 0,
          centerX, centerY, ctx.canvas.width * 0.8
        );
        radialGradient.addColorStop(0, 'rgba(30, 30, 30, 0.15)');
        radialGradient.addColorStop(1, 'rgba(0, 0, 0, 0)');

        ctx.fillStyle = gradient;
        ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
        ctx.fillStyle = radialGradient;
        ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);

        return 'rgba(0,0,0,0)';
      }
      default:
        return '#000000';
    }
  }

  private calculateCurrentZoomState(
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number
  ): ZoomKeyframe {
    const isPaused = this.activeRenderContext?.video?.paused ?? true;

    // Only recompute bake signature when segment reference or view dims change.
    // Avoids JSON.stringify + .map() allocations on every frame (was 120x/sec).
    if (segment !== this.lastBakeSegment || viewW !== this.lastBakeViewW || viewH !== this.lastBakeViewH) {
      this.lastBakeSegment = segment;
      this.lastBakeViewW = viewW;
      this.lastBakeViewH = viewH;

      const signature = JSON.stringify({
        trim: [segment.trimStart, segment.trimEnd],
        crop: segment.crop,
        smoothMotionPath: segment.smoothMotionPath?.map(p => ({ t: p.time, z: p.zoom })),
        zoomKeyframes: segment.zoomKeyframes?.map(k => ({ t: k.time, d: k.duration, x: k.positionX, y: k.positionY, z: k.zoomFactor })),
        zoomInfluence: segment.zoomInfluencePoints?.map(p => ({ t: p.time, v: p.value })),
        cursorVis: segment.cursorVisibilitySegments?.map(s => ({ s: s.startTime, e: s.endTime })),
        vidDims: [viewW, viewH]
      });

      if (this.lastBakeSignature !== signature) {
        this.cachedBakedPath = this.generateBakedPath(segment, viewW / (segment.crop?.width || 1), viewH / (segment.crop?.height || 1), 60);
        this.lastBakeSignature = signature;
      }
    }

    if (!isPaused && this.cachedBakedPath && this.cachedBakedPath.length > 0) {
      const relTime = currentTime - segment.trimStart;
      const step = 1 / 60;
      const idx = Math.floor(relTime / step);

      if (idx >= 0 && idx < this.cachedBakedPath.length) {
        const p1 = this.cachedBakedPath[idx];
        const p2 = this.cachedBakedPath[idx + 1] || p1;
        const t = (relTime % step) / step;

        const globalX = p1.x + (p2.x - p1.x) * t;
        const globalY = p1.y + (p2.y - p1.y) * t;
        const zoom = p1.zoom + (p2.zoom - p1.zoom) * t;

        const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
        const fullW = viewW / crop.width;
        const fullH = viewH / crop.height;
        const cropOffsetX = fullW * crop.x;
        const cropOffsetY = fullH * crop.y;

        const state: ZoomKeyframe = {
          time: currentTime,
          duration: 0,
          zoomFactor: zoom,
          positionX: Math.max(0, Math.min(1, (globalX - cropOffsetX) / viewW)),
          positionY: Math.max(0, Math.min(1, (globalY - cropOffsetY) / viewH)),
          easingType: 'linear'
        };
        this.lastCalculatedState = state;
        return state;
      }
    }

    const state = this.calculateCurrentZoomStateInternal(currentTime, segment, viewW, viewH);
    this.lastCalculatedState = state;
    return state;
  }

  private calculateCurrentZoomStateInternal(
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number
  ): ZoomKeyframe {

    // --- 1. CALCULATE AUTO-SMART ZOOM STATE (Background Track) ---
    const hasAutoPath = segment.smoothMotionPath && segment.smoothMotionPath.length > 0;
    let autoState: ZoomKeyframe | null = null;

    if (hasAutoPath) {
      const path = segment.smoothMotionPath!;
      const idx = path.findIndex((p: any) => p.time >= currentTime);
      let cam = { x: viewW / 2, y: viewH / 2, zoom: 1.0 };

      if (idx === -1) {
        const last = path[path.length - 1];
        cam = { x: last.x, y: last.y, zoom: last.zoom };
      } else if (idx === 0) {
        const first = path[0];
        cam = { x: first.x, y: first.y, zoom: first.zoom };
      } else {
        const p1 = path[idx - 1];
        const p2 = path[idx];
        const t = (currentTime - p1.time) / (p2.time - p1.time);
        cam = {
          x: p1.x + (p2.x - p1.x) * t,
          y: p1.y + (p2.y - p1.y) * t,
          zoom: p1.zoom + (p2.zoom - p1.zoom) * t
        };
      }

      // Apply Influence
      if (segment.zoomInfluencePoints && segment.zoomInfluencePoints.length > 0) {
        const points = segment.zoomInfluencePoints;
        let influence = 1.0;
        const iIdx = points.findIndex((p: { time: number }) => p.time >= currentTime);
        if (iIdx === -1) {
          influence = points[points.length - 1].value;
        } else if (iIdx === 0) {
          influence = points[0].value;
        } else {
          const ip1 = points[iIdx - 1];
          const ip2 = points[iIdx];
          const it = (currentTime - ip1.time) / (ip2.time - ip1.time);
          const cosT = (1 - Math.cos(it * Math.PI)) / 2;
          influence = ip1.value * (1 - cosT) + ip2.value * cosT;
        }
        cam.zoom = 1.0 + (cam.zoom - 1.0) * influence;
        // Use crop center in full-video coords so influence=0 returns to crop center,
        // not viewW/2 which is only half the cropped width (wrong for non-center crops)
        const cropInf = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
        const fullWInf = viewW / cropInf.width;
        const fullHInf = viewH / cropInf.height;
        const centerX = fullWInf * cropInf.x + viewW / 2;
        const centerY = fullHInf * cropInf.y + viewH / 2;
        cam.x = centerX + (cam.x - centerX) * influence;
        cam.y = centerY + (cam.y - centerY) * influence;
      }

      const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
      const fullW = viewW / crop.width;
      const fullH = viewH / crop.height;
      const cropOffsetX = fullW * crop.x;
      const cropOffsetY = fullH * crop.y;

      autoState = {
        time: currentTime,
        duration: 0,
        zoomFactor: cam.zoom,
        positionX: (cam.x - cropOffsetX) / viewW,
        positionY: (cam.y - cropOffsetY) / viewH,
        easingType: 'linear'
      };
    }

    // --- 2. CALCULATE MANUAL KEYFRAME STATE (Foreground Track) ---
    // Improved logic to blend seamlessly with Auto-Zoom

    let manualState: ZoomKeyframe | null = null;
    let manualInfluence = 0.0;

    const sortedKeyframes = [...segment.zoomKeyframes].sort((a: ZoomKeyframe, b: ZoomKeyframe) => a.time - b.time);

    if (sortedKeyframes.length > 0) {
      // Dynamic blending window size based on movement
      const calculateDynamicWindow = (kf1: ZoomKeyframe, kf2?: ZoomKeyframe) => {
        if (!kf2) return 3.0; // Default tail if single keyframe
        const dx = Math.abs(kf1.positionX - kf2.positionX);
        const dy = Math.abs(kf1.positionY - kf2.positionY);
        const dz = Math.abs(kf1.zoomFactor - kf2.zoomFactor);
        const distanceScore = Math.sqrt(dx * dx + dy * dy) + (dz * 0.5);
        return Math.max(1.5, Math.min(4.0, distanceScore * 3.0)); // Adaptive 1.5s to 4s
      };

      const nextKfIdx = sortedKeyframes.findIndex(k => k.time > currentTime);
      const prevKf = nextKfIdx > 0 ? sortedKeyframes[nextKfIdx - 1] : (nextKfIdx === -1 ? sortedKeyframes[sortedKeyframes.length - 1] : null);
      const nextKf = nextKfIdx !== -1 ? sortedKeyframes[nextKfIdx] : null;

      if (prevKf && nextKf) {
        // BETWEEN TWO KEYFRAMES — always smoothly interpolate between adjacent keyframes.
        // Manual keyframes form a continuous connected curve regardless of auto-path.
        // No decay to default between keyframes — no independent humps.
        manualInfluence = 1.0;
        const timeDiff = nextKf.time - prevKf.time;
        const rawT = (currentTime - prevKf.time) / timeDiff;
        const t = Math.max(0, Math.min(1, rawT));
        const easedT = this.easeCameraMove(t);

        const { zoom: currentZoom, posX, posY } = this.blendZoomStates(prevKf, nextKf, easedT);

        manualState = {
          time: currentTime, duration: 0, zoomFactor: currentZoom, positionX: posX, positionY: posY, easingType: 'easeOut'
        };
      } else if (prevKf) {
        // AFTER LAST KEYFRAME
        if (hasAutoPath) {
          const currentTarget = autoState || this.DEFAULT_STATE;
          const decayWindow = calculateDynamicWindow(prevKf, currentTarget);

          const timeFromPrev = currentTime - prevKf.time;
          if (timeFromPrev < decayWindow) {
            const progress = timeFromPrev / decayWindow; // 0 at keyframe → 1 at end of decay
            manualInfluence = 1 - this.easeCameraMove(progress);
          }
        } else {
          // Hold last keyframe forever if no auto path
          manualInfluence = 1.0;
        }
        manualState = prevKf;
      } else if (nextKf) {
        // BEFORE FIRST KEYFRAME — cosine ease from default to keyframe
        const currentTarget = autoState || this.DEFAULT_STATE;
        const hasCustomDuration = nextKf.duration > 0;
        const rampWindow = hasCustomDuration ? nextKf.duration : calculateDynamicWindow(nextKf, currentTarget);

        const timeToNext = nextKf.time - currentTime;
        if (timeToNext <= rampWindow) {
          const progress = 1 - timeToNext / rampWindow; // 0 at ramp start → 1 at keyframe
          manualInfluence = this.easeCameraMove(progress);
        }
        manualState = nextKf;
      }
    }

    // --- 3. FINAL BLENDING ---

    let result: ZoomKeyframe;

    if (autoState) {
      if (manualState && manualInfluence > 0.001) {
        // Blend Auto and Manual in viewport-center space
        const { zoom: finalZoom, posX: finalX, posY: finalY } = this.blendZoomStates(autoState, manualState, manualInfluence);
        result = { time: currentTime, duration: 0, zoomFactor: finalZoom, positionX: finalX, positionY: finalY, easingType: 'linear' };
      } else {
        // Pure Auto
        result = autoState;
      }
    } else if (manualState && manualInfluence > 0.001) {
      // No Auto path — always blend (no threshold skip that creates zoom jumps)
      const def = this.DEFAULT_STATE;
      const { zoom: finalZoom, posX: finalX, posY: finalY } = this.blendZoomStates(def, manualState, manualInfluence);
      result = { time: currentTime, duration: 0, zoomFactor: finalZoom, positionX: finalX, positionY: finalY, easingType: 'linear' };
    } else {
      return this.DEFAULT_STATE;
    }

    // Clamp position to valid viewport range — prevents off-screen navigation
    // when auto-zoom targets points outside the crop region or blending overshoots
    result.positionX = Math.max(0, Math.min(1, result.positionX));
    result.positionY = Math.max(0, Math.min(1, result.positionY));
    return result;
  }

  // --- Utility functions needed for the interpolation ---
  private catmullRomInterpolate(p0: number, p1: number, p2: number, p3: number, t: number): number {
    const t2 = t * t;
    const t3 = t2 * t;
    return 0.5 * (
      (2 * p1) +
      (-p0 + p2) * t +
      (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
      (-p0 + 3 * p1 - 3 * p2 + p3) * t3
    );
  }

  private smoothMousePositions(positions: MousePosition[], targetFps: number = 120): MousePosition[] {
    if (positions.length < 4) return positions;
    const smoothed: MousePosition[] = [];

    for (let i = 0; i < positions.length - 3; i++) {
      const p0 = positions[i];
      const p1 = positions[i + 1];
      const p2 = positions[i + 2];
      const p3 = positions[i + 3];

      const segmentDuration = p2.timestamp - p1.timestamp;
      const numFrames = Math.ceil(segmentDuration * targetFps);

      for (let frame = 0; frame < numFrames; frame++) {
        const t = frame / numFrames;
        const timestamp = p1.timestamp + (segmentDuration * t);
        const x = this.catmullRomInterpolate(p0.x, p1.x, p2.x, p3.x, t);
        const y = this.catmullRomInterpolate(p0.y, p1.y, p2.y, p3.y, t);
        const isClicked = Boolean(p1.isClicked || p2.isClicked);
        const cursor_type = t < 0.5 ? p1.cursor_type : p2.cursor_type;
        smoothed.push({ x, y, timestamp, isClicked, cursor_type });
      }
    }

    const windowSize = ((this.backgroundConfig?.cursorSmoothness || 5) * 2) + 1;
    const passes = Math.ceil(windowSize / 2);
    let currentSmoothed = smoothed;

    for (let pass = 0; pass < passes; pass++) {
      const passSmoothed: MousePosition[] = [];
      for (let i = 0; i < currentSmoothed.length; i++) {
        let sumX = 0;
        let sumY = 0;
        let totalWeight = 0;
        const cursor_type = currentSmoothed[i].cursor_type;

        for (let j = Math.max(0, i - windowSize); j <= Math.min(currentSmoothed.length - 1, i + windowSize); j++) {
          const distance = Math.abs(i - j);
          const weight = Math.exp(-distance * (0.5 / windowSize));
          sumX += currentSmoothed[j].x * weight;
          sumY += currentSmoothed[j].y * weight;
          totalWeight += weight;
        }

        passSmoothed.push({
          x: sumX / totalWeight,
          y: sumY / totalWeight,
          timestamp: currentSmoothed[i].timestamp,
          isClicked: currentSmoothed[i].isClicked,
          cursor_type
        });
      }
      currentSmoothed = passSmoothed;
    }

    const threshold = 0.5 / (windowSize / 2);
    let lastSignificantPos = currentSmoothed[0];
    const finalSmoothed = [lastSignificantPos];

    for (let i = 1; i < currentSmoothed.length; i++) {
      const current = currentSmoothed[i];
      const distance = Math.sqrt(
        Math.pow(current.x - lastSignificantPos.x, 2) +
        Math.pow(current.y - lastSignificantPos.y, 2)
      );

      if (distance > threshold || current.isClicked !== lastSignificantPos.isClicked) {
        finalSmoothed.push(current);
        lastSignificantPos = current;
      } else {
        finalSmoothed.push({
          ...lastSignificantPos,
          timestamp: current.timestamp
        });
      }
    }

    return finalSmoothed;
  }

  private interpolateCursorPosition(
    currentTime: number,
    mousePositions: MousePosition[],
  ): { x: number; y: number; isClicked: boolean; cursor_type: string } | null {
    // 1. Invalidate cache if input changed
    if (this.lastMousePositionsRef !== mousePositions) {
      this.smoothedPositions = null;
      this.lastMousePositionsRef = mousePositions;
    }

    // 2. Generate cache if needed
    if (!this.smoothedPositions && mousePositions.length > 0) {
      this.smoothedPositions = this.smoothMousePositions(mousePositions);
    }

    // 3. Use cached data
    const dataToUse = this.smoothedPositions || mousePositions;

    return this.interpolateCursorPositionInternal(currentTime, dataToUse);
  }

  // Internal version to support both live and export baking
  private interpolateCursorPositionInternal(
    currentTime: number,
    positions: MousePosition[],
  ): { x: number; y: number; isClicked: boolean; cursor_type: string } | null {
    if (!positions || positions.length === 0) return null;

    const exactMatch = positions.find((pos: MousePosition) => Math.abs(pos.timestamp - currentTime) < 0.001);
    if (exactMatch) {
      return {
        x: exactMatch.x,
        y: exactMatch.y,
        isClicked: Boolean(exactMatch.isClicked),
        cursor_type: exactMatch.cursor_type || 'default'
      };
    }

    const nextIndex = positions.findIndex((pos: MousePosition) => pos.timestamp > currentTime);
    if (nextIndex === -1) {
      const last = positions[positions.length - 1];
      return {
        x: last.x,
        y: last.y,
        isClicked: Boolean(last.isClicked),
        cursor_type: last.cursor_type || 'default'
      };
    }

    if (nextIndex === 0) {
      const first = positions[0];
      return {
        x: first.x,
        y: first.y,
        isClicked: Boolean(first.isClicked),
        cursor_type: first.cursor_type || 'default'
      };
    }

    const prev = positions[nextIndex - 1];
    const next = positions[nextIndex];
    const t = (currentTime - prev.timestamp) / (next.timestamp - prev.timestamp);

    return {
      x: prev.x + (next.x - prev.x) * t,
      y: prev.y + (next.y - prev.y) * t,
      isClicked: Boolean(prev.isClicked || next.isClicked),
      cursor_type: next.cursor_type || 'default'
    };
  }

  private drawMouseCursor(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    isClicked: boolean,
    scale: number = 2,
    cursorType: string = 'default'
  ) {
    // When globalAlpha < 1 (cursor fade in/out), drawing strokes+fills directly
    // causes white borders to bleed through semi-transparent black fills.
    // Fix: draw at full opacity onto offscreen canvas, then stamp with globalAlpha.
    if (ctx.globalAlpha < 0.999) {
      const margin = 64;
      const size = Math.ceil(scale * 48) + margin * 2;
      if (this.cursorOffscreen.width !== size || this.cursorOffscreen.height !== size) {
        this.cursorOffscreen.width = size;
        this.cursorOffscreen.height = size;
        this.cursorOffscreenCtx = this.cursorOffscreen.getContext('2d')!;
      }
      const oCtx = this.cursorOffscreenCtx;
      oCtx.clearRect(0, 0, size, size);
      oCtx.globalAlpha = 1;
      this.drawCursorShape(oCtx as unknown as CanvasRenderingContext2D, margin, margin, isClicked, scale, cursorType);
      ctx.save();
      ctx.drawImage(this.cursorOffscreen, x - margin, y - margin);
      ctx.restore();
    } else {
      ctx.save();
      this.drawCursorShape(ctx, x, y, isClicked, scale, cursorType);
      ctx.restore();
    }
  }

  private drawCursorShape(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    _isClicked: boolean,
    scale: number = 2,
    cursorType: string
  ) {
    const lowerType = cursorType.toLowerCase();
    ctx.save();
    ctx.translate(x, y);
    ctx.scale(scale, scale);
    ctx.scale(this.currentSquishScale, this.currentSquishScale);

    let effectiveType = lowerType;
    if (effectiveType === 'pointer') {
      if (!this.pointerImage.complete || this.pointerImage.naturalWidth === 0) {
        effectiveType = 'default';
      }
    }

    switch (effectiveType) {
      case 'text': {
        // I-beam shape
        // Adjust for hotspot: I-beam center is roughly (3, 8)
        ctx.translate(-6, -8);
        const ibeam = new Path2D(`
          M 2 0 L 10 0 L 10 2 L 7 2 L 7 14 L 10 14 L 10 16 L 2 16 L 2 14 L 5 14 L 5 2 L 2 2 Z
        `);
        ctx.strokeStyle = 'white';
        ctx.lineWidth = 1.5;
        ctx.stroke(ibeam);
        ctx.fillStyle = 'black';
        ctx.fill(ibeam);
        break;
      }

      case 'pointer': {
        // Hand cursor image
        let imgWidth = 24, imgHeight = 24;
        if (this.pointerImage.complete && this.pointerImage.naturalWidth > 0) {
          imgWidth = this.pointerImage.naturalWidth;
          imgHeight = this.pointerImage.naturalHeight;
          const offsetX = 8;
          const offsetY = 16;
          // Note: TS draws offset, we need to match this in Rust
          ctx.translate(-imgWidth / 2 + offsetX, -imgHeight / 2 + offsetY);
          ctx.drawImage(this.pointerImage, 0, 0, imgWidth, imgHeight);
        }
        break;
      }

      default: {
        // Standard Arrow
        // We translate by (-8, -5) to bring tip to (0,0)
        ctx.translate(-8, -5);
        const mainArrow = new Path2D('M 8.2 4.9 L 19.8 16.5 L 13 16.5 L 12.6 16.6 L 8.2 20.9 Z');
        const clickIndicator = new Path2D('M 17.3 21.6 L 13.7 23.1 L 9 12 L 12.7 10.5 Z');
        ctx.strokeStyle = 'white';
        ctx.lineWidth = 1.5;
        ctx.stroke(mainArrow);
        ctx.stroke(clickIndicator);
        ctx.fillStyle = 'black';
        ctx.fill(mainArrow);
        ctx.fill(clickIndicator);
        break;
      }
    }
    ctx.restore();
  }

  private drawTextOverlay(
    ctx: CanvasRenderingContext2D,
    textSegment: TextSegment,
    width: number,
    height: number,
    fadeAlpha: number = 1.0
  ) {
    const { style } = textSegment;
    const textAlign = style.textAlign ?? 'center';
    const opacity = style.opacity ?? 1;
    const letterSpacing = style.letterSpacing ?? 0;
    const background = style.background;
    const fontSize = style.fontSize;

    const vars = style.fontVariations;
    const wght = vars?.wght ?? (style.fontWeight === 'bold' ? 700 : 400);

    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0); // Reset to identity — text is viewport-relative
    ctx.globalAlpha = opacity * fadeAlpha;

    // Set font-variation-settings on canvas element CSS — the only way to control
    // variable font axes (wdth, slnt, ROND) in Canvas 2D (no native API exists).
    this.applyFontVariations(ctx, vars);
    ctx.font = `${wght} ${fontSize}px 'Google Sans Flex', sans-serif`;

    ctx.textBaseline = 'middle';

    // Split text by newlines for multi-line
    const lines = textSegment.text.split('\n');
    const lineHeight = fontSize * 1.25;

    // Measure each line width (account for letter spacing)
    const measureLine = (line: string): number => {
      const baseWidth = ctx.measureText(line).width;
      if (letterSpacing !== 0 && line.length > 1) {
        return baseWidth + letterSpacing * (line.length - 1);
      }
      return baseWidth;
    };

    const lineWidths = lines.map(measureLine);
    const maxLineWidth = Math.max(...lineWidths);
    const totalHeight = lines.length * lineHeight;

    // Anchor position (0-100% based)
    const anchorX = (style.x / 100) * width;
    const anchorY = (style.y / 100) * height;

    // Background pill padding
    const bgPadX = background?.enabled ? (background.paddingX ?? 16) : 0;
    const bgPadY = background?.enabled ? (background.paddingY ?? 8) : 0;

    // Hit area encompasses all lines + padding
    const hitPad = 10;
    let blockLeft: number;
    if (textAlign === 'left') {
      blockLeft = anchorX;
    } else if (textAlign === 'right') {
      blockLeft = anchorX - maxLineWidth;
    } else {
      blockLeft = anchorX - maxLineWidth / 2;
    }
    const blockTop = anchorY - totalHeight / 2;

    const hitArea = {
      x: blockLeft - bgPadX - hitPad,
      y: blockTop - bgPadY - hitPad,
      width: maxLineWidth + bgPadX * 2 + hitPad * 2,
      height: totalHeight + bgPadY * 2 + hitPad * 2
    };

    // Background pill
    if (background?.enabled) {
      const pillX = blockLeft - bgPadX;
      const pillY = blockTop - bgPadY;
      const pillW = maxLineWidth + bgPadX * 2;
      const pillH = totalHeight + bgPadY * 2;
      const r = Math.min(background.borderRadius ?? 8, pillW / 2, pillH / 2);

      const savedAlpha = ctx.globalAlpha;
      ctx.globalAlpha = savedAlpha * (background.opacity ?? 0.6);
      ctx.beginPath();
      ctx.roundRect(pillX, pillY, pillW, pillH, r);
      ctx.fillStyle = background.color ?? '#000000';
      ctx.fill();
      ctx.globalAlpha = savedAlpha;
    }

    // Draw each line
    ctx.shadowColor = 'rgba(0,0,0,0.7)';
    ctx.shadowBlur = 4;
    ctx.shadowOffsetX = 2;
    ctx.shadowOffsetY = 2;
    ctx.fillStyle = style.color;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const ly = blockTop + i * lineHeight + lineHeight / 2;
      let lx: number;
      if (textAlign === 'left') {
        lx = blockLeft;
      } else if (textAlign === 'right') {
        lx = blockLeft + maxLineWidth;
      } else {
        lx = blockLeft + maxLineWidth / 2;
      }

      if (letterSpacing !== 0 && line.length > 1) {
        // Char-by-char rendering for letter spacing
        this.drawTextWithSpacing(ctx, line, lx, ly, letterSpacing, textAlign, lineWidths[i]);
      } else {
        ctx.textAlign = textAlign;
        ctx.fillText(line, lx, ly);
      }
    }

    ctx.restore();
    return hitArea;
  }

  private drawTextWithSpacing(
    ctx: CanvasRenderingContext2D,
    text: string,
    x: number,
    y: number,
    spacing: number,
    align: CanvasTextAlign,
    totalWidth: number
  ) {
    ctx.textAlign = 'left';
    let startX: number;
    if (align === 'center') {
      startX = x - totalWidth / 2;
    } else if (align === 'right') {
      startX = x - totalWidth;
    } else {
      startX = x;
    }

    let cx = startX;
    for (let i = 0; i < text.length; i++) {
      ctx.fillText(text[i], cx, y);
      cx += ctx.measureText(text[i]).width + spacing;
    }
  }

  private getTextHitArea(
    ctx: CanvasRenderingContext2D,
    textSegment: TextSegment,
    width: number,
    height: number
  ) {
    const { style } = textSegment;
    const textAlign = style.textAlign ?? 'center';
    const letterSpacing = style.letterSpacing ?? 0;
    const fontSize = style.fontSize;
    const background = style.background;

    const vars = style.fontVariations;
    const wght = vars?.wght ?? (style.fontWeight === 'bold' ? 700 : 400);

    ctx.save();
    this.applyFontVariations(ctx, vars);
    ctx.font = `${wght} ${fontSize}px 'Google Sans Flex', sans-serif`;

    const lines = textSegment.text.split('\n');
    const lineHeight = fontSize * 1.25;

    const measureLine = (line: string): number => {
      const baseWidth = ctx.measureText(line).width;
      if (letterSpacing !== 0 && line.length > 1) {
        return baseWidth + letterSpacing * (line.length - 1);
      }
      return baseWidth;
    };

    const maxLineWidth = Math.max(...lines.map(measureLine));
    const totalHeight = lines.length * lineHeight;

    const anchorX = (style.x / 100) * width;
    const anchorY = (style.y / 100) * height;

    const bgPadX = background?.enabled ? (background.paddingX ?? 16) : 0;
    const bgPadY = background?.enabled ? (background.paddingY ?? 8) : 0;
    const hitPad = 10;

    let blockLeft: number;
    if (textAlign === 'left') {
      blockLeft = anchorX;
    } else if (textAlign === 'right') {
      blockLeft = anchorX - maxLineWidth;
    } else {
      blockLeft = anchorX - maxLineWidth / 2;
    }
    const blockTop = anchorY - totalHeight / 2;

    ctx.restore();

    return {
      x: blockLeft - bgPadX - hitPad,
      y: blockTop - bgPadY - hitPad,
      width: maxLineWidth + bgPadX * 2 + hitPad * 2,
      height: totalHeight + bgPadY * 2 + hitPad * 2
    };
  }

  public handleMouseDown(e: MouseEvent, segment: VideoSegment, canvas: HTMLCanvasElement): string | null {
    const rect = canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left) * (canvas.width / rect.width);
    const y = (e.clientY - rect.top) * (canvas.height / rect.height);

    for (const text of segment.textSegments) {
      const ctx = canvas.getContext('2d');
      if (!ctx) return null;
      const hitArea = this.getTextHitArea(ctx, text, canvas.width, canvas.height);
      if (x >= hitArea.x && x <= hitArea.x + hitArea.width &&
        y >= hitArea.y && y <= hitArea.y + hitArea.height) {
        this.isDraggingText = true;
        this.draggedTextId = text.id;
        this.dragOffset.x = x - (text.style.x / 100 * canvas.width);
        this.dragOffset.y = y - (text.style.y / 100 * canvas.height);
        return text.id;
      }
    }
    return null;
  }

  public handleMouseMove(
    e: MouseEvent,
    _segment: VideoSegment,
    canvas: HTMLCanvasElement,
    onTextMove: (id: string, x: number, y: number) => void
  ) {
    if (!this.isDraggingText || !this.draggedTextId) return;

    const rect = canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left) * (canvas.width / rect.width);
    const y = (e.clientY - rect.top) * (canvas.height / rect.height);

    const newX = Math.max(0, Math.min(100, ((x - this.dragOffset.x) / canvas.width) * 100));
    const newY = Math.max(0, Math.min(100, ((y - this.dragOffset.y) / canvas.height) * 100));

    onTextMove(this.draggedTextId, newX, newY);
  }

  public handleMouseUp() {
    this.isDraggingText = false;
    this.draggedTextId = null;
  }

  /**
   * Pre-render each text overlay to an RGBA bitmap at the given output resolution.
   * Rust just alpha-composites these per frame with fade applied — no dual pipeline.
   */
  public bakeTextOverlays(
    segment: VideoSegment,
    outputWidth: number,
    outputHeight: number
  ): BakedTextOverlay[] {
    const result: BakedTextOverlay[] = [];
    if (!segment.textSegments?.length) return result;

    const shadowPad = 24; // extra padding for drop shadow

    for (const textSeg of segment.textSegments) {
      // Render to full-size offscreen canvas (drawTextOverlay needs full dims for % positioning)
      // Must be in DOM so CSS font-variation-settings on the element takes effect.
      const offscreen = document.createElement('canvas');
      offscreen.width = outputWidth;
      offscreen.height = outputHeight;
      offscreen.style.cssText = 'position:fixed;left:-9999px;top:-9999px;pointer-events:none;';
      document.body.appendChild(offscreen);
      const ctx = offscreen.getContext('2d')!;

      // Draw at full opacity (fadeAlpha=1); opacity is baked into pixel alpha
      this.drawTextOverlay(ctx, textSeg, outputWidth, outputHeight, 1.0);

      // Compute tight bounds via getTextHitArea
      const hitArea = this.getTextHitArea(ctx, textSeg, outputWidth, outputHeight);

      // Crop region (hit area + shadow padding, clamped to canvas)
      const cropX = Math.max(0, Math.floor(hitArea.x - shadowPad));
      const cropY = Math.max(0, Math.floor(hitArea.y - shadowPad));
      const cropRight = Math.min(outputWidth, Math.ceil(hitArea.x + hitArea.width + shadowPad));
      const cropBottom = Math.min(outputHeight, Math.ceil(hitArea.y + hitArea.height + shadowPad));
      const cropW = cropRight - cropX;
      const cropH = cropBottom - cropY;

      if (cropW <= 0 || cropH <= 0) {
        offscreen.remove();
        continue;
      }

      const imageData = ctx.getImageData(cropX, cropY, cropW, cropH);
      offscreen.remove();
      result.push({
        startTime: textSeg.startTime,
        endTime: textSeg.endTime,
        x: cropX,
        y: cropY,
        width: cropW,
        height: cropH,
        data: Array.from(imageData.data)
      });
    }

    return result;
  }
}

export const videoRenderer = new VideoRenderer();