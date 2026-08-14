import type {
  BakedOverlayPayload,
  KeystrokeEvent,
  TextSegment,
  VideoSegment,
} from "@/types/video";
import { normalizeTextStyle } from "@/lib/textStyleDefaults";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";
import {
  DEFAULT_KEYSTROKE_OVERLAY_SCALE,
  DEFAULT_KEYSTROKE_OVERLAY_X,
  DEFAULT_KEYSTROKE_OVERLAY_Y,
  type KeystrokeBubbleLayout,
  type KeystrokeState,
  drawKeystrokeBubble,
  getCachedKeystrokeBubbleLayout,
  getKeystrokeOverlayConfig,
  getKeystrokeOverlayTransform,
  rebuildKeystrokeRenderCache,
} from "./keystrokeRenderer";
import {
  buildTextLayout,
  drawTextOverlay,
  getTextHitArea,
} from "./overlayTextRenderer";

export const MAX_OVERLAY_ATLAS_SIZE = 4096;
const ATLAS_GAP = 2;

export interface OverlayAtlasRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface PackedOverlayAtlas {
  rects: OverlayAtlasRect[];
  width: number;
  height: number;
}

interface TextAtlasItem {
  text: TextSegment;
  hitArea: { x: number; y: number; width: number; height: number };
  pad: number;
  width: number;
  height: number;
  rect?: OverlayAtlasRect;
}

interface KeystrokeAtlasItem {
  uniqueKey: string;
  event: KeystrokeEvent;
  layout: KeystrokeBubbleLayout;
  pad: number;
  width: number;
  height: number;
  rectNormal?: OverlayAtlasRect;
  rectHeld?: OverlayAtlasRect;
}

function assertAtlasDimension(value: number, label: string): number {
  const rounded = Math.max(1, Math.ceil(value));
  if (!Number.isFinite(rounded) || rounded > MAX_OVERLAY_ATLAS_SIZE) {
    throw new Error(
      `${label} is too large for export. Reduce its text size or overlay scale.`,
    );
  }
  return rounded;
}

export function packOverlayAtlasRects(
  sizes: ReadonlyArray<{ width: number; height: number }>,
): PackedOverlayAtlas {
  const rects: OverlayAtlasRect[] = [];
  let packX = 0;
  let packY = 0;
  let rowHeight = 0;
  let usedWidth = 1;

  for (const size of sizes) {
    const width = assertAtlasDimension(size.width, "An overlay");
    const height = assertAtlasDimension(size.height, "An overlay");
    if (packX + width > MAX_OVERLAY_ATLAS_SIZE) {
      packX = 0;
      packY += rowHeight + ATLAS_GAP;
      rowHeight = 0;
    }
    if (packY + height > MAX_OVERLAY_ATLAS_SIZE) {
      throw new Error(
        "The text and keystroke overlays exceed export capacity. Remove some overlays or reduce their size.",
      );
    }
    rects.push({ x: packX, y: packY, w: width, h: height });
    usedWidth = Math.max(usedWidth, packX + width);
    packX += width + ATLAS_GAP;
    rowHeight = Math.max(rowHeight, height);
  }

  return {
    rects,
    width: usedWidth,
    height: Math.max(1, Math.min(MAX_OVERLAY_ATLAS_SIZE, packY + rowHeight)),
  };
}

export function getKeystrokeBakePadding(
  layout: KeystrokeBubbleLayout,
): number {
  return Math.max(28, Math.round(layout.fontSize * 1.35));
}

function getOverlayTextSegments(segment: VideoSegment): TextSegment[] {
  return [
    ...getVisibleSubtitleSegments(segment),
    ...(segment.textSegments ?? []),
  ];
}

function drawKeystrokeAtlasItem(
  context: CanvasRenderingContext2D,
  item: KeystrokeAtlasItem,
  rect: OverlayAtlasRect,
  held: boolean,
): void {
  const isMouse =
    item.event.type === "mousedown" || item.event.type === "wheel";
  const baseSlant = isMouse ? -6 : 0;
  const baseRoundness = isMouse ? 96 : 88;
  drawKeystrokeBubble(
    context,
    item.event,
    rect.x + item.pad,
    rect.y + item.pad,
    item.layout.width,
    item.layout.height,
    item.layout.label,
    item.layout.fontSize,
    item.layout.radius,
    item.layout.paddingX,
    item.layout.showMouseIcon,
    item.layout.keyIcon,
    item.layout.iconBoxWidth,
    item.layout.iconGap,
    "center",
    1,
    {
      alpha: 1,
      scale: 1,
      scaleX: 1,
      scaleY: 1,
      translateY: 0,
      wdth: held ? (isMouse ? 95 : 97) : 100,
      wght: held ? (isMouse ? 675 : 655) : 600,
      slnt: held ? (isMouse ? -12 : -2) : baseSlant,
      rond: held ? (isMouse ? 82 : 78) : baseRoundness,
      holdMix: held ? 1 : 0,
      laneWeight: 1,
    },
  );
}

export async function bakeOverlayAtlasAndPaths(
  segment: VideoSegment,
  outputWidth: number,
  outputHeight: number,
  _fps: number,
  keystrokeState: KeystrokeState,
): Promise<BakedOverlayPayload | undefined> {
  keystrokeState.keystrokeLanguage = segment.keystrokeLanguage ?? "en";
  const duration = Math.max(
    segment.trimEnd,
    ...(segment.trimSegments ?? []).map((trim) => trim.endTime),
    0,
  );
  const measurementCanvas = document.createElement("canvas");
  const measurementContext = measurementCanvas.getContext("2d");
  if (!measurementContext) {
    throw new Error("Canvas text rendering is unavailable for export");
  }

  const textPad = 24;
  const textItems: TextAtlasItem[] = getOverlayTextSegments(segment).map(
    (text) => {
      const hitArea = getTextHitArea(
        measurementContext,
        text,
        outputWidth,
        outputHeight,
      );
      return {
        text,
        hitArea,
        pad: textPad,
        width: hitArea.width + textPad * 2,
        height: hitArea.height + textPad * 2,
      };
    },
  );

  const cache = rebuildKeystrokeRenderCache(
    keystrokeState,
    segment,
    duration,
  );
  const overlayTransform = getKeystrokeOverlayTransform(
    segment,
    outputWidth,
    outputHeight,
  );
  const keystrokeEventKeys = new Map<string, string>();
  const keystrokeItems = new Map<string, KeystrokeAtlasItem>();
  for (const event of cache?.displayEvents ?? []) {
    const layout = getCachedKeystrokeBubbleLayout(
      keystrokeState,
      measurementContext,
      event,
      outputHeight,
      overlayTransform.scale,
    );
    const uniqueKey = [
      event.type,
      layout.label,
      layout.showMouseIcon ? 1 : 0,
      layout.keyIcon ?? "",
      layout.fontSize,
    ].join("|");
    keystrokeEventKeys.set(event.id, uniqueKey);
    if (!keystrokeItems.has(uniqueKey)) {
      const pad = getKeystrokeBakePadding(layout);
      keystrokeItems.set(uniqueKey, {
        uniqueKey,
        event,
        layout,
        pad,
        width: layout.width + pad * 2,
        height: layout.height + pad * 2,
      });
    }
  }

  if (textItems.length === 0 && keystrokeItems.size === 0) return undefined;

  const keystrokeList = [...keystrokeItems.values()];
  const packed = packOverlayAtlasRects([
    ...textItems.map((item) => ({ width: item.width, height: item.height })),
    ...keystrokeList.flatMap((item) => [
      { width: item.width, height: item.height },
      { width: item.width, height: item.height },
    ]),
  ]);
  let rectIndex = 0;
  for (const item of textItems) item.rect = packed.rects[rectIndex++];
  for (const item of keystrokeList) {
    item.rectNormal = packed.rects[rectIndex++];
    item.rectHeld = packed.rects[rectIndex++];
  }

  const atlasCanvas = document.createElement("canvas");
  atlasCanvas.width = packed.width;
  atlasCanvas.height = packed.height;
  const atlasContext = atlasCanvas.getContext("2d");
  if (!atlasContext) throw new Error("Canvas atlas rendering is unavailable");

  for (const item of textItems) {
    const rect = item.rect!;
    atlasContext.save();
    atlasContext.translate(
      rect.x + item.pad - item.hitArea.x,
      rect.y + item.pad - item.hitArea.y,
    );
    drawTextOverlay(atlasContext, item.text, outputWidth, outputHeight, 1);
    atlasContext.restore();
  }
  for (let index = 0; index < keystrokeList.length; index += 1) {
    const item = keystrokeList[index];
    drawKeystrokeAtlasItem(atlasContext, item, item.rectNormal!, false);
    drawKeystrokeAtlasItem(atlasContext, item, item.rectHeld!, true);
    if ((index + 1) % 10 === 0) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
  }

  const textEntries = textItems.map((item) => {
    const style = normalizeTextStyle(item.text.style);
    const layout = buildTextLayout(
      atlasContext,
      item.text,
      outputWidth,
      outputHeight,
    );
    return {
      id: item.text.id,
      startTime: item.text.startTime,
      endTime: item.text.endTime,
      rectX: item.rect!.x,
      rectY: item.rect!.y,
      rectW: item.rect!.w,
      rectH: item.rect!.h,
      hitX: item.hitArea.x,
      hitY: item.hitArea.y,
      hitW: item.hitArea.width,
      hitH: item.hitArea.height,
      pivotX: layout.pivotX,
      pivotY: layout.pivotY,
      pad: item.pad,
      animationPreset: style.animation?.preset ?? "fade",
      animationInDuration: style.animation?.inDuration ?? 0.3,
      animationOutDuration: style.animation?.outDuration ?? 0.3,
    };
  });
  const keystrokeEntries = keystrokeList.map((item) => ({
    uniqueKey: item.uniqueKey,
    normalRectX: item.rectNormal!.x,
    normalRectY: item.rectNormal!.y,
    normalRectW: item.rectNormal!.w,
    normalRectH: item.rectNormal!.h,
    heldRectX: item.rectHeld!.x,
    heldRectY: item.rectHeld!.y,
    heldRectW: item.rectHeld!.w,
    heldRectH: item.rectHeld!.h,
    layoutWidth: item.layout.width,
    layoutHeight: item.layout.height,
    layoutFontSize: item.layout.fontSize,
    layoutMarginBottom: item.layout.marginBottom,
    pad: item.pad,
    bubbleWidth: item.layout.width,
  }));
  const overlayConfig = getKeystrokeOverlayConfig(segment);
  const atlasMetadata = {
    atlasWidth: packed.width,
    atlasHeight: packed.height,
    textEntries,
    keystrokeEntries,
    keystrokeMode: cache?.mode ?? "off",
    keystrokeDelaySec: segment.keystrokeDelaySec ?? 0,
    overlayX: cache ? overlayConfig.x : DEFAULT_KEYSTROKE_OVERLAY_X,
    overlayY: cache ? overlayConfig.y : DEFAULT_KEYSTROKE_OVERLAY_Y,
    overlayScale: cache
      ? overlayConfig.scale
      : DEFAULT_KEYSTROKE_OVERLAY_SCALE,
    visibilitySegments: cache?.visibilityRef ?? [],
    displayEvents: (cache?.displayEvents ?? []).map((event) => ({
      id: event.id,
      uniqueKey: keystrokeEventKeys.get(event.id) ?? "",
      type: event.type,
      startTime: event.startTime,
      endTime: event.endTime,
      isHold: Boolean(event.isHold),
    })),
    keyboardStartTimes: cache?.keyboardStartTimes ?? [],
    keyboardIndices: cache?.keyboardIndices ?? [],
    mouseStartTimes: cache?.mouseStartTimes ?? [],
    mouseIndices: cache?.mouseIndices ?? [],
    keyboardMaxDuration: cache?.keyboardMaxDuration ?? 0,
    mouseMaxDuration: cache?.mouseMaxDuration ?? 0,
    eventSlots: cache?.eventSlots ?? [],
    eventIdentities: cache?.eventIdentities ?? [],
    keyboardSlotRepresentativeWidths: (cache?.keyboardSlotRepresentatives ?? [])
      .map((index) => {
        const event = cache?.displayEvents[index];
        if (!event) return 0;
        return getCachedKeystrokeBubbleLayout(
          keystrokeState,
          atlasContext,
          event,
          outputHeight,
          overlayConfig.scale,
        ).width;
      }),
    mouseSlotRepresentativeWidths: (cache?.mouseSlotRepresentatives ?? [])
      .map((index) => {
        const event = cache?.displayEvents[index];
        if (!event) return 0;
        return getCachedKeystrokeBubbleLayout(
          keystrokeState,
          atlasContext,
          event,
          outputHeight,
          overlayConfig.scale,
        ).width;
      }),
  };

  return {
    atlasBase64: atlasCanvas.toDataURL("image/png"),
    atlasWidth: packed.width,
    atlasHeight: packed.height,
    frames: [],
    totalFrameCount: 0,
    atlasMetadata,
  };
}
