import type {
  TextSegment,
  VideoSegment,
  BakedOverlayPayload,
  OverlayFrame,
  OverlayQuad,
} from '@/types/video';
import { getTrimSegments } from '@/lib/trimSegments';
import { getSpeedAtTime } from '@/lib/exportEstimator';
import {
  type KeystrokeState,
  type KeystrokeBubbleLayout,
  clamp01,
  rebuildKeystrokeRenderCache,
  getKeystrokeOverlayTransform,
  getKeystrokeOverlayConfig,
  getCachedKeystrokeBubbleLayout,
  drawKeystrokeBubble,
  buildActiveKeystrokeFrameLayout,
  getKeystrokeDelaySec,
  DEFAULT_KEYSTROKE_OVERLAY_X,
  DEFAULT_KEYSTROKE_OVERLAY_Y,
  DEFAULT_KEYSTROKE_OVERLAY_SCALE,
} from './keystrokeRenderer';
import { normalizeTextStyle } from '@/lib/textStyleDefaults';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import {
  applyAnimationToRect,
  buildTextLayout,
  drawTextOverlay,
  getTextAnimationState,
  getTextHitArea,
} from './overlayTextRenderer';

function getOverlayTextSegments(segment: VideoSegment): TextSegment[] {
  return [...getVisibleSubtitleSegments(segment), ...(segment.textSegments ?? [])];
}

// ---------------------------------------------------------------------------
// Keystroke bake padding helper
// ---------------------------------------------------------------------------

export function getKeystrokeBakePadding(layout: KeystrokeBubbleLayout): number {
  return Math.max(28, Math.round(layout.fontSize * 1.35));
}

// ---------------------------------------------------------------------------
// Overlay atlas baking (main export function)
// ---------------------------------------------------------------------------

/**
 * Bake all text and keystroke overlays into a single sprite atlas and compute
 * per-frame quad arrays for GPU compositing. Replaces the old per-bitmap bakers.
 */
export async function bakeOverlayAtlasAndPaths(
  segment: VideoSegment,
  outputWidth: number,
  outputHeight: number,
  fps: number = 60,
  keystrokeState: KeystrokeState
): Promise<BakedOverlayPayload> {
  keystrokeState.keystrokeLanguage = segment.keystrokeLanguage ?? 'en';
  const duration = Math.max(
    segment.trimEnd,
    ...(segment.trimSegments || []).map(s => s.endTime),
    0
  );

  const MAX_ATLAS_SIZE = 4096;
  const atlasCanvas = document.createElement('canvas');
  atlasCanvas.width = MAX_ATLAS_SIZE;
  atlasCanvas.height = MAX_ATLAS_SIZE;
  atlasCanvas.style.cssText = 'position:fixed;left:-9999px;top:-9999px;pointer-events:none;';
  document.body.appendChild(atlasCanvas);

  const atlasCtx = atlasCanvas.getContext('2d', { willReadFrequently: true });
  if (!atlasCtx) {
    atlasCanvas.remove();
    return { atlasBase64: '', atlasWidth: 1, atlasHeight: 1, frames: [], totalFrameCount: 0 };
  }

  let packX = 0;
  let packY = 0;
  let rowH = 0;
  const pack = (w: number, h: number) => {
    if (packX + w > MAX_ATLAS_SIZE) { packX = 0; packY += rowH + 2; rowH = 0; }
    const rect = { x: packX, y: packY, w, h };
    packX += w + 2;
    rowH = Math.max(rowH, h);
    return rect;
  };

  type AtlasRect = { x: number; y: number; w: number; h: number };
  const textMap = new Map<string, { rect: AtlasRect; baseHitArea: { x: number; y: number; width: number; height: number }; pad: number }>();

  // Pack text overlays
  const textPad = 24;
  for (const text of getOverlayTextSegments(segment)) {
    const hitArea = getTextHitArea(atlasCtx, text, outputWidth, outputHeight);
    const w = Math.ceil(hitArea.width + textPad * 2);
    const h = Math.ceil(hitArea.height + textPad * 2);
    const rect = pack(w, h);
    atlasCtx.save();
    atlasCtx.translate(rect.x + textPad - hitArea.x, rect.y + textPad - hitArea.y);
    drawTextOverlay(atlasCtx, text, outputWidth, outputHeight, 1.0);
    atlasCtx.restore();
    textMap.set(text.id, { rect, baseHitArea: hitArea, pad: textPad });
  }

  // Pack keystroke overlays -- dual-state baking (normal + held) per unique bubble.
  // keystrokeUniqueMap: uniqueKey -> {rectNormal, rectHeld, layout, pad}
  // keystrokeEventMap:  eventId  -> uniqueKey
  const keystrokeUniqueMap = new Map<string, { rectNormal: AtlasRect; rectHeld: AtlasRect; layout: KeystrokeBubbleLayout; pad: number }>();
  const keystrokeEventMap = new Map<string, string>(); // eventId -> uniqueKey
  const cache = rebuildKeystrokeRenderCache(keystrokeState, segment, duration);
  if (cache && cache.displayEvents.length > 0) {
    const overlayTransform = getKeystrokeOverlayTransform(segment, outputWidth, outputHeight);
    let uniqueCount = 0;
    for (const event of cache.displayEvents) {
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, event, outputHeight, overlayTransform.scale);
      const uniqueKey = `${layout.label}|${layout.showMouseIcon}|${layout.keyIcon ?? ''}|${layout.fontSize}`;
      keystrokeEventMap.set(event.id, uniqueKey);
      if (!keystrokeUniqueMap.has(uniqueKey)) {
        const pad = getKeystrokeBakePadding(layout);
        const w = layout.width + pad * 2;
        const h = layout.height + pad * 2;

        const isMouse = event.type === 'mousedown' || event.type === 'wheel';
        const baseSlnt = isMouse ? -6 : 0;
        const baseRond = isMouse ? 96 : 88;

        // Bake Normal state (holdMix = 0)
        const rectNormal = pack(w, h);
        atlasCtx.clearRect(rectNormal.x, rectNormal.y, w, h);
        drawKeystrokeBubble(
          atlasCtx, event,
          rectNormal.x + pad, rectNormal.y + pad,
          layout.width, layout.height,
          layout.label, layout.fontSize, layout.radius, layout.paddingX,
          layout.showMouseIcon, layout.keyIcon, layout.iconBoxWidth, layout.iconGap,
          'center', 1.0,
          { alpha: 1, scale: 1, scaleX: 1, scaleY: 1, translateY: 0, wdth: 100, wght: 600, slnt: baseSlnt, rond: baseRond, holdMix: 0, laneWeight: 1 }
        );

        // Bake Held state (holdMix = 1) -- saturated color, slant, narrower width
        const rectHeld = pack(w, h);
        atlasCtx.clearRect(rectHeld.x, rectHeld.y, w, h);
        drawKeystrokeBubble(
          atlasCtx, event,
          rectHeld.x + pad, rectHeld.y + pad,
          layout.width, layout.height,
          layout.label, layout.fontSize, layout.radius, layout.paddingX,
          layout.showMouseIcon, layout.keyIcon, layout.iconBoxWidth, layout.iconGap,
          'center', 1.0,
          { alpha: 1, scale: 1, scaleX: 1, scaleY: 1, translateY: 0, wdth: isMouse ? 95 : 97, wght: isMouse ? 675 : 655, slnt: isMouse ? -12 : -2, rond: isMouse ? 82 : 78, holdMix: 1, laneWeight: 1 }
        );

        keystrokeUniqueMap.set(uniqueKey, { rectNormal, rectHeld, layout, pad });
        uniqueCount++;
        // Yield to UI every 10 unique renders so the browser stays responsive.
        if (uniqueCount % 10 === 0) await new Promise(r => setTimeout(r, 0));
      }
    }
  }

  const actualAtlasHeight = Math.max(1, packY + rowH + 2);
  // The atlas is transferred to the backend as a PNG base64 string over IPC and
  // decoded in Rust; the raw RGBA bytes below are kept alongside it for callers
  // that want the pre-encoded pixels without re-decoding the PNG.
  const atlasRgba = atlasCtx.getImageData(0, 0, MAX_ATLAS_SIZE, actualAtlasHeight).data;
  const finalCanvas = document.createElement('canvas');
  finalCanvas.width = MAX_ATLAS_SIZE;
  finalCanvas.height = actualAtlasHeight;
  finalCanvas.getContext('2d')!.drawImage(atlasCanvas, 0, 0);
  const atlasBase64 = finalCanvas.toDataURL('image/png');
  atlasCanvas.remove();
  finalCanvas.remove();

  // Build compact atlas metadata for Rust-side frame quad generation.
  // This eliminates the need to send 40K+ frame objects over IPC.
  const overlayConfig = getKeystrokeOverlayConfig(segment);
  const textEntries = Array.from(textMap.entries()).map(([id, m]) => {
    const text = getOverlayTextSegments(segment).find(t => t.id === id);
    const style = text ? normalizeTextStyle(text.style) : null;
    const layout = text ? (() => {
      atlasCtx.save();
      const result = buildTextLayout(atlasCtx, text, outputWidth, outputHeight);
      atlasCtx.restore();
      return result;
    })() : null;
    return {
      id,
      startTime: text?.startTime ?? 0,
      endTime: text?.endTime ?? 0,
      rectX: m.rect.x,
      rectY: m.rect.y,
      rectW: m.rect.w,
      rectH: m.rect.h,
      hitX: m.baseHitArea.x,
      hitY: m.baseHitArea.y,
      hitW: m.baseHitArea.width,
      hitH: m.baseHitArea.height,
      pivotX: layout?.pivotX ?? (m.baseHitArea.x + m.baseHitArea.width / 2),
      pivotY: layout?.pivotY ?? (m.baseHitArea.y + m.baseHitArea.height / 2),
      pad: m.pad,
      animationPreset: style?.animation?.preset ?? 'fade',
      animationInDuration: style?.animation?.inDuration ?? 0.3,
      animationOutDuration: style?.animation?.outDuration ?? 0.3,
    };
  });
  const keystrokeEntries = Array.from(keystrokeUniqueMap.entries()).map(([uniqueKey, m]) => ({
    uniqueKey,
    normalRectX: m.rectNormal.x,
    normalRectY: m.rectNormal.y,
    normalRectW: m.rectNormal.w,
    normalRectH: m.rectNormal.h,
    heldRectX: m.rectHeld.x,
    heldRectY: m.rectHeld.y,
    heldRectW: m.rectHeld.w,
    heldRectH: m.rectHeld.h,
    layoutWidth: m.layout.width,
    layoutHeight: m.layout.height,
    layoutFontSize: m.layout.fontSize,
    layoutMarginBottom: m.layout.marginBottom,
    pad: m.pad,
    bubbleWidth: m.layout.width,
  }));

  const atlasMetadata = cache ? {
    atlasWidth: MAX_ATLAS_SIZE,
    atlasHeight: actualAtlasHeight,
    textEntries,
    keystrokeEntries,
    keystrokeMode: segment.keystrokeMode ?? 'off',
    keystrokeDelaySec: segment.keystrokeDelaySec ?? 0,
    overlayX: overlayConfig.x,
    overlayY: overlayConfig.y,
    overlayScale: overlayConfig.scale,
    visibilitySegments: cache.visibilityRef ?? [],
    displayEvents: cache.displayEvents.map(e => ({
      id: e.id,
      uniqueKey: keystrokeEventMap.get(e.id) ?? '',
      type: e.type,
      startTime: e.startTime,
      endTime: e.endTime,
      isHold: Boolean(e.isHold),
    })),
    keyboardStartTimes: cache.keyboardStartTimes,
    keyboardIndices: cache.keyboardIndices,
    mouseStartTimes: cache.mouseStartTimes,
    mouseIndices: cache.mouseIndices,
    keyboardMaxDuration: cache.keyboardMaxDuration,
    mouseMaxDuration: cache.mouseMaxDuration,
    eventSlots: cache.eventSlots,
    eventIdentities: cache.eventIdentities,
    keyboardSlotRepresentativeWidths: cache.keyboardSlotRepresentatives.map(idx => {
      if (typeof idx !== 'number') return 0;
      const ev = cache.displayEvents[idx];
      if (!ev) return 0;
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, ev, outputHeight, overlayConfig.scale);
      return layout.width;
    }),
    mouseSlotRepresentativeWidths: cache.mouseSlotRepresentatives.map(idx => {
      if (typeof idx !== 'number') return 0;
      const ev = cache.displayEvents[idx];
      if (!ev) return 0;
      const layout = getCachedKeystrokeBubbleLayout(keystrokeState, atlasCtx, ev, outputHeight, overlayConfig.scale);
      return layout.width;
    }),
  } : (textEntries.length > 0 ? {
    atlasWidth: MAX_ATLAS_SIZE,
    atlasHeight: actualAtlasHeight,
    textEntries,
    keystrokeEntries: [],
    keystrokeMode: 'off',
    keystrokeDelaySec: 0,
    overlayX: DEFAULT_KEYSTROKE_OVERLAY_X,
    overlayY: DEFAULT_KEYSTROKE_OVERLAY_Y,
    overlayScale: DEFAULT_KEYSTROKE_OVERLAY_SCALE,
    visibilitySegments: [],
    displayEvents: [],
    keyboardStartTimes: [],
    keyboardIndices: [],
    mouseStartTimes: [],
    mouseIndices: [],
    keyboardMaxDuration: 0,
    mouseMaxDuration: 0,
    eventSlots: [],
    eventIdentities: [],
    keyboardSlotRepresentativeWidths: [],
    mouseSlotRepresentativeWidths: [],
  } : null);

  // When metadata is available, skip the expensive JS frame loop entirely.
  // Rust will generate overlay frames from the metadata in ~1ms.
  if (atlasMetadata) {
    return {
      atlasBase64,
      atlasRgba: new Uint8Array(atlasRgba.buffer),
      atlasWidth: MAX_ATLAS_SIZE,
      atlasHeight: actualAtlasHeight,
      frames: [],
      totalFrameCount: 0,
      atlasMetadata,
    };
  }

  // Fallback: generate per-frame quad arrays in JS (used by composition export
  // or when metadata is not available).
  // Generate per-frame quad arrays.
  // IMPORTANT: This loop must mirror gpu_pipeline.rs `build_frame_times` exactly so that
  // frames[i] corresponds to output frame i. The Rust compositor indexes overlay_frames by
  // frame_idx directly — so any mismatch here causes keystrokes to be invisible at non-1x
  // speed segments (at 1x speed source_time == output_time, masking the bug).
  //
  // Algorithm (identical to Rust build_frame_times):
  //   current_source_time starts at trimSegments[0].startTime
  //   per output frame: advance by clamp(speed(t), 0.1, 16) * (1/fps)
  //   when current_source_time crosses a segment boundary: jump to next segment's startTime
  const frames: OverlayFrame[] = [];
  const outDt = 1 / fps;
  const speedPoints = segment.speedPoints || [];
  const trimSegments = getTrimSegments(segment, duration);
  const endTime = trimSegments[trimSegments.length - 1].endTime;
  const delaySec = getKeystrokeDelaySec(segment);
  let segIdx = 0;
  let t = trimSegments[0].startTime;
  let frameCount = 0;

  while (t < endTime - 1e-9) {
    // Advance to the next trim segment if source time has passed the current segment's end
    // (mirrors the inner while in Rust build_frame_times)
    while (segIdx < trimSegments.length && t >= trimSegments[segIdx].endTime) {
      segIdx++;
      if (segIdx < trimSegments.length) {
        t = trimSegments[segIdx].startTime;
      }
    }
    if (segIdx >= trimSegments.length) break;

    const quads: OverlayQuad[] = [];

    for (const text of getOverlayTextSegments(segment)) {
      if (t >= text.startTime && t <= text.endTime) {
        const animation = getTextAnimationState(text, t);
        const mapping = textMap.get(text.id);
        if (mapping && animation.alpha > 0.001) {
          const animatedRect = applyAnimationToRect(
            {
              x: mapping.baseHitArea.x - mapping.pad,
              y: mapping.baseHitArea.y - mapping.pad,
              width: mapping.rect.w,
              height: mapping.rect.h,
            },
            mapping.baseHitArea.x + mapping.baseHitArea.width / 2,
            mapping.baseHitArea.y + mapping.baseHitArea.height / 2,
            animation,
          );
          quads.push({
            x: animatedRect.x,
            y: animatedRect.y,
            w: animatedRect.width,
            h: animatedRect.height,
            u: mapping.rect.x / MAX_ATLAS_SIZE,
            v: mapping.rect.y / actualAtlasHeight,
            uw: mapping.rect.w / MAX_ATLAS_SIZE,
            vh: mapping.rect.h / actualAtlasHeight,
            alpha: animation.alpha,
          });
        }
      }
    }

    if (cache) {
      const layout = buildActiveKeystrokeFrameLayout(keystrokeState, atlasCtx, segment, cache, t, delaySec, outputWidth, outputHeight);
      const drawPlacements = (placements: any[]) => {
        for (const p of placements) {
          const uniqueKey = keystrokeEventMap.get(p.item.active.event.id);
          const mapping = uniqueKey ? keystrokeUniqueMap.get(uniqueKey) : undefined;
          if (!mapping) continue;
          const visual = p.item.visual;
          if (visual.alpha <= 0.001) continue;
          const baseW = p.item.layout.width + mapping.pad * 2;
          const baseH = p.item.layout.height + mapping.pad * 2;
          const drawW = baseW * visual.scale * visual.scaleX;
          const drawH = baseH * visual.scale * visual.scaleY;
          const cx = p.x + p.item.bubbleWidth / 2;
          const cy = p.y + p.item.layout.height / 2 + visual.translateY;
          const quadX = cx - drawW / 2;
          const quadY = cy - drawH / 2;
          const mix = clamp01(visual.holdMix);

          // Crossfade two opaque states (Normal + Held) using Premultiplied SrcOver.
          // Math: A_total = A_held + A_normal - A_held * A_normal.
          // Solving for A_total = visual.alpha gives the alphaNormal coefficient below.
          const alphaHeld = visual.alpha * mix;
          const alphaNormal = alphaHeld >= 0.999 ? 0 : (visual.alpha * (1 - mix)) / (1 - alphaHeld);
          if (alphaNormal > 0.001) {
            quads.push({
              x: quadX,
              y: quadY,
              w: drawW,
              h: drawH,
              u: mapping.rectNormal.x / MAX_ATLAS_SIZE,
              v: mapping.rectNormal.y / actualAtlasHeight,
              uw: mapping.rectNormal.w / MAX_ATLAS_SIZE,
              vh: mapping.rectNormal.h / actualAtlasHeight,
              alpha: alphaNormal,
            });
          }
          if (alphaHeld > 0.001) {
            quads.push({
              x: quadX,
              y: quadY,
              w: drawW,
              h: drawH,
              u: mapping.rectHeld.x / MAX_ATLAS_SIZE,
              v: mapping.rectHeld.y / actualAtlasHeight,
              uw: mapping.rectHeld.w / MAX_ATLAS_SIZE,
              vh: mapping.rectHeld.h / actualAtlasHeight,
              alpha: alphaHeld,
            });
          }
        }
      };
      drawPlacements(layout.keyboard);
      drawPlacements(layout.mouse);
    }

    // Only emit non-empty frames (sparse output) — Rust expands to dense array.
    if (quads.length > 0) {
      frames.push({ frameIndex: frameCount, quads });
    }
    frameCount++;
    // Yield every 500 frames to keep the browser responsive during long exports.
    if (frameCount % 500 === 0) await new Promise(r => setTimeout(r, 0));

    // Advance source time by speed-adjusted output step — identical to Rust build_frame_times:
    //   speed = get_speed(current_source_time, speed_points).clamp(0.1, 16.0)
    //   current_source_time += speed * out_dt
    const speed = Math.max(0.1, Math.min(16.0, getSpeedAtTime(t, speedPoints)));
    t += speed * outDt;
  }

  return { atlasBase64, atlasWidth: MAX_ATLAS_SIZE, atlasHeight: actualAtlasHeight, frames, totalFrameCount: frameCount, atlasMetadata: null };
}
