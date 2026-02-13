import { BackgroundConfig, MousePosition, VideoSegment, ZoomKeyframe, TextSegment, BakedCameraFrame, BakedCursorFrame, BakedTextOverlay } from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';
import { getTrimSegments, sourceRangeToCompactRanges, toCompactTime } from '@/lib/trimSegments';

// --- CONFIGURATION ---
// Default pointer movement delay (seconds)
const DEFAULT_CURSOR_OFFSET_SEC = 0.03;
const DEFAULT_CURSOR_WIGGLE_STRENGTH = 0.30;
const DEFAULT_CURSOR_WIGGLE_DAMPING = 0.55;
const DEFAULT_CURSOR_WIGGLE_RESPONSE = 6.5;
const CURSOR_ASSET_VERSION = `cursor-types-runtime-${Date.now()}`;

type CursorRenderType =
  | 'default-screenstudio'
  | 'text-screenstudio'
  | 'pointer-screenstudio'
  | 'openhand-screenstudio'
  | 'closehand-screenstudio'
  | 'wait-screenstudio'
  | 'appstarting-screenstudio'
  | 'crosshair-screenstudio'
  | 'resize-ns-screenstudio'
  | 'resize-we-screenstudio'
  | 'resize-nwse-screenstudio'
  | 'resize-nesw-screenstudio'
  | 'default-macos26'
  | 'text-macos26'
  | 'pointer-macos26'
  | 'openhand-macos26'
  | 'closehand-macos26'
  | 'wait-macos26'
  | 'appstarting-macos26'
  | 'crosshair-macos26'
  | 'resize-ns-macos26'
  | 'resize-we-macos26'
  | 'resize-nwse-macos26'
  | 'resize-nesw-macos26'
  | 'default-sgtcute'
  | 'text-sgtcute'
  | 'pointer-sgtcute'
  | 'openhand-sgtcute'
  | 'closehand-sgtcute'
  | 'wait-sgtcute'
  | 'appstarting-sgtcute'
  | 'crosshair-sgtcute'
  | 'resize-ns-sgtcute'
  | 'resize-we-sgtcute'
  | 'resize-nwse-sgtcute'
  | 'resize-nesw-sgtcute'
  | 'default-sgtcool'
  | 'text-sgtcool'
  | 'pointer-sgtcool'
  | 'openhand-sgtcool'
  | 'closehand-sgtcool'
  | 'wait-sgtcool'
  | 'appstarting-sgtcool'
  | 'crosshair-sgtcool'
  | 'resize-ns-sgtcool'
  | 'resize-we-sgtcool'
  | 'resize-nwse-sgtcool'
  | 'resize-nesw-sgtcool'
  | 'default-sgtai'
  | 'text-sgtai'
  | 'pointer-sgtai'
  | 'openhand-sgtai'
  | 'closehand-sgtai'
  | 'wait-sgtai'
  | 'appstarting-sgtai'
  | 'crosshair-sgtai'
  | 'resize-ns-sgtai'
  | 'resize-we-sgtai'
  | 'resize-nwse-sgtai'
  | 'resize-nesw-sgtai'
  | 'default-sgtpixel'
  | 'text-sgtpixel'
  | 'pointer-sgtpixel'
  | 'openhand-sgtpixel'
  | 'closehand-sgtpixel'
  | 'wait-sgtpixel'
  | 'appstarting-sgtpixel'
  | 'crosshair-sgtpixel'
  | 'resize-ns-sgtpixel'
  | 'resize-we-sgtpixel'
  | 'resize-nwse-sgtpixel'
  | 'resize-nesw-sgtpixel'
  | 'default-jepriwin11'
  | 'text-jepriwin11'
  | 'pointer-jepriwin11'
  | 'openhand-jepriwin11'
  | 'closehand-jepriwin11'
  | 'wait-jepriwin11'
  | 'appstarting-jepriwin11'
  | 'crosshair-jepriwin11'
  | 'resize-ns-jepriwin11'
  | 'resize-we-jepriwin11'
  | 'resize-nwse-jepriwin11'
  | 'resize-nesw-jepriwin11';

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
  private pointerScreenStudioImage: HTMLImageElement;
  private defaultScreenStudioImage: HTMLImageElement;
  private textScreenStudioImage: HTMLImageElement;
  private openHandScreenStudioImage: HTMLImageElement;
  private closeHandScreenStudioImage: HTMLImageElement;
  private waitScreenStudioImage: HTMLImageElement;
  private appStartingScreenStudioImage: HTMLImageElement;
  private crosshairScreenStudioImage: HTMLImageElement;
  private resizeNsScreenStudioImage: HTMLImageElement;
  private resizeWeScreenStudioImage: HTMLImageElement;
  private resizeNwseScreenStudioImage: HTMLImageElement;
  private resizeNeswScreenStudioImage: HTMLImageElement;
  private defaultMacos26Image: HTMLImageElement;
  private textMacos26Image: HTMLImageElement;
  private pointerMacos26Image: HTMLImageElement;
  private openHandMacos26Image: HTMLImageElement;
  private closeHandMacos26Image: HTMLImageElement;
  private waitMacos26Image: HTMLImageElement;
  private appStartingMacos26Image: HTMLImageElement;
  private crosshairMacos26Image: HTMLImageElement;
  private resizeNsMacos26Image: HTMLImageElement;
  private resizeWeMacos26Image: HTMLImageElement;
  private resizeNwseMacos26Image: HTMLImageElement;
  private resizeNeswMacos26Image: HTMLImageElement;
  private defaultSgtcuteImage: HTMLImageElement;
  private textSgtcuteImage: HTMLImageElement;
  private pointerSgtcuteImage: HTMLImageElement;
  private openHandSgtcuteImage: HTMLImageElement;
  private closeHandSgtcuteImage: HTMLImageElement;
  private waitSgtcuteImage: HTMLImageElement;
  private appStartingSgtcuteImage: HTMLImageElement;
  private crosshairSgtcuteImage: HTMLImageElement;
  private resizeNsSgtcuteImage: HTMLImageElement;
  private resizeWeSgtcuteImage: HTMLImageElement;
  private resizeNwseSgtcuteImage: HTMLImageElement;
  private resizeNeswSgtcuteImage: HTMLImageElement;
  private defaultSgtcoolImage: HTMLImageElement;
  private textSgtcoolImage: HTMLImageElement;
  private pointerSgtcoolImage: HTMLImageElement;
  private openHandSgtcoolImage: HTMLImageElement;
  private closeHandSgtcoolImage: HTMLImageElement;
  private waitSgtcoolImage: HTMLImageElement;
  private appStartingSgtcoolImage: HTMLImageElement;
  private crosshairSgtcoolImage: HTMLImageElement;
  private resizeNsSgtcoolImage: HTMLImageElement;
  private resizeWeSgtcoolImage: HTMLImageElement;
  private resizeNwseSgtcoolImage: HTMLImageElement;
  private resizeNeswSgtcoolImage: HTMLImageElement;
  private defaultSgtaiImage: HTMLImageElement;
  private textSgtaiImage: HTMLImageElement;
  private pointerSgtaiImage: HTMLImageElement;
  private openHandSgtaiImage: HTMLImageElement;
  private closeHandSgtaiImage: HTMLImageElement;
  private waitSgtaiImage: HTMLImageElement;
  private appStartingSgtaiImage: HTMLImageElement;
  private crosshairSgtaiImage: HTMLImageElement;
  private resizeNsSgtaiImage: HTMLImageElement;
  private resizeWeSgtaiImage: HTMLImageElement;
  private resizeNwseSgtaiImage: HTMLImageElement;
  private resizeNeswSgtaiImage: HTMLImageElement;
  private defaultSgtpixelImage: HTMLImageElement;
  private textSgtpixelImage: HTMLImageElement;
  private pointerSgtpixelImage: HTMLImageElement;
  private openHandSgtpixelImage: HTMLImageElement;
  private closeHandSgtpixelImage: HTMLImageElement;
  private waitSgtpixelImage: HTMLImageElement;
  private appStartingSgtpixelImage: HTMLImageElement;
  private crosshairSgtpixelImage: HTMLImageElement;
  private resizeNsSgtpixelImage: HTMLImageElement;
  private resizeWeSgtpixelImage: HTMLImageElement;
  private resizeNwseSgtpixelImage: HTMLImageElement;
  private resizeNeswSgtpixelImage: HTMLImageElement;
  private defaultJepriwin11Image: HTMLImageElement;
  private textJepriwin11Image: HTMLImageElement;
  private pointerJepriwin11Image: HTMLImageElement;
  private openHandJepriwin11Image: HTMLImageElement;
  private closeHandJepriwin11Image: HTMLImageElement;
  private waitJepriwin11Image: HTMLImageElement;
  private appStartingJepriwin11Image: HTMLImageElement;
  private crosshairJepriwin11Image: HTMLImageElement;
  private resizeNsJepriwin11Image: HTMLImageElement;
  private resizeWeJepriwin11Image: HTMLImageElement;
  private resizeNwseJepriwin11Image: HTMLImageElement;
  private resizeNeswJepriwin11Image: HTMLImageElement;
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

  private processedCursorPositions: MousePosition[] | null = null;
  private lastMousePositionsRef: MousePosition[] | null = null;
  private lastCursorProcessSignature: string = '';
  private cachedBakedPath: BakedCameraFrame[] | null = null;
  private lastBakeSignature: string = '';
  private lastBakeSegment: VideoSegment | null = null;
  private lastBakeViewW: number = 0;
  private lastBakeViewH: number = 0;
  private loggedCursorTypes: Set<string> = new Set();
  private loggedCursorMappings: Set<string> = new Set();

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
  // Motion blur canvases (reused across frames)
  private blurAccumCanvas: OffscreenCanvas | null = null;
  private blurAccumCtx: OffscreenCanvasRenderingContext2D | null = null;
  private blurSubCanvas: OffscreenCanvas | null = null;
  private blurSubCtx: OffscreenCanvasRenderingContext2D | null = null;

  constructor() {
    this.defaultScreenStudioImage = new Image();
    this.defaultScreenStudioImage.src = `/cursor-default-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultScreenStudioImage.onload = () => { };

    this.textScreenStudioImage = new Image();
    this.textScreenStudioImage.src = `/cursor-text-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textScreenStudioImage.onload = () => { };

    this.pointerScreenStudioImage = new Image();
    this.pointerScreenStudioImage.src = `/cursor-pointer-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerScreenStudioImage.onload = () => { };

    this.openHandScreenStudioImage = new Image();
    this.openHandScreenStudioImage.src = `/cursor-openhand-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandScreenStudioImage.onload = () => { };

    this.closeHandScreenStudioImage = new Image();
    this.closeHandScreenStudioImage.src = `/cursor-closehand-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandScreenStudioImage.onload = () => { };

    this.waitScreenStudioImage = new Image();
    this.waitScreenStudioImage.src = `/cursor-wait-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitScreenStudioImage.onload = () => { };

    this.appStartingScreenStudioImage = new Image();
    this.appStartingScreenStudioImage.src = `/cursor-appstarting-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingScreenStudioImage.onload = () => { };

    this.crosshairScreenStudioImage = new Image();
    this.crosshairScreenStudioImage.src = `/cursor-crosshair-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairScreenStudioImage.onload = () => { };

    this.resizeNsScreenStudioImage = new Image();
    this.resizeNsScreenStudioImage.src = `/cursor-resize-ns-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsScreenStudioImage.onload = () => { };

    this.resizeWeScreenStudioImage = new Image();
    this.resizeWeScreenStudioImage.src = `/cursor-resize-we-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeScreenStudioImage.onload = () => { };

    this.resizeNwseScreenStudioImage = new Image();
    this.resizeNwseScreenStudioImage.src = `/cursor-resize-nwse-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseScreenStudioImage.onload = () => { };

    this.resizeNeswScreenStudioImage = new Image();
    this.resizeNeswScreenStudioImage.src = `/cursor-resize-nesw-screenstudio.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswScreenStudioImage.onload = () => { };

    this.defaultMacos26Image = new Image();
    this.defaultMacos26Image.src = `/cursor-default-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultMacos26Image.onload = () => { };

    this.textMacos26Image = new Image();
    this.textMacos26Image.src = `/cursor-text-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textMacos26Image.onload = () => { };

    this.pointerMacos26Image = new Image();
    this.pointerMacos26Image.src = `/cursor-pointer-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerMacos26Image.onload = () => { };

    this.openHandMacos26Image = new Image();
    this.openHandMacos26Image.src = `/cursor-openhand-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandMacos26Image.onload = () => { };

    this.closeHandMacos26Image = new Image();
    this.closeHandMacos26Image.src = `/cursor-closehand-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandMacos26Image.onload = () => { };

    this.waitMacos26Image = new Image();
    this.waitMacos26Image.src = `/cursor-wait-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitMacos26Image.onload = () => { };

    this.appStartingMacos26Image = new Image();
    this.appStartingMacos26Image.src = `/cursor-appstarting-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingMacos26Image.onload = () => { };

    this.crosshairMacos26Image = new Image();
    this.crosshairMacos26Image.src = `/cursor-crosshair-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairMacos26Image.onload = () => { };

    this.resizeNsMacos26Image = new Image();
    this.resizeNsMacos26Image.src = `/cursor-resize-ns-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsMacos26Image.onload = () => { };

    this.resizeWeMacos26Image = new Image();
    this.resizeWeMacos26Image.src = `/cursor-resize-we-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeMacos26Image.onload = () => { };

    this.resizeNwseMacos26Image = new Image();
    this.resizeNwseMacos26Image.src = `/cursor-resize-nwse-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseMacos26Image.onload = () => { };

    this.resizeNeswMacos26Image = new Image();
    this.resizeNeswMacos26Image.src = `/cursor-resize-nesw-macos26.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswMacos26Image.onload = () => { };

    this.defaultSgtcuteImage = new Image();
    this.defaultSgtcuteImage.src = `/cursor-default-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultSgtcuteImage.onload = () => { };

    this.textSgtcuteImage = new Image();
    this.textSgtcuteImage.src = `/cursor-text-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textSgtcuteImage.onload = () => { };

    this.pointerSgtcuteImage = new Image();
    this.pointerSgtcuteImage.src = `/cursor-pointer-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerSgtcuteImage.onload = () => { };

    this.openHandSgtcuteImage = new Image();
    this.openHandSgtcuteImage.src = `/cursor-openhand-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandSgtcuteImage.onload = () => { };

    this.closeHandSgtcuteImage = new Image();
    this.closeHandSgtcuteImage.src = `/cursor-closehand-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandSgtcuteImage.onload = () => { };

    this.waitSgtcuteImage = new Image();
    this.waitSgtcuteImage.src = `/cursor-wait-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitSgtcuteImage.onload = () => { };

    this.appStartingSgtcuteImage = new Image();
    this.appStartingSgtcuteImage.src = `/cursor-appstarting-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingSgtcuteImage.onload = () => { };

    this.crosshairSgtcuteImage = new Image();
    this.crosshairSgtcuteImage.src = `/cursor-crosshair-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairSgtcuteImage.onload = () => { };

    this.resizeNsSgtcuteImage = new Image();
    this.resizeNsSgtcuteImage.src = `/cursor-resize-ns-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsSgtcuteImage.onload = () => { };

    this.resizeWeSgtcuteImage = new Image();
    this.resizeWeSgtcuteImage.src = `/cursor-resize-we-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeSgtcuteImage.onload = () => { };

    this.resizeNwseSgtcuteImage = new Image();
    this.resizeNwseSgtcuteImage.src = `/cursor-resize-nwse-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseSgtcuteImage.onload = () => { };

    this.resizeNeswSgtcuteImage = new Image();
    this.resizeNeswSgtcuteImage.src = `/cursor-resize-nesw-sgtcute.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswSgtcuteImage.onload = () => { };

    this.defaultSgtcoolImage = new Image();
    this.defaultSgtcoolImage.src = `/cursor-default-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultSgtcoolImage.onload = () => { };

    this.textSgtcoolImage = new Image();
    this.textSgtcoolImage.src = `/cursor-text-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textSgtcoolImage.onload = () => { };

    this.pointerSgtcoolImage = new Image();
    this.pointerSgtcoolImage.src = `/cursor-pointer-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerSgtcoolImage.onload = () => { };

    this.openHandSgtcoolImage = new Image();
    this.openHandSgtcoolImage.src = `/cursor-openhand-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandSgtcoolImage.onload = () => { };

    this.closeHandSgtcoolImage = new Image();
    this.closeHandSgtcoolImage.src = `/cursor-closehand-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandSgtcoolImage.onload = () => { };

    this.waitSgtcoolImage = new Image();
    this.waitSgtcoolImage.src = `/cursor-wait-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitSgtcoolImage.onload = () => { };

    this.appStartingSgtcoolImage = new Image();
    this.appStartingSgtcoolImage.src = `/cursor-appstarting-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingSgtcoolImage.onload = () => { };

    this.crosshairSgtcoolImage = new Image();
    this.crosshairSgtcoolImage.src = `/cursor-crosshair-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairSgtcoolImage.onload = () => { };

    this.resizeNsSgtcoolImage = new Image();
    this.resizeNsSgtcoolImage.src = `/cursor-resize-ns-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsSgtcoolImage.onload = () => { };

    this.resizeWeSgtcoolImage = new Image();
    this.resizeWeSgtcoolImage.src = `/cursor-resize-we-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeSgtcoolImage.onload = () => { };

    this.resizeNwseSgtcoolImage = new Image();
    this.resizeNwseSgtcoolImage.src = `/cursor-resize-nwse-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseSgtcoolImage.onload = () => { };

    this.resizeNeswSgtcoolImage = new Image();
    this.resizeNeswSgtcoolImage.src = `/cursor-resize-nesw-sgtcool.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswSgtcoolImage.onload = () => { };

    this.defaultSgtaiImage = new Image();
    this.defaultSgtaiImage.src = `/cursor-default-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultSgtaiImage.onload = () => { };

    this.textSgtaiImage = new Image();
    this.textSgtaiImage.src = `/cursor-text-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textSgtaiImage.onload = () => { };

    this.pointerSgtaiImage = new Image();
    this.pointerSgtaiImage.src = `/cursor-pointer-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerSgtaiImage.onload = () => { };

    this.openHandSgtaiImage = new Image();
    this.openHandSgtaiImage.src = `/cursor-openhand-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandSgtaiImage.onload = () => { };

    this.closeHandSgtaiImage = new Image();
    this.closeHandSgtaiImage.src = `/cursor-closehand-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandSgtaiImage.onload = () => { };

    this.waitSgtaiImage = new Image();
    this.waitSgtaiImage.src = `/cursor-wait-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitSgtaiImage.onload = () => { };

    this.appStartingSgtaiImage = new Image();
    this.appStartingSgtaiImage.src = `/cursor-appstarting-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingSgtaiImage.onload = () => { };

    this.crosshairSgtaiImage = new Image();
    this.crosshairSgtaiImage.src = `/cursor-crosshair-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairSgtaiImage.onload = () => { };

    this.resizeNsSgtaiImage = new Image();
    this.resizeNsSgtaiImage.src = `/cursor-resize-ns-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsSgtaiImage.onload = () => { };

    this.resizeWeSgtaiImage = new Image();
    this.resizeWeSgtaiImage.src = `/cursor-resize-we-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeSgtaiImage.onload = () => { };

    this.resizeNwseSgtaiImage = new Image();
    this.resizeNwseSgtaiImage.src = `/cursor-resize-nwse-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseSgtaiImage.onload = () => { };

    this.resizeNeswSgtaiImage = new Image();
    this.resizeNeswSgtaiImage.src = `/cursor-resize-nesw-sgtai.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswSgtaiImage.onload = () => { };

    this.defaultSgtpixelImage = new Image();
    this.defaultSgtpixelImage.src = `/cursor-default-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultSgtpixelImage.onload = () => { };

    this.textSgtpixelImage = new Image();
    this.textSgtpixelImage.src = `/cursor-text-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textSgtpixelImage.onload = () => { };

    this.pointerSgtpixelImage = new Image();
    this.pointerSgtpixelImage.src = `/cursor-pointer-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerSgtpixelImage.onload = () => { };

    this.openHandSgtpixelImage = new Image();
    this.openHandSgtpixelImage.src = `/cursor-openhand-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandSgtpixelImage.onload = () => { };

    this.closeHandSgtpixelImage = new Image();
    this.closeHandSgtpixelImage.src = `/cursor-closehand-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandSgtpixelImage.onload = () => { };

    this.waitSgtpixelImage = new Image();
    this.waitSgtpixelImage.src = `/cursor-wait-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitSgtpixelImage.onload = () => { };

    this.appStartingSgtpixelImage = new Image();
    this.appStartingSgtpixelImage.src = `/cursor-appstarting-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingSgtpixelImage.onload = () => { };

    this.crosshairSgtpixelImage = new Image();
    this.crosshairSgtpixelImage.src = `/cursor-crosshair-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairSgtpixelImage.onload = () => { };

    this.resizeNsSgtpixelImage = new Image();
    this.resizeNsSgtpixelImage.src = `/cursor-resize-ns-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsSgtpixelImage.onload = () => { };

    this.resizeWeSgtpixelImage = new Image();
    this.resizeWeSgtpixelImage.src = `/cursor-resize-we-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeSgtpixelImage.onload = () => { };

    this.resizeNwseSgtpixelImage = new Image();
    this.resizeNwseSgtpixelImage.src = `/cursor-resize-nwse-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseSgtpixelImage.onload = () => { };

    this.resizeNeswSgtpixelImage = new Image();
    this.resizeNeswSgtpixelImage.src = `/cursor-resize-nesw-sgtpixel.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswSgtpixelImage.onload = () => { };

    this.defaultJepriwin11Image = new Image();
    this.defaultJepriwin11Image.src = `/cursor-default-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.defaultJepriwin11Image.onload = () => { };

    this.textJepriwin11Image = new Image();
    this.textJepriwin11Image.src = `/cursor-text-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.textJepriwin11Image.onload = () => { };

    this.pointerJepriwin11Image = new Image();
    this.pointerJepriwin11Image.src = `/cursor-pointer-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.pointerJepriwin11Image.onload = () => { };

    this.openHandJepriwin11Image = new Image();
    this.openHandJepriwin11Image.src = `/cursor-openhand-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.openHandJepriwin11Image.onload = () => { };

    this.closeHandJepriwin11Image = new Image();
    this.closeHandJepriwin11Image.src = `/cursor-closehand-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.closeHandJepriwin11Image.onload = () => { };

    this.waitJepriwin11Image = new Image();
    this.waitJepriwin11Image.src = `/cursor-wait-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.waitJepriwin11Image.onload = () => { };

    this.appStartingJepriwin11Image = new Image();
    this.appStartingJepriwin11Image.src = `/cursor-appstarting-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.appStartingJepriwin11Image.onload = () => { };

    this.crosshairJepriwin11Image = new Image();
    this.crosshairJepriwin11Image.src = `/cursor-crosshair-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.crosshairJepriwin11Image.onload = () => { };

    this.resizeNsJepriwin11Image = new Image();
    this.resizeNsJepriwin11Image.src = `/cursor-resize-ns-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNsJepriwin11Image.onload = () => { };

    this.resizeWeJepriwin11Image = new Image();
    this.resizeWeJepriwin11Image.src = `/cursor-resize-we-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeWeJepriwin11Image.onload = () => { };

    this.resizeNwseJepriwin11Image = new Image();
    this.resizeNwseJepriwin11Image.src = `/cursor-resize-nwse-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNwseJepriwin11Image.onload = () => { };

    this.resizeNeswJepriwin11Image = new Image();
    this.resizeNeswJepriwin11Image.src = `/cursor-resize-nesw-jepriwin11.svg?v=${CURSOR_ASSET_VERSION}`;
    this.resizeNeswJepriwin11Image.onload = () => { };

    this.cursorOffscreen = new OffscreenCanvas(128, 128);
    this.cursorOffscreenCtx = this.cursorOffscreen.getContext('2d')!;
  }

  private activeRenderContext: RenderContext | null = null;

  private getCursorMovementDelaySec(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorMovementDelay;
    if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_OFFSET_SEC;
    return Math.max(0, Math.min(0.5, raw));
  }

  private getCursorSmoothness(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorSmoothness;
    if (raw === undefined || Number.isNaN(raw)) return 5;
    return Math.max(0, Math.min(10, raw));
  }

  private getCursorShadowStrength(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorShadow;
    if (raw === undefined || Number.isNaN(raw)) return 35;
    return Math.max(0, Math.min(200, raw));
  }

  private getCursorWiggleStrength(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorWiggleStrength;
    if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_STRENGTH;
    return Math.max(0, Math.min(1, raw));
  }

  private getCursorWiggleDamping(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorWiggleDamping;
    if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_DAMPING;
    return Math.max(0.35, Math.min(0.98, raw));
  }

  private getCursorWiggleResponse(backgroundConfig?: BackgroundConfig | null): number {
    const raw = backgroundConfig?.cursorWiggleResponse;
    if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_RESPONSE;
    return Math.max(2, Math.min(12, raw));
  }

  private getCursorProcessingSignature(backgroundConfig?: BackgroundConfig | null): string {
    return [
      this.getCursorSmoothness(backgroundConfig).toFixed(2),
      this.getCursorWiggleStrength(backgroundConfig).toFixed(2),
      this.getCursorWiggleDamping(backgroundConfig).toFixed(2),
      this.getCursorWiggleResponse(backgroundConfig).toFixed(2),
      this.getCursorTiltAngleRad(backgroundConfig).toFixed(4),
    ].join('|');
  }

  private getCursorTiltAngleRad(backgroundConfig?: BackgroundConfig | null): number {
    return (backgroundConfig?.cursorTiltAngle ?? -10) * (Math.PI / 180);
  }

  private getCursorPack(backgroundConfig?: BackgroundConfig | null): 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11' {
    if (backgroundConfig?.cursorPack === 'jepriwin11') return 'jepriwin11';
    if (backgroundConfig?.cursorPack === 'sgtpixel') return 'sgtpixel';
    if (backgroundConfig?.cursorPack === 'sgtai') return 'sgtai';
    if (backgroundConfig?.cursorPack === 'sgtcool') return 'sgtcool';
    if (backgroundConfig?.cursorPack === 'sgtcute') return 'sgtcute';
    if (backgroundConfig?.cursorPack === 'macos26') return 'macos26';
    if (backgroundConfig?.cursorPack === 'screenstudio') return 'screenstudio';
    if (backgroundConfig?.cursorDefaultVariant === 'jepriwin11'
      || backgroundConfig?.cursorTextVariant === 'jepriwin11'
      || backgroundConfig?.cursorPointerVariant === 'jepriwin11'
      || backgroundConfig?.cursorOpenHandVariant === 'jepriwin11') {
      return 'jepriwin11';
    }
    if (backgroundConfig?.cursorDefaultVariant === 'sgtpixel'
      || backgroundConfig?.cursorTextVariant === 'sgtpixel'
      || backgroundConfig?.cursorPointerVariant === 'sgtpixel'
      || backgroundConfig?.cursorOpenHandVariant === 'sgtpixel') {
      return 'sgtpixel';
    }
    if (backgroundConfig?.cursorDefaultVariant === 'sgtai'
      || backgroundConfig?.cursorTextVariant === 'sgtai'
      || backgroundConfig?.cursorPointerVariant === 'sgtai'
      || backgroundConfig?.cursorOpenHandVariant === 'sgtai') {
      return 'sgtai';
    }
    if (backgroundConfig?.cursorDefaultVariant === 'sgtcool'
      || backgroundConfig?.cursorTextVariant === 'sgtcool'
      || backgroundConfig?.cursorPointerVariant === 'sgtcool'
      || backgroundConfig?.cursorOpenHandVariant === 'sgtcool') {
      return 'sgtcool';
    }
    if (backgroundConfig?.cursorDefaultVariant === 'sgtcute'
      || backgroundConfig?.cursorTextVariant === 'sgtcute'
      || backgroundConfig?.cursorPointerVariant === 'sgtcute'
      || backgroundConfig?.cursorOpenHandVariant === 'sgtcute') {
      return 'sgtcute';
    }
    if (backgroundConfig?.cursorDefaultVariant === 'macos26'
      || backgroundConfig?.cursorTextVariant === 'macos26'
      || backgroundConfig?.cursorPointerVariant === 'macos26'
      || backgroundConfig?.cursorOpenHandVariant === 'macos26') {
      return 'macos26';
    }
    return 'screenstudio';
  }

  private resolveCursorRenderType(rawType: string, backgroundConfig?: BackgroundConfig | null, isClicked: boolean = false): CursorRenderType {
    const lower = (rawType || 'default').toLowerCase();
    const pack = this.getCursorPack(backgroundConfig);

    const semanticType =
      (lower === 'text' || lower === 'ibeam') ? 'text'
        : (lower === 'pointer' || lower === 'hand') ? 'pointer'
          : (lower === 'wait') ? 'wait'
            : (lower === 'appstarting') ? 'appstarting'
              : (lower === 'crosshair' || lower === 'cross') ? 'crosshair'
                : (lower === 'resize_ns' || lower === 'sizens') ? 'resize_ns'
                  : (lower === 'resize_we' || lower === 'sizewe') ? 'resize_we'
                    : (lower === 'resize_nwse' || lower === 'sizenwse') ? 'resize_nwse'
                      : (lower === 'resize_nesw' || lower === 'sizenesw') ? 'resize_nesw'
                        : (
                          lower === 'move' ||
                          lower === 'sizeall' ||
                          lower === 'drag' ||
                          lower === 'dragging' ||
                          lower === 'openhand' ||
                          lower === 'open-hand' ||
                          lower === 'open_hand' ||
                          lower === 'closedhand' ||
                          lower === 'closed-hand' ||
                          lower === 'closed_hand' ||
                          lower === 'closehand' ||
                          lower === 'close-hand' ||
                          lower === 'close_hand' ||
                          lower === 'grab' ||
                          lower === 'grabbing'
                        )
                          ? (isClicked ? 'closehand' : 'openhand')
                          : (lower === 'other') ? 'default'
                            : (lower === 'default' || lower === 'arrow') ? 'default'
                            : 'default';

    if (pack === 'macos26') {
      switch (semanticType) {
        case 'text': return 'text-macos26';
        case 'pointer': return 'pointer-macos26';
        case 'openhand': return 'openhand-macos26';
        case 'closehand': return 'closehand-macos26';
        case 'wait': return 'wait-macos26';
        case 'appstarting': return 'appstarting-macos26';
        case 'crosshair': return 'crosshair-macos26';
        case 'resize_ns': return 'resize-ns-macos26';
        case 'resize_we': return 'resize-we-macos26';
        case 'resize_nwse': return 'resize-nwse-macos26';
        case 'resize_nesw': return 'resize-nesw-macos26';
        default: return 'default-macos26';
      }
    }

    if (pack === 'sgtcool') {
      switch (semanticType) {
        case 'text': return 'text-sgtcool';
        case 'pointer': return 'pointer-sgtcool';
        case 'openhand': return 'openhand-sgtcool';
        case 'closehand': return 'closehand-sgtcool';
        case 'wait': return 'wait-sgtcool';
        case 'appstarting': return 'appstarting-sgtcool';
        case 'crosshair': return 'crosshair-sgtcool';
        case 'resize_ns': return 'resize-ns-sgtcool';
        case 'resize_we': return 'resize-we-sgtcool';
        case 'resize_nwse': return 'resize-nwse-sgtcool';
        case 'resize_nesw': return 'resize-nesw-sgtcool';
        default: return 'default-sgtcool';
      }
    }

    if (pack === 'sgtai') {
      switch (semanticType) {
        case 'text': return 'text-sgtai';
        case 'pointer': return 'pointer-sgtai';
        case 'openhand': return 'openhand-sgtai';
        case 'closehand': return 'closehand-sgtai';
        case 'wait': return 'wait-sgtai';
        case 'appstarting': return 'appstarting-sgtai';
        case 'crosshair': return 'crosshair-sgtai';
        case 'resize_ns': return 'resize-ns-sgtai';
        case 'resize_we': return 'resize-we-sgtai';
        case 'resize_nwse': return 'resize-nwse-sgtai';
        case 'resize_nesw': return 'resize-nesw-sgtai';
        default: return 'default-sgtai';
      }
    }

    if (pack === 'sgtpixel') {
      switch (semanticType) {
        case 'text': return 'text-sgtpixel';
        case 'pointer': return 'pointer-sgtpixel';
        case 'openhand': return 'openhand-sgtpixel';
        case 'closehand': return 'closehand-sgtpixel';
        case 'wait': return 'wait-sgtpixel';
        case 'appstarting': return 'appstarting-sgtpixel';
        case 'crosshair': return 'crosshair-sgtpixel';
        case 'resize_ns': return 'resize-ns-sgtpixel';
        case 'resize_we': return 'resize-we-sgtpixel';
        case 'resize_nwse': return 'resize-nwse-sgtpixel';
        case 'resize_nesw': return 'resize-nesw-sgtpixel';
        default: return 'default-sgtpixel';
      }
    }

    if (pack === 'jepriwin11') {
      switch (semanticType) {
        case 'text': return 'text-jepriwin11';
        case 'pointer': return 'pointer-jepriwin11';
        case 'openhand': return 'openhand-jepriwin11';
        case 'closehand': return 'closehand-jepriwin11';
        case 'wait': return 'wait-jepriwin11';
        case 'appstarting': return 'appstarting-jepriwin11';
        case 'crosshair': return 'crosshair-jepriwin11';
        case 'resize_ns': return 'resize-ns-jepriwin11';
        case 'resize_we': return 'resize-we-jepriwin11';
        case 'resize_nwse': return 'resize-nwse-jepriwin11';
        case 'resize_nesw': return 'resize-nesw-jepriwin11';
        default: return 'default-jepriwin11';
      }
    }

    if (pack === 'sgtcute') {
      switch (semanticType) {
        case 'text': return 'text-sgtcute';
        case 'pointer': return 'pointer-sgtcute';
        case 'openhand': return 'openhand-sgtcute';
        case 'closehand': return 'closehand-sgtcute';
        case 'wait': return 'wait-sgtcute';
        case 'appstarting': return 'appstarting-sgtcute';
        case 'crosshair': return 'crosshair-sgtcute';
        case 'resize_ns': return 'resize-ns-sgtcute';
        case 'resize_we': return 'resize-we-sgtcute';
        case 'resize_nwse': return 'resize-nwse-sgtcute';
        case 'resize_nesw': return 'resize-nesw-sgtcute';
        default: return 'default-sgtcute';
      }
    }

    switch (semanticType) {
      case 'text': return 'text-screenstudio';
      case 'pointer': return 'pointer-screenstudio';
      case 'openhand': return 'openhand-screenstudio';
      case 'closehand': return 'closehand-screenstudio';
      case 'wait': return 'wait-screenstudio';
      case 'appstarting': return 'appstarting-screenstudio';
      case 'crosshair': return 'crosshair-screenstudio';
      case 'resize_ns': return 'resize-ns-screenstudio';
      case 'resize_we': return 'resize-we-screenstudio';
      case 'resize_nwse': return 'resize-nwse-screenstudio';
      case 'resize_nesw': return 'resize-nesw-screenstudio';
      default: return 'default-screenstudio';
    }
  }

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
    backgroundConfig?: BackgroundConfig,
    fps: number = 60
  ): BakedCursorFrame[] {
    const baked: BakedCursorFrame[] = [];
    const step = 1 / fps;
    const duration = Math.max(segment.trimEnd, ...(segment.trimSegments || []).map(s => s.endTime));
    const trimSegments = getTrimSegments(segment, duration);

    const processed = this.processCursorPositions(mousePositions, backgroundConfig);

    let simSquishScale = 1.0;
    let simLastHoldTime = -1;
    const simRatio = 2.0;

    const cursorOffsetSec = this.getCursorMovementDelaySec(backgroundConfig);

    // Bake the full source-time range (including hidden gaps between trim segments)
    // so cursor state evolves naturally through cuts — no jarring jumps at bridges.
    const fullStart = trimSegments[0].startTime;
    const fullEnd = trimSegments[trimSegments.length - 1].endTime;

    for (let t = fullStart; t <= fullEnd + 0.00001; t += step) {
      const cursorT = t + cursorOffsetSec;
      const pos = this.interpolateCursorPositionInternal(cursorT, processed);

      if (!pos) {
        if (baked.length > 0) {
          const last = baked[baked.length - 1];
          baked.push({ ...last, time: t });
        } else {
          baked.push({
            time: t,
            x: 0,
            y: 0,
            scale: 1,
            isClicked: false,
            type: this.resolveCursorRenderType('default', backgroundConfig, false),
            opacity: 1
          });
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
      const resolvedCursorType = this.resolveCursorRenderType(pos.cursor_type || 'default', backgroundConfig, Boolean(pos.isClicked));

      baked.push({
        time: t,
        x: pos.x,
        y: pos.y,
        scale: Number((simSquishScale * cursorVis.scale).toFixed(3)),
        isClicked: isClicked,
        type: resolvedCursorType,
        opacity: Number(cursorVis.opacity.toFixed(3)),
        rotation: this.shouldCursorRotate(resolvedCursorType) ? Number((pos.cursor_rotation || 0).toFixed(4)) : 0,
      });
    }

    return baked;
  }

  // --- BAKED CAMERA PATH GENERATION ---
  public generateBakedPath(
    segment: VideoSegment,
    videoWidth: number,
    videoHeight: number,
    fps: number = 60,
    srcCropW?: number,  // actual cropped video source width (for auto-path coord transform)
    srcCropH?: number   // actual cropped video source height
  ): BakedCameraFrame[] {
    const bakedPath: BakedCameraFrame[] = [];
    const step = 1 / fps;
    const duration = Math.max(segment.trimEnd, ...(segment.trimSegments || []).map(s => s.endTime));
    const trimSegments = getTrimSegments(segment, duration);

    const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const croppedW = videoWidth * crop.width;
    const croppedH = videoHeight * crop.height;
    const cropOffsetX = videoWidth * crop.x;
    const cropOffsetY = videoHeight * crop.y;

    // Bake the full source-time range (including hidden gaps between trim segments)
    // so camera motion evolves naturally through cuts — no jarring jumps at bridges.
    const fullStart = trimSegments[0].startTime;
    const fullEnd = trimSegments[trimSegments.length - 1].endTime;

    for (let t = fullStart; t <= fullEnd + 0.00001; t += step) {
      // Pass CROPPED dimensions — calculateCurrentZoomStateInternal's crop
      // conversion assumes viewW/viewH are crop-region pixel dimensions.
      // srcCropW/H tell it the actual video source crop dims for contain-fit.
      const state = this.calculateCurrentZoomStateInternal(t, segment, croppedW, croppedH, srcCropW, srcCropH);

      const globalX = cropOffsetX + (state.positionX * croppedW);
      const globalY = cropOffsetY + (state.positionY * croppedH);

      bakedPath.push({
        time: t,
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
    // Don't reset cursor processing cache — it's invalidated by reference/config check
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
    // During decoder seeks (e.g. jumping trim gaps), keep last frame to avoid black flashes.
    if (video.seeking) return;

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

      // Pass actual cropped video source dims so auto-zoom can contain-fit
      // correctly when canvas aspect ratio differs from video aspect ratio
      const zoomState = this.calculateCurrentZoomState(video.currentTime, segment, canvas.width, canvas.height, srcW, srcH);

      // Supersample only during export to keep preview responsive
      const zf = zoomState?.zoomFactor ?? 1;
      const ss = isExportMode && zf > 1 ? Math.min(Math.ceil(zf), 3) : 1;

      // --- Prepare tempCanvas (video + shadow + border radius) - same for all sub-frames ---
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

      // --- Compute cursor state (squish, visibility) once per frame ---
      const cursorTime = video.currentTime + this.getCursorMovementDelaySec(backgroundConfig);
      const interpolatedPosition = this.interpolateCursorPosition(cursorTime, mousePositions, backgroundConfig);
      const cursorVis = getCursorVisibility(video.currentTime, segment.cursorVisibilitySegments);
      const showCursor = interpolatedPosition && cursorVis.opacity > 0.001;

      if (showCursor) {
        const isActuallyClicked = interpolatedPosition!.isClicked;
        const timeSinceLastHold = video.currentTime - this.lastHoldTime;
        const shouldBeSquished = isActuallyClicked || (this.lastHoldTime >= 0 && timeSinceLastHold < this.CLICK_FUSE_THRESHOLD && timeSinceLastHold > 0);
        if (isActuallyClicked) this.lastHoldTime = video.currentTime;
        const targetScale = shouldBeSquished ? 0.75 : 1.0;
        if (this.currentSquishScale > targetScale) {
          this.currentSquishScale = Math.max(targetScale, this.currentSquishScale - this.SQUISH_SPEED * (this.latestElapsed / (1000 / 120)));
        } else if (this.currentSquishScale < targetScale) {
          this.currentSquishScale = Math.min(targetScale, this.currentSquishScale + this.RELEASE_SPEED * (this.latestElapsed / (1000 / 120)));
        }
      }

      const bgStyle = this.getBackgroundStyle(ctx, backgroundConfig.backgroundType, backgroundConfig.customBackground);
      const sizeRatio = Math.min(canvas.width / srcW, canvas.height / srcH);

      // Helper: compute cursor screen position for a given cursor + zoom state
      const cursorScreenPos = (
        cur: { x: number; y: number },
        zs: ZoomKeyframe | null
      ) => {
        const relCX = (cur.x - srcX) / srcW;
        const relCY = (cur.y - srcY) / (srcH * (1 - legacyCrop));
        let cx = x + relCX * scaledWidth;
        let cy = y + relCY * scaledHeight;
        if (zs && zs.zoomFactor !== 1) {
          cx = cx * zs.zoomFactor + (canvasW - canvasW * zs.zoomFactor) * zs.positionX;
          cy = cy * zs.zoomFactor + (canvasH - canvasH * zs.zoomFactor) * zs.positionY;
        }
        return { x: cx, y: cy };
      };

      // Helper: draw one composited sub-frame (background + video + cursor) with normal compositing
      const drawSubFrame = (
        tCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
        subZoom: ZoomKeyframe | null,
        subCur: { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null,
      ) => {
        // Scene (background + video) with zoom transform
        tCtx.save();
        if (subZoom && subZoom.zoomFactor !== 1) {
          const zW = canvasW * subZoom.zoomFactor;
          const zH = canvasH * subZoom.zoomFactor;
          tCtx.translate((canvasW - zW) * subZoom.positionX, (canvasH - zH) * subZoom.positionY);
          tCtx.scale(subZoom.zoomFactor, subZoom.zoomFactor);
        }
        tCtx.fillStyle = bgStyle;
        tCtx.fillRect(0, 0, canvasW, canvasH);
        tCtx.drawImage(tempCanvas, 0, 0, canvasW, canvasH);
        tCtx.restore();

        // Cursor (screen space)
        if (subCur && showCursor) {
          tCtx.save();
          tCtx.setTransform(1, 0, 0, 1, 0, 0);
          tCtx.globalAlpha = cursorVis.opacity;
          const sp = cursorScreenPos(subCur, subZoom);
          const cScale = (backgroundConfig.cursorScale || 2) * sizeRatio * (subZoom?.zoomFactor || 1) * cursorVis.scale;
          this.drawMouseCursor(
            tCtx as unknown as CanvasRenderingContext2D, sp.x, sp.y,
            interpolatedPosition!.isClicked,
            cScale,
            this.resolveCursorRenderType(subCur.cursor_type || 'default', backgroundConfig, Boolean(subCur.isClicked)),
            subCur.cursor_rotation || 0
          );
          tCtx.restore();
        }
      };

      // --- Motion blur detection (slider-based: 0=off, 50=standard, 100=heavy) ---
      const blurZoomVal = backgroundConfig.motionBlurZoom ?? 10;
      const blurPanVal = backgroundConfig.motionBlurPan ?? 10;
      const blurCursorVal = backgroundConfig.motionBlurCursor ?? 25;
      const maxBlurVal = Math.max(blurZoomVal, blurPanVal, blurCursorVal);
      const anyBlurEnabled = maxBlurVal > 0;
      // Shutter angle: val=50 → 270° (cinematic+), val=100 → 540° (extreme)
      const shutterAngle = maxBlurVal * 5.4;
      // Use 30fps as reference frame interval (matches typical recording/export FPS)
      // so preview blur width matches what export produces
      const refFps = 30;
      const shutterSec = anyBlurEnabled ? (shutterAngle / 360) / refFps : 0;
      // Per-channel shutter (proportional to their slider)
      const cursorShutterSec = blurCursorVal > 0 ? (blurCursorVal * 5.4 / 360) / refFps : 0;
      // Preview is real-time: cap samples to avoid starving the video decoder
      // High zoom = heavier per-draw, so reduce samples further
      // Export uses 12-32 samples offline for perfect quality
      const blurZf = zoomState?.zoomFactor ?? 1;
      const maxN = video.paused ? 2 : blurZf > 5 ? 1 : blurZf > 3 ? 2 : 3;
      const N = Math.min(maxN, shutterAngle <= 0 ? 1 : shutterAngle <= 180 ? 2 : 3);

      // Check if camera/cursor is actually moving
      let cameraMoving = false;
      let cursorMoving = false;
      if (anyBlurEnabled && shutterSec > 0) {
        const halfShutter = shutterSec / 2;
        const t0 = video.currentTime - halfShutter;
        const t1 = video.currentTime + halfShutter;
        if (blurZoomVal > 0 || blurPanVal > 0) {
          const z0 = this.calculateCurrentZoomState(t0, segment, canvasW, canvasH, srcW, srcH);
          const z1 = this.calculateCurrentZoomState(t1, segment, canvasW, canvasH, srcW, srcH);
          if (z0 && z1) {
            if (blurZoomVal > 0 && Math.abs(z0.zoomFactor - z1.zoomFactor) > 0.002) cameraMoving = true;
            if (blurPanVal > 0 && (Math.abs(z0.positionX - z1.positionX) > 0.001 || Math.abs(z0.positionY - z1.positionY) > 0.001)) cameraMoving = true;
          }
        }
        if (blurCursorVal > 0 && interpolatedPosition) {
          const c0 = this.interpolateCursorPosition(t0 + this.getCursorMovementDelaySec(backgroundConfig), mousePositions, backgroundConfig);
          const c1 = this.interpolateCursorPosition(t1 + this.getCursorMovementDelaySec(backgroundConfig), mousePositions, backgroundConfig);
          if (c0 && c1 && Math.hypot(c1.x - c0.x, c1.y - c0.y) > 1.0) cursorMoving = true;
        }
      }

      ctx.save();

      if (cameraMoving) {
        // --- CAMERA BLUR PATH: render each sub-frame to temp canvas, accumulate with lighter ---
        // Ensure accumulation canvas
        if (!this.blurAccumCanvas || this.blurAccumCanvas.width !== canvasW || this.blurAccumCanvas.height !== canvasH) {
          this.blurAccumCanvas = new OffscreenCanvas(canvasW, canvasH);
          this.blurAccumCtx = this.blurAccumCanvas.getContext('2d')!;
        }
        // Ensure sub-frame canvas (rendered with NORMAL compositing, then blitted additively)
        if (!this.blurSubCanvas || this.blurSubCanvas.width !== canvasW || this.blurSubCanvas.height !== canvasH) {
          this.blurSubCanvas = new OffscreenCanvas(canvasW, canvasH);
          this.blurSubCtx = this.blurSubCanvas.getContext('2d')!;
        }
        const aCtx = this.blurAccumCtx!;
        const sCtx = this.blurSubCtx!;
        aCtx.clearRect(0, 0, canvasW, canvasH);

        const centerZoom = zoomState;
        for (let i = 0; i < N; i++) {
          const f = (i + 0.5) / N - 0.5; // [-0.5, +0.5]
          const cameraSubT = video.currentTime + f * shutterSec;
          const cursorSubT = video.currentTime + this.getCursorMovementDelaySec(backgroundConfig) + f * cursorShutterSec;

          // Sample camera — cherry-pick per channel
          const subCamState = this.calculateCurrentZoomState(cameraSubT, segment, canvasW, canvasH, srcW, srcH);
          const subZoom: ZoomKeyframe | null = subCamState ? {
            ...subCamState,
            zoomFactor: blurZoomVal > 0 ? subCamState.zoomFactor : (centerZoom?.zoomFactor ?? 1),
            positionX: blurPanVal > 0 ? subCamState.positionX : (centerZoom?.positionX ?? 0.5),
            positionY: blurPanVal > 0 ? subCamState.positionY : (centerZoom?.positionY ?? 0.5),
          } : centerZoom;

          const subCur = cursorMoving
            ? this.interpolateCursorPosition(cursorSubT, mousePositions, backgroundConfig)
            : interpolatedPosition;

          // Render sub-frame with NORMAL compositing to temp canvas
          sCtx.clearRect(0, 0, canvasW, canvasH);
          drawSubFrame(sCtx, subZoom, subCur);

          // Accumulate onto blur canvas with source-over averaging
          // Online average: frame_i gets alpha = 1/(i+1), blending equally with all prior frames
          // This avoids 8-bit quantization banding that 'lighter' with 1/N alpha causes
          aCtx.save();
          aCtx.globalAlpha = 1 / (i + 1);
          aCtx.drawImage(this.blurSubCanvas!, 0, 0);
          aCtx.restore();
        }

        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.drawImage(this.blurAccumCanvas, 0, 0);

      } else if (cursorMoving && showCursor) {
        // --- CURSOR-ONLY BLUR PATH: single video draw + multi-cursor ---
        drawSubFrame(ctx, zoomState, null);

        // Blurred cursor overlay
        if (!this.blurAccumCanvas || this.blurAccumCanvas.width !== canvasW || this.blurAccumCanvas.height !== canvasH) {
          this.blurAccumCanvas = new OffscreenCanvas(canvasW, canvasH);
          this.blurAccumCtx = this.blurAccumCanvas.getContext('2d')!;
        }
        const aCtx = this.blurAccumCtx!;
        aCtx.clearRect(0, 0, canvasW, canvasH);

        for (let i = 0; i < N; i++) {
          const f = (i + 0.5) / N - 0.5;
          const subCursorT = video.currentTime + this.getCursorMovementDelaySec(backgroundConfig) + f * cursorShutterSec;
          const subCur = this.interpolateCursorPosition(subCursorT, mousePositions, backgroundConfig);
          if (!subCur) continue;

          aCtx.save();
          aCtx.setTransform(1, 0, 0, 1, 0, 0);
          aCtx.globalCompositeOperation = 'lighter';
          aCtx.globalAlpha = cursorVis.opacity / N;
          const sp = cursorScreenPos(subCur, zoomState);
          const cScale = (backgroundConfig.cursorScale || 2) * sizeRatio * (zoomState?.zoomFactor || 1) * cursorVis.scale;
          this.drawMouseCursor(
            aCtx as unknown as CanvasRenderingContext2D, sp.x, sp.y,
            interpolatedPosition!.isClicked, cScale,
            this.resolveCursorRenderType(subCur.cursor_type || 'default', backgroundConfig, Boolean(subCur.isClicked)),
            subCur.cursor_rotation || 0
          );
          aCtx.restore();
        }

        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.drawImage(this.blurAccumCanvas, 0, 0);

      } else {
        // --- NO BLUR PATH: single draw (existing behavior) ---
        drawSubFrame(ctx, zoomState, interpolatedPosition);
      }


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
      case 'white': {
        const wGrad = ctx.createLinearGradient(0, 0, 0, ctx.canvas.height);
        wGrad.addColorStop(0, '#f5f5f5');
        wGrad.addColorStop(0.5, '#ffffff');
        wGrad.addColorStop(1, '#f5f5f5');

        const wCx = ctx.canvas.width / 2;
        const wCy = ctx.canvas.height / 2;
        const wRadial = ctx.createRadialGradient(wCx, wCy, 0, wCx, wCy, ctx.canvas.width * 0.8);
        wRadial.addColorStop(0, 'rgba(225, 225, 225, 0.15)');
        wRadial.addColorStop(1, 'rgba(255, 255, 255, 0)');

        ctx.fillStyle = wGrad;
        ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
        ctx.fillStyle = wRadial;
        ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);

        return 'rgba(0,0,0,0)';
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
    viewH: number,
    srcCropW?: number,  // actual cropped video source width (for auto-path coord transform)
    srcCropH?: number   // actual cropped video source height
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
        trimSegments: segment.trimSegments?.map(s => ({ s: s.startTime, e: s.endTime })),
        crop: segment.crop,
        smoothMotionPath: segment.smoothMotionPath?.map(p => ({ t: p.time, z: p.zoom })),
        zoomKeyframes: segment.zoomKeyframes?.map(k => ({ t: k.time, d: k.duration, x: k.positionX, y: k.positionY, z: k.zoomFactor })),
        zoomInfluence: segment.zoomInfluencePoints?.map(p => ({ t: p.time, v: p.value })),
        cursorVis: segment.cursorVisibilitySegments?.map(s => ({ s: s.startTime, e: s.endTime })),
        vidDims: [viewW, viewH]
      });

      if (this.lastBakeSignature !== signature) {
        this.cachedBakedPath = this.generateBakedPath(segment, viewW / (segment.crop?.width || 1), viewH / (segment.crop?.height || 1), 60, srcCropW, srcCropH);
        this.lastBakeSignature = signature;
      }
    }

    if (!isPaused && this.cachedBakedPath && this.cachedBakedPath.length > 0) {
      const timelineDuration = Math.max(
        segment.trimEnd,
        ...(segment.trimSegments || []).map(s => s.endTime)
      );
      const relTime = toCompactTime(currentTime, segment, timelineDuration);
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

    const state = this.calculateCurrentZoomStateInternal(currentTime, segment, viewW, viewH, srcCropW, srcCropH);
    this.lastCalculatedState = state;
    return state;
  }

  private calculateCurrentZoomStateInternal(
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,  // actual cropped video source width (for auto-path coord transform)
    srcCropH?: number   // actual cropped video source height
  ): ZoomKeyframe {

    // Source crop dimensions — when provided, auto-path video-pixel coords are
    // transformed through contain-fit into canvas-anchor space.  When not provided
    // (backwards compat), viewW/viewH are assumed to match the source crop dims.
    const sCropW = srcCropW ?? viewW;
    const sCropH = srcCropH ?? viewH;

    // --- 1. CALCULATE AUTO-SMART ZOOM STATE (Background Track) ---
    const hasAutoPath = segment.smoothMotionPath && segment.smoothMotionPath.length > 0;
    let autoState: ZoomKeyframe | null = null;

    if (hasAutoPath) {
      const path = segment.smoothMotionPath!;
      const idx = path.findIndex((p: any) => p.time >= currentTime);
      // Default in video-pixel space (center of cropped source)
      const crop0 = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
      const vidFullW = sCropW / crop0.width;
      const vidFullH = sCropH / crop0.height;
      let cam = { x: vidFullW * crop0.x + sCropW / 2, y: vidFullH * crop0.y + sCropH / 2, zoom: 1.0 };

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
        // Use crop center in video-pixel coords so influence=0 returns to crop center
        const cropInf = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
        const fullWInf = sCropW / cropInf.width;
        const fullHInf = sCropH / cropInf.height;
        const centerX = fullWInf * cropInf.x + sCropW / 2;
        const centerY = fullHInf * cropInf.y + sCropH / 2;
        cam.x = centerX + (cam.x - centerX) * influence;
        cam.y = centerY + (cam.y - centerY) * influence;
      }

      // Convert auto-path coords (video pixel space) → canvas-anchor posX/posY
      // via contain-fit when canvas dims differ from source dims
      const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
      const fullW = sCropW / crop.width;
      const fullH = sCropH / crop.height;
      const cropOffsetX = fullW * crop.x;
      const cropOffsetY = fullH * crop.y;

      // Relative position within crop (0-1)
      const relX = (cam.x - cropOffsetX) / sCropW;
      const relY = (cam.y - cropOffsetY) / sCropH;

      // Contain-fit of cropped source into canvas
      const srcAspect = sCropW / sCropH;
      const canvasAspect = viewW / viewH;
      let fitW: number, fitH: number;
      if (srcAspect > canvasAspect) {
        fitW = viewW;
        fitH = viewW / srcAspect;
      } else {
        fitH = viewH;
        fitW = viewH * srcAspect;
      }
      const fitX = (viewW - fitW) / 2;
      const fitY = (viewH - fitH) / 2;

      autoState = {
        time: currentTime,
        duration: 0,
        zoomFactor: cam.zoom,
        positionX: (fitX + relX * fitW) / viewW,
        positionY: (fitY + relY * fitH) / viewH,
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

  private normalizeAngleRad(angle: number): number {
    let a = angle;
    while (a > Math.PI) a -= Math.PI * 2;
    while (a < -Math.PI) a += Math.PI * 2;
    return a;
  }

  private lerpAngleRad(from: number, to: number, t: number): number {
    const delta = this.normalizeAngleRad(to - from);
    return this.normalizeAngleRad(from + delta * t);
  }

  private smoothDampScalar(
    current: number,
    target: number,
    velocity: number,
    smoothTime: number,
    maxSpeed: number,
    deltaTime: number
  ): { value: number; velocity: number } {
    const safeSmoothTime = Math.max(0.0001, smoothTime);
    const omega = 2 / safeSmoothTime;
    const x = omega * deltaTime;
    const exp = 1 / (1 + x + (0.48 * x * x) + (0.235 * x * x * x));

    let change = current - target;
    const originalTarget = target;
    const maxChange = maxSpeed * safeSmoothTime;
    change = Math.max(-maxChange, Math.min(maxChange, change));
    target = current - change;

    const temp = (velocity + omega * change) * deltaTime;
    let newVelocity = (velocity - omega * temp) * exp;
    let output = target + (change + temp) * exp;

    if ((originalTarget - current > 0) === (output > originalTarget)) {
      output = originalTarget;
      newVelocity = (output - originalTarget) / Math.max(deltaTime, 0.0001);
    }

    return { value: output, velocity: newVelocity };
  }

  private smoothDampAngleRad(
    current: number,
    target: number,
    velocity: number,
    smoothTime: number,
    maxSpeed: number,
    deltaTime: number
  ): { value: number; velocity: number } {
    const adjustedTarget = current + this.normalizeAngleRad(target - current);
    return this.smoothDampScalar(current, adjustedTarget, velocity, smoothTime, maxSpeed, deltaTime);
  }

  /**
   * Analytical damped spring step for scalar values.
   * Exact solution of the damped harmonic oscillator ODE — frame-rate independent.
   * Supports underdamped (zeta<1, bouncy), critically damped (zeta=1), overdamped (zeta>1).
   */
  private springStepScalar(
    current: number,
    target: number,
    velocity: number,
    angularFreq: number,
    dampingRatio: number,
    dt: number
  ): { value: number; velocity: number } {
    const disp = current - target;

    if (Math.abs(disp) < 1e-8 && Math.abs(velocity) < 1e-8) {
      return { value: target, velocity: 0 };
    }

    const omega = angularFreq;
    const zeta = dampingRatio;
    let newDisp: number;
    let newVel: number;

    if (zeta < 1.0 - 1e-6) {
      // Underdamped — oscillatory with exponential decay (the wiggle)
      const alpha = omega * Math.sqrt(1 - zeta * zeta);
      const decay = Math.exp(-omega * zeta * dt);
      const cosA = Math.cos(alpha * dt);
      const sinA = Math.sin(alpha * dt);

      newDisp = decay * (
        disp * cosA +
        ((velocity + omega * zeta * disp) / alpha) * sinA
      );
      newVel = decay * (
        velocity * cosA -
        ((velocity * zeta * omega + omega * omega * disp) / alpha) * sinA
      );
    } else if (zeta > 1.0 + 1e-6) {
      // Overdamped — exponential decay without oscillation
      const disc = Math.sqrt(zeta * zeta - 1);
      const s1 = -omega * (zeta - disc);
      const s2 = -omega * (zeta + disc);
      const c2 = (velocity - s1 * disp) / (s2 - s1);
      const c1 = disp - c2;
      const e1 = Math.exp(s1 * dt);
      const e2 = Math.exp(s2 * dt);

      newDisp = c1 * e1 + c2 * e2;
      newVel = c1 * s1 * e1 + c2 * s2 * e2;
    } else {
      // Critically damped — fastest non-oscillatory settling
      const decay = Math.exp(-omega * dt);
      newDisp = (disp + (velocity + omega * disp) * dt) * decay;
      newVel = (velocity - (velocity + omega * disp) * omega * dt) * decay;
    }

    return { value: target + newDisp, velocity: newVel };
  }

  /** Spring step for angle values — normalizes angle then delegates to scalar solver. */
  private springStepAngle(
    current: number,
    target: number,
    velocity: number,
    angularFreq: number,
    dampingRatio: number,
    dt: number
  ): { value: number; velocity: number } {
    const adjustedTarget = current + this.normalizeAngleRad(target - current);
    return this.springStepScalar(current, adjustedTarget, velocity, angularFreq, dampingRatio, dt);
  }

  private smoothMousePositions(
    positions: MousePosition[],
    targetFps: number = 120,
    backgroundConfig?: BackgroundConfig | null
  ): MousePosition[] {
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

    const windowSize = (this.getCursorSmoothness(backgroundConfig) * 2) + 1;
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

  private processCursorPositions(
    positions: MousePosition[],
    backgroundConfig?: BackgroundConfig | null
  ): MousePosition[] {
    const smoothed = this.smoothMousePositions(positions, 120, backgroundConfig);
    const springed = this.applySpringPositionDynamics(smoothed, backgroundConfig);
    const wiggled = this.applyAdaptiveCursorWiggle(springed, backgroundConfig);
    return this.applyCursorTiltOffset(wiggled, backgroundConfig);
  }

  /** Only asymmetric pointer-like cursors get a static tilt offset.
   *  Text beam, crosshair, resize handles etc. are symmetric and stay upright. */
  private shouldCursorTilt(cursorType: string): boolean {
    const t = cursorType.toLowerCase();
    return t.startsWith('default') || t.startsWith('pointer');
  }

  /** Adds a static angular offset (resting tilt) to cursor rotation. */
  private applyCursorTiltOffset(
    positions: MousePosition[],
    backgroundConfig?: BackgroundConfig | null
  ): MousePosition[] {
    const tiltRad = this.getCursorTiltAngleRad(backgroundConfig);
    if (Math.abs(tiltRad) < 0.0001) return positions;
    return positions.map(pos => ({
      ...pos,
      cursor_rotation: (pos.cursor_rotation || 0) +
        (this.shouldCursorTilt(pos.cursor_type || 'default') ? tiltRad : 0),
    }));
  }

  /**
   * Spring-based cursor position dynamics — adds physical inertia to cursor movement.
   * The cursor trails behind during fast movement and slightly overshoots on stop,
   * creating the "alive" cinematic feel used by Screen Studio.
   * Runs BEFORE rotation wiggle so tilt is computed from spring-smoothed velocities.
   */
  private applySpringPositionDynamics(
    positions: MousePosition[],
    backgroundConfig?: BackgroundConfig | null
  ): MousePosition[] {
    if (positions.length < 2) return positions;

    const strength = this.getCursorWiggleStrength(backgroundConfig);
    if (strength <= 0.001) return positions;

    const dampingRatio = this.getCursorWiggleDamping(backgroundConfig);
    const responseHz = this.getCursorWiggleResponse(backgroundConfig);

    // Position spring: stiffer at low strength (subtle), looser at high (dramatic)
    const baseOmega = 2 * Math.PI * responseHz;
    const posOmega = baseOmega * (4.0 - strength * 2.5);
    // More damped than rotation spring — position overshoot should be very subtle
    const posZeta = Math.min(0.92, dampingRatio + 0.18);
    // Max displacement cap prevents extreme lag at very fast mouse speeds
    const maxDisp = 8 + strength * 28;

    const result: MousePosition[] = [];
    let sx = positions[0].x;
    let sy = positions[0].y;
    let vx = 0;
    let vy = 0;

    result.push({ ...positions[0] });

    for (let i = 1; i < positions.length; i++) {
      const prev = positions[i - 1];
      const target = positions[i];
      const dt = Math.max(1 / 1000, target.timestamp - prev.timestamp);

      const stepX = this.springStepScalar(sx, target.x, vx, posOmega, posZeta, dt);
      const stepY = this.springStepScalar(sy, target.y, vy, posOmega, posZeta, dt);

      sx = stepX.value;
      sy = stepY.value;
      vx = stepX.velocity;
      vy = stepY.velocity;

      // Clamp displacement to prevent excessive trailing at extreme speeds
      const dx = sx - target.x;
      const dy = sy - target.y;
      const dist = Math.hypot(dx, dy);
      if (dist > maxDisp) {
        const ratio = maxDisp / dist;
        sx = target.x + dx * ratio;
        sy = target.y + dy * ratio;
        vx *= ratio;
        vy *= ratio;
      }

      result.push({
        ...target,
        x: sx,
        y: sy,
      });
    }

    return result;
  }

  private applyAdaptiveCursorWiggle(
    positions: MousePosition[],
    backgroundConfig?: BackgroundConfig | null
  ): MousePosition[] {
    if (positions.length < 2) return positions;

    const strength = this.getCursorWiggleStrength(backgroundConfig);
    if (strength <= 0.001) return positions;

    const dampingRatio = this.getCursorWiggleDamping(backgroundConfig);
    const responseHz = this.getCursorWiggleResponse(backgroundConfig);

    const result: MousePosition[] = [];
    let lagHeading = 0;
    let lagHeadingVel = 0;
    let hasHeading = false;
    let cursorRotation = 0;
    let cursorRotationVel = 0;

    // Derive physics params from user-facing knobs
    const maxTiltRad = (2.2 + strength * 8.8) * (Math.PI / 180);
    const headingSmoothTime = 0.28 - strength * 0.17;
    const tiltGain = 0.33 + strength * 0.92;
    const speedStart = 120;
    const speedFull = 1650;

    // Underdamped spring params for the rotation channel
    // omega = natural angular frequency; zeta = damping ratio (<1 → bounce)
    const rotationOmega = 2 * Math.PI * responseHz;
    const rotationZeta = dampingRatio;

    result.push({ ...positions[0], cursor_rotation: 0 });

    for (let i = 1; i < positions.length; i++) {
      const prevTarget = positions[i - 1];
      const target = positions[i];
      const dtRaw = Math.max(1 / 1000, target.timestamp - prevTarget.timestamp);

      const targetVx = (target.x - prevTarget.x) / dtRaw;
      const targetVy = (target.y - prevTarget.y) / dtRaw;
      const speed = Math.hypot(targetVx, targetVy);

      let tiltTarget = 0;

      if (speed > speedStart) {
        const heading = Math.atan2(targetVy, targetVx);
        if (!hasHeading) {
          lagHeading = heading;
          hasHeading = true;
        }

        const headingStep = this.smoothDampAngleRad(
          lagHeading, heading, lagHeadingVel,
          headingSmoothTime, 18, dtRaw
        );
        lagHeading = headingStep.value;
        lagHeadingVel = headingStep.velocity;

        // SmoothStep speed fade (less abrupt than linear)
        const t = Math.max(0, Math.min(1, (speed - speedStart) / (speedFull - speedStart)));
        const speedFade = t * t * (3 - 2 * t);

        const rawTilt = this.normalizeAngleRad(heading - lagHeading) * tiltGain * speedFade;
        tiltTarget = Math.max(-maxTiltRad, Math.min(maxTiltRad, rawTilt));
      }
      // else: tiltTarget stays 0 → spring settles with bounce

      // Underdamped spring drives rotation toward tiltTarget
      // During movement: tracks smoothly (target changes gradually)
      // On stop: target jumps to 0 → spring overshoots → wiggle
      const rotStep = this.springStepAngle(
        cursorRotation, tiltTarget, cursorRotationVel,
        rotationOmega, rotationZeta, dtRaw
      );
      cursorRotation = rotStep.value;
      cursorRotationVel = rotStep.velocity;

      result.push({
        ...target,
        x: target.x,
        y: target.y,
        cursor_rotation: cursorRotation,
      });
    }

    return result;
  }

  private interpolateCursorPosition(
    currentTime: number,
    mousePositions: MousePosition[],
    backgroundConfig?: BackgroundConfig | null,
  ): { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null {
    const processSignature = this.getCursorProcessingSignature(backgroundConfig);

    // 1. Invalidate cache if input changed
    if (this.lastMousePositionsRef !== mousePositions || this.lastCursorProcessSignature !== processSignature) {
      this.processedCursorPositions = null;
      this.lastMousePositionsRef = mousePositions;
      this.lastCursorProcessSignature = processSignature;
    }

    // 2. Generate cache if needed
    if (!this.processedCursorPositions && mousePositions.length > 0) {
      this.processedCursorPositions = this.processCursorPositions(mousePositions, backgroundConfig);
    }

    // 3. Use cached data
    const dataToUse = this.processedCursorPositions || mousePositions;

    return this.interpolateCursorPositionInternal(currentTime, dataToUse);
  }

  // Internal version to support both live and export baking
  private interpolateCursorPositionInternal(
    currentTime: number,
    positions: MousePosition[],
  ): { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null {
    if (!positions || positions.length === 0) return null;

    const exactMatch = positions.find((pos: MousePosition) => Math.abs(pos.timestamp - currentTime) < 0.001);
    if (exactMatch) {
      return {
        x: exactMatch.x,
        y: exactMatch.y,
        isClicked: Boolean(exactMatch.isClicked),
        cursor_type: exactMatch.cursor_type || 'default',
        cursor_rotation: exactMatch.cursor_rotation || 0,
      };
    }

    const nextIndex = positions.findIndex((pos: MousePosition) => pos.timestamp > currentTime);
    if (nextIndex === -1) {
      const last = positions[positions.length - 1];
      return {
        x: last.x,
        y: last.y,
        isClicked: Boolean(last.isClicked),
        cursor_type: last.cursor_type || 'default',
        cursor_rotation: last.cursor_rotation || 0,
      };
    }

    if (nextIndex === 0) {
      const first = positions[0];
      return {
        x: first.x,
        y: first.y,
        isClicked: Boolean(first.isClicked),
        cursor_type: first.cursor_type || 'default',
        cursor_rotation: first.cursor_rotation || 0,
      };
    }

    const prev = positions[nextIndex - 1];
    const next = positions[nextIndex];
    const t = (currentTime - prev.timestamp) / (next.timestamp - prev.timestamp);

    return {
      x: prev.x + (next.x - prev.x) * t,
      y: prev.y + (next.y - prev.y) * t,
      isClicked: Boolean(prev.isClicked || next.isClicked),
      cursor_type: next.cursor_type || 'default',
      cursor_rotation: this.lerpAngleRad(prev.cursor_rotation || 0, next.cursor_rotation || 0, t),
    };
  }

  /** Arrow, pointing hand, and text cursors rotate. Grip cursors (grab, grabbing) stay upright. */
  private shouldCursorRotate(cursorType: string): boolean {
    const t = cursorType.toLowerCase();
    return t.startsWith('default-') || t.startsWith('pointer-') || t.startsWith('text-');
  }

  private getCursorRotationPivot(cursorType: string): { x: number; y: number } {
    switch (cursorType.toLowerCase()) {
      case 'pointer-screenstudio':
      case 'openhand-screenstudio':
      case 'closehand-screenstudio':
      case 'pointer-macos26':
      case 'openhand-macos26':
      case 'closehand-macos26':
      case 'pointer-sgtcute':
      case 'openhand-sgtcute':
      case 'closehand-sgtcute':
      case 'pointer-sgtcool':
      case 'openhand-sgtcool':
      case 'closehand-sgtcool':
      case 'pointer-sgtai':
      case 'openhand-sgtai':
      case 'closehand-sgtai':
      case 'pointer-sgtpixel':
      case 'openhand-sgtpixel':
      case 'closehand-sgtpixel':
      case 'pointer-jepriwin11':
      case 'openhand-jepriwin11':
      case 'closehand-jepriwin11':
        return { x: 3.0, y: 8.5 };
      case 'text-screenstudio':
      case 'text-macos26':
      case 'text-sgtcute':
      case 'text-sgtcool':
      case 'text-sgtai':
      case 'text-sgtpixel':
      case 'text-jepriwin11':
        return { x: 0, y: 0 };
      default:
        return { x: 3.6, y: 5.6 };
    }
  }

  private drawMouseCursor(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    isClicked: boolean,
    scale: number = 2,
    cursorType: string = 'default',
    rotation: number = 0
  ) {
    // Always render through offscreen so visible->dismiss transition uses identical
    // rasterization and bounds (prevents viewbox "jump" / clipping on fade start).
    const shadowStrength = this.getCursorShadowStrength(this.activeRenderContext?.backgroundConfig);
    const normalizedShadow = Math.max(0, shadowStrength) / 100;
    const shadowOverdrive = Math.max(0, normalizedShadow - 1);
    const shadowBlur = 1.2 + (9.0 * Math.min(normalizedShadow, 1)) + (8.0 * shadowOverdrive);
    const shadowOffset = (2.2 * Math.min(normalizedShadow, 1)) + (1.8 * shadowOverdrive);
    const shapeRadius = Math.max(28, scale * 32);
    const margin = Math.ceil(shapeRadius + shadowBlur + shadowOffset + 24);
    const idealSize = margin * 2;

    // Cap offscreen canvas to prevent quadratic cost at high zoom.
    // At 11x zoom the ideal size reaches ~1500px — shadow blur on a 1500x1500
    // canvas every frame is extremely expensive. Cap to 512 and scale up via
    // drawImage; the shadow is soft so bilinear upsampling is imperceptible.
    const maxPreviewSize = 512;
    const ratio = idealSize > maxPreviewSize ? maxPreviewSize / idealSize : 1;
    const size = Math.ceil(idealSize * ratio);

    if (this.cursorOffscreen.width !== size || this.cursorOffscreen.height !== size) {
      this.cursorOffscreen.width = size;
      this.cursorOffscreen.height = size;
      this.cursorOffscreenCtx = this.cursorOffscreen.getContext('2d')!;
    }

    const oCtx = this.cursorOffscreenCtx;
    oCtx.clearRect(0, 0, size, size);
    oCtx.globalAlpha = 1;

    if (ratio < 1) {
      oCtx.save();
      oCtx.scale(ratio, ratio);
      this.drawCursorShape(oCtx as unknown as CanvasRenderingContext2D, margin, margin, isClicked, scale, cursorType, rotation, ratio);
      oCtx.restore();
    } else {
      this.drawCursorShape(oCtx as unknown as CanvasRenderingContext2D, margin, margin, isClicked, scale, cursorType, rotation);
    }

    ctx.save();
    if (ratio < 1) {
      // Scale capped canvas back to ideal size — cursor + shadow appear full-res
      ctx.drawImage(this.cursorOffscreen, x - margin, y - margin, idealSize, idealSize);
    } else {
      ctx.drawImage(this.cursorOffscreen, x - margin, y - margin);
    }
    ctx.restore();
  }

  private getMacos26CursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-macos26': return this.defaultMacos26Image;
      case 'text-macos26': return this.textMacos26Image;
      case 'pointer-macos26': return this.pointerMacos26Image;
      case 'openhand-macos26': return this.openHandMacos26Image;
      case 'closehand-macos26': return this.closeHandMacos26Image;
      case 'wait-macos26': return this.waitMacos26Image;
      case 'appstarting-macos26': return this.appStartingMacos26Image;
      case 'crosshair-macos26': return this.crosshairMacos26Image;
      case 'resize-ns-macos26': return this.resizeNsMacos26Image;
      case 'resize-we-macos26': return this.resizeWeMacos26Image;
      case 'resize-nwse-macos26': return this.resizeNwseMacos26Image;
      case 'resize-nesw-macos26': return this.resizeNeswMacos26Image;
      default: return null;
    }
  }

  private getSgtcuteCursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-sgtcute': return this.defaultSgtcuteImage;
      case 'text-sgtcute': return this.textSgtcuteImage;
      case 'pointer-sgtcute': return this.pointerSgtcuteImage;
      case 'openhand-sgtcute': return this.openHandSgtcuteImage;
      case 'closehand-sgtcute': return this.closeHandSgtcuteImage;
      case 'wait-sgtcute': return this.waitSgtcuteImage;
      case 'appstarting-sgtcute': return this.appStartingSgtcuteImage;
      case 'crosshair-sgtcute': return this.crosshairSgtcuteImage;
      case 'resize-ns-sgtcute': return this.resizeNsSgtcuteImage;
      case 'resize-we-sgtcute': return this.resizeWeSgtcuteImage;
      case 'resize-nwse-sgtcute': return this.resizeNwseSgtcuteImage;
      case 'resize-nesw-sgtcute': return this.resizeNeswSgtcuteImage;
      default: return null;
    }
  }

  private getSgtcoolCursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-sgtcool': return this.defaultSgtcoolImage;
      case 'text-sgtcool': return this.textSgtcoolImage;
      case 'pointer-sgtcool': return this.pointerSgtcoolImage;
      case 'openhand-sgtcool': return this.openHandSgtcoolImage;
      case 'closehand-sgtcool': return this.closeHandSgtcoolImage;
      case 'wait-sgtcool': return this.waitSgtcoolImage;
      case 'appstarting-sgtcool': return this.appStartingSgtcoolImage;
      case 'crosshair-sgtcool': return this.crosshairSgtcoolImage;
      case 'resize-ns-sgtcool': return this.resizeNsSgtcoolImage;
      case 'resize-we-sgtcool': return this.resizeWeSgtcoolImage;
      case 'resize-nwse-sgtcool': return this.resizeNwseSgtcoolImage;
      case 'resize-nesw-sgtcool': return this.resizeNeswSgtcoolImage;
      default: return null;
    }
  }

  private getSgtaiCursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-sgtai': return this.defaultSgtaiImage;
      case 'text-sgtai': return this.textSgtaiImage;
      case 'pointer-sgtai': return this.pointerSgtaiImage;
      case 'openhand-sgtai': return this.openHandSgtaiImage;
      case 'closehand-sgtai': return this.closeHandSgtaiImage;
      case 'wait-sgtai': return this.waitSgtaiImage;
      case 'appstarting-sgtai': return this.appStartingSgtaiImage;
      case 'crosshair-sgtai': return this.crosshairSgtaiImage;
      case 'resize-ns-sgtai': return this.resizeNsSgtaiImage;
      case 'resize-we-sgtai': return this.resizeWeSgtaiImage;
      case 'resize-nwse-sgtai': return this.resizeNwseSgtaiImage;
      case 'resize-nesw-sgtai': return this.resizeNeswSgtaiImage;
      default: return null;
    }
  }

  private getSgtpixelCursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-sgtpixel': return this.defaultSgtpixelImage;
      case 'text-sgtpixel': return this.textSgtpixelImage;
      case 'pointer-sgtpixel': return this.pointerSgtpixelImage;
      case 'openhand-sgtpixel': return this.openHandSgtpixelImage;
      case 'closehand-sgtpixel': return this.closeHandSgtpixelImage;
      case 'wait-sgtpixel': return this.waitSgtpixelImage;
      case 'appstarting-sgtpixel': return this.appStartingSgtpixelImage;
      case 'crosshair-sgtpixel': return this.crosshairSgtpixelImage;
      case 'resize-ns-sgtpixel': return this.resizeNsSgtpixelImage;
      case 'resize-we-sgtpixel': return this.resizeWeSgtpixelImage;
      case 'resize-nwse-sgtpixel': return this.resizeNwseSgtpixelImage;
      case 'resize-nesw-sgtpixel': return this.resizeNeswSgtpixelImage;
      default: return null;
    }
  }

  private getJepriwin11CursorImage(type: CursorRenderType): HTMLImageElement | null {
    switch (type) {
      case 'default-jepriwin11': return this.defaultJepriwin11Image;
      case 'text-jepriwin11': return this.textJepriwin11Image;
      case 'pointer-jepriwin11': return this.pointerJepriwin11Image;
      case 'openhand-jepriwin11': return this.openHandJepriwin11Image;
      case 'closehand-jepriwin11': return this.closeHandJepriwin11Image;
      case 'wait-jepriwin11': return this.waitJepriwin11Image;
      case 'appstarting-jepriwin11': return this.appStartingJepriwin11Image;
      case 'crosshair-jepriwin11': return this.crosshairJepriwin11Image;
      case 'resize-ns-jepriwin11': return this.resizeNsJepriwin11Image;
      case 'resize-we-jepriwin11': return this.resizeWeJepriwin11Image;
      case 'resize-nwse-jepriwin11': return this.resizeNwseJepriwin11Image;
      case 'resize-nesw-jepriwin11': return this.resizeNeswJepriwin11Image;
      default: return null;
    }
  }

  private getScreenStudioCursorImage(type: CursorRenderType | string): HTMLImageElement | null {
    switch (type) {
      case 'default-screenstudio': return this.defaultScreenStudioImage;
      case 'text-screenstudio': return this.textScreenStudioImage;
      case 'pointer-screenstudio': return this.pointerScreenStudioImage;
      case 'openhand-screenstudio': return this.openHandScreenStudioImage;
      case 'closehand-screenstudio': return this.closeHandScreenStudioImage;
      case 'wait-screenstudio': return this.waitScreenStudioImage;
      case 'appstarting-screenstudio': return this.appStartingScreenStudioImage;
      case 'crosshair-screenstudio': return this.crosshairScreenStudioImage;
      case 'resize-ns-screenstudio': return this.resizeNsScreenStudioImage;
      case 'resize-we-screenstudio': return this.resizeWeScreenStudioImage;
      case 'resize-nwse-screenstudio': return this.resizeNwseScreenStudioImage;
      case 'resize-nesw-screenstudio': return this.resizeNeswScreenStudioImage;
      default: return null;
    }
  }

  private drawCenteredCursorImage(ctx: CanvasRenderingContext2D, img: HTMLImageElement) {
    if (!img.complete || img.naturalWidth === 0 || img.naturalHeight === 0) return;
    // Normalize very large-source SVG cursors (e.g. imported sprite slices) so they
    // match the same on-canvas footprint as the native cursor packs.
    const sourceMax = Math.max(img.naturalWidth, img.naturalHeight);
    const normalizeScale = sourceMax > 96 ? (48 / sourceMax) : 1;
    const drawW = img.naturalWidth * normalizeScale;
    const drawH = img.naturalHeight * normalizeScale;
    const hotspotX = drawW * 0.5;
    const hotspotY = drawH * 0.5;
    ctx.translate(-hotspotX, -hotspotY);
    ctx.drawImage(img, 0, 0, drawW, drawH);
  }

  private drawCursorShape(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    _isClicked: boolean,
    scale: number = 2,
    cursorType: string,
    rotation: number = 0,
    shadowScale: number = 1
  ) {
    const lowerType = cursorType.toLowerCase();
    ctx.save();
    ctx.translate(x, y);
    if (this.shouldCursorRotate(lowerType) && Math.abs(rotation) > 0.0001) {
      const pivot = this.getCursorRotationPivot(lowerType);
      ctx.translate(pivot.x, pivot.y);
      ctx.rotate(rotation);
      ctx.translate(-pivot.x, -pivot.y);
    }
    ctx.scale(scale, scale);
    ctx.scale(this.currentSquishScale, this.currentSquishScale);

    const cursorShadowStrength = this.getCursorShadowStrength(this.activeRenderContext?.backgroundConfig);
    if (cursorShadowStrength > 0.001) {
      const normalized = cursorShadowStrength / 100;
      const base = Math.pow(Math.min(normalized, 1), 0.8);
      const overdrive = Math.max(0, normalized - 1);
      const alpha = Math.min(1, (0.9 * base) + (0.6 * overdrive));
      ctx.shadowColor = `rgba(0, 0, 0, ${alpha.toFixed(3)})`;
      ctx.shadowBlur = (1.2 + (9.0 * base) + (8.0 * overdrive)) * shadowScale;
      ctx.shadowOffsetX = ((1.1 * base) + (1.0 * overdrive)) * shadowScale;
      ctx.shadowOffsetY = ((2.2 * base) + (1.8 * overdrive)) * shadowScale;
    } else {
      ctx.shadowColor = 'rgba(0,0,0,0)';
      ctx.shadowBlur = 0;
      ctx.shadowOffsetX = 0;
      ctx.shadowOffsetY = 0;
    }

    let effectiveType = lowerType;
    if (effectiveType.endsWith('-screenstudio')) {
      const image = this.getScreenStudioCursorImage(effectiveType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType === 'pointer-screenstudio' && (!this.pointerScreenStudioImage.complete || this.pointerScreenStudioImage.naturalWidth === 0)) {
      effectiveType = 'default-screenstudio';
    }
    if (effectiveType === 'openhand-screenstudio' && (!this.openHandScreenStudioImage.complete || this.openHandScreenStudioImage.naturalWidth === 0)) {
      effectiveType = 'pointer-screenstudio';
    }
    if (effectiveType.endsWith('-macos26')) {
      const image = this.getMacos26CursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType.endsWith('-sgtcute')) {
      const image = this.getSgtcuteCursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType.endsWith('-sgtcool')) {
      const image = this.getSgtcoolCursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType.endsWith('-sgtai')) {
      const image = this.getSgtaiCursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType.endsWith('-sgtpixel')) {
      const image = this.getSgtpixelCursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }
    if (effectiveType.endsWith('-jepriwin11')) {
      const image = this.getJepriwin11CursorImage(effectiveType as CursorRenderType);
      if (!image || !image.complete || image.naturalWidth === 0) {
        effectiveType = 'default-screenstudio';
      }
    }

    const mappingKey = `${cursorType}=>${effectiveType}`;
    if (!this.loggedCursorMappings.has(mappingKey)) {
      this.loggedCursorMappings.add(mappingKey);
      console.log('[CursorDebug] map', {
        rawType: cursorType,
        effectiveType,
      });
    }

    if (!this.loggedCursorTypes.has(effectiveType)) {
      this.loggedCursorTypes.add(effectiveType);
      const debugImg =
        this.getScreenStudioCursorImage(effectiveType) ??
        this.getMacos26CursorImage(effectiveType as CursorRenderType) ??
        this.getSgtcuteCursorImage(effectiveType as CursorRenderType) ??
        this.getSgtcoolCursorImage(effectiveType as CursorRenderType) ??
        this.getSgtaiCursorImage(effectiveType as CursorRenderType) ??
        this.getSgtpixelCursorImage(effectiveType as CursorRenderType) ??
        this.getJepriwin11CursorImage(effectiveType as CursorRenderType);
      console.log('[CursorDebug] loaded', {
        effectiveType,
        src: debugImg?.src,
        naturalWidth: debugImg?.naturalWidth,
        naturalHeight: debugImg?.naturalHeight,
        complete: debugImg?.complete,
      });
    }

    switch (effectiveType) {
      case 'text-screenstudio':
      case 'pointer-screenstudio':
      case 'openhand-screenstudio':
      case 'closehand-screenstudio':
      case 'wait-screenstudio':
      case 'appstarting-screenstudio':
      case 'crosshair-screenstudio':
      case 'resize-ns-screenstudio':
      case 'resize-we-screenstudio':
      case 'resize-nwse-screenstudio':
      case 'resize-nesw-screenstudio': {
        const img = this.getScreenStudioCursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-macos26':
      case 'text-macos26':
      case 'pointer-macos26':
      case 'openhand-macos26':
      case 'closehand-macos26':
      case 'wait-macos26':
      case 'appstarting-macos26':
      case 'crosshair-macos26':
      case 'resize-ns-macos26':
      case 'resize-we-macos26':
      case 'resize-nwse-macos26':
      case 'resize-nesw-macos26': {
        const img = this.getMacos26CursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-sgtcute':
      case 'text-sgtcute':
      case 'pointer-sgtcute':
      case 'openhand-sgtcute':
      case 'closehand-sgtcute':
      case 'wait-sgtcute':
      case 'appstarting-sgtcute':
      case 'crosshair-sgtcute':
      case 'resize-ns-sgtcute':
      case 'resize-we-sgtcute':
      case 'resize-nwse-sgtcute':
      case 'resize-nesw-sgtcute': {
        const img = this.getSgtcuteCursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-sgtcool':
      case 'text-sgtcool':
      case 'pointer-sgtcool':
      case 'openhand-sgtcool':
      case 'closehand-sgtcool':
      case 'wait-sgtcool':
      case 'appstarting-sgtcool':
      case 'crosshair-sgtcool':
      case 'resize-ns-sgtcool':
      case 'resize-we-sgtcool':
      case 'resize-nwse-sgtcool':
      case 'resize-nesw-sgtcool': {
        const img = this.getSgtcoolCursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-sgtai':
      case 'text-sgtai':
      case 'pointer-sgtai':
      case 'openhand-sgtai':
      case 'closehand-sgtai':
      case 'wait-sgtai':
      case 'appstarting-sgtai':
      case 'crosshair-sgtai':
      case 'resize-ns-sgtai':
      case 'resize-we-sgtai':
      case 'resize-nwse-sgtai':
      case 'resize-nesw-sgtai': {
        const img = this.getSgtaiCursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-sgtpixel':
      case 'text-sgtpixel':
      case 'pointer-sgtpixel':
      case 'openhand-sgtpixel':
      case 'closehand-sgtpixel':
      case 'wait-sgtpixel':
      case 'appstarting-sgtpixel':
      case 'crosshair-sgtpixel':
      case 'resize-ns-sgtpixel':
      case 'resize-we-sgtpixel':
      case 'resize-nwse-sgtpixel':
      case 'resize-nesw-sgtpixel': {
        const img = this.getSgtpixelCursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-jepriwin11':
      case 'text-jepriwin11':
      case 'pointer-jepriwin11':
      case 'openhand-jepriwin11':
      case 'closehand-jepriwin11':
      case 'wait-jepriwin11':
      case 'appstarting-jepriwin11':
      case 'crosshair-jepriwin11':
      case 'resize-ns-jepriwin11':
      case 'resize-we-jepriwin11':
      case 'resize-nwse-jepriwin11':
      case 'resize-nesw-jepriwin11': {
        const img = this.getJepriwin11CursorImage(effectiveType);
        if (img) this.drawCenteredCursorImage(ctx, img);
        break;
      }

      case 'default-screenstudio': {
        const img = this.defaultScreenStudioImage;
        this.drawCenteredCursorImage(ctx, img);
        break;
      }

      default: {
        const img = this.defaultScreenStudioImage;
        this.drawCenteredCursorImage(ctx, img);
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
    const duration = Math.max(segment.trimEnd, ...(segment.trimSegments || []).map(s => s.endTime));

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
      const compactRanges = sourceRangeToCompactRanges(textSeg.startTime, textSeg.endTime, segment, duration);
      for (const range of compactRanges) {
        result.push({
          startTime: range.start,
          endTime: range.end,
          x: cropX,
          y: cropY,
          width: cropW,
          height: cropH,
          data: Array.from(imageData.data)
        });
      }
    }

    return result;
  }
}

export const videoRenderer = new VideoRenderer();
