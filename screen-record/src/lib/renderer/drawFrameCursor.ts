import { BackgroundConfig, MousePosition, VideoSegment } from '@/types/video';
import {
  getCursorProcessingSignature,
  processCursorPositions,
  interpolateCursorPositionInternal,
} from './cursorDynamics';
import { normalizeMousePositionsToVideoSpace } from '@/lib/dynamicCapture';
import { logPreviewCursorState } from '@/lib/cursorDebug';
import { getKeystrokeDelaySec } from './keystrokeRenderer';
import type { RendererState } from './drawFrame';

// Constants
export const CLICK_FUSE_THRESHOLD  = 0.05;
export const SQUISH_TARGET          = 0.75;
export const SQUISH_DOWN_DUR_BASE   = 0.10;  // comfortable press when click is isolated
export const SQUISH_DOWN_DUR_MIN    = 0.04;  // rushed press when previous click was close
export const RELEASE_DUR_BASE       = 0.15;  // comfortable spring-back when no next click is close
export const RELEASE_DUR_MIN        = 0.04;  // rushed spring-back when next click is imminent

// Ease-out cubic: fast initial response, smooth arrival
export function squishEaseDown(t: number): number {
  return 1 - Math.pow(1 - t, 3);
}
// Spring-back easing: subtle overshoot (springy) when there's room,
// plain ease-out cubic when the gap to the next click is tight
export function squishEaseUp(t: number, hasRoom: boolean): number {
  if (!hasRoom) return 1 - Math.pow(1 - t, 3);
  const c = 1.2; // overshoot ~5%
  return 1 + (c + 1) * Math.pow(t - 1, 3) + c * Math.pow(t - 1, 2);
}

// ---------------------------------------------------------------------------
// interpolateCursorPosition - cached wrapper around cursorDynamics
// ---------------------------------------------------------------------------

export function interpolateCursorPosition(
  currentTime: number,
  mousePositions: MousePosition[],
  state: RendererState,
  fallbackWidth: number,
  fallbackHeight: number,
  backgroundConfig?: BackgroundConfig | null,
): { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null {
  const normalizationSignature = `${fallbackWidth}x${fallbackHeight}`;
  const processSignature = getCursorProcessingSignature(backgroundConfig);

  if (
    state.lastMousePositionsRef !== mousePositions ||
    state.lastCursorProcessSignature !== processSignature ||
    state.lastCursorNormalizationSignature !== normalizationSignature
  ) {
    state.processedCursorPositions = null;
    state.lastMousePositionsRef = mousePositions;
    state.lastCursorProcessSignature = processSignature;
    state.lastCursorNormalizationSignature = normalizationSignature;
    state.lastCursorPreviewDebugSignature = '';
    state.lastCursorPreviewDebugBucket = -1;
    state.lastCursorPreviewDebugPoint = null;
  }

  if (!state.processedCursorPositions && mousePositions.length > 0) {
    const normalizedMousePositions = normalizeMousePositionsToVideoSpace(
      mousePositions,
      fallbackWidth,
      fallbackHeight
    );
    state.processedCursorPositions = processCursorPositions(normalizedMousePositions, backgroundConfig);
  }

  const dataToUse = state.processedCursorPositions || mousePositions;
  return interpolateCursorPositionInternal(currentTime, dataToUse);
}

export function logPreviewCursorDebug(
  state: RendererState,
  currentTime: number,
  cursorTime: number,
  interpolatedPosition: { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null,
  showCursor: boolean,
  cursorVis: { opacity: number; scale: number },
  segment: VideoSegment
): void {
  const point = interpolatedPosition ? { x: interpolatedPosition.x, y: interpolatedPosition.y } : null;
  const deltaPx = point && state.lastCursorPreviewDebugPoint
    ? Math.hypot(point.x - state.lastCursorPreviewDebugPoint.x, point.y - state.lastCursorPreviewDebugPoint.y)
    : null;
  const motionState = !point ? 'missing' : (deltaPx !== null && deltaPx >= 0.75 ? 'moving' : 'stopped');
  const visibilityReason = segment.useCustomCursor === false
    ? 'custom-cursor-disabled'
    : !point
      ? 'no-sampled-position'
      : segment.cursorVisibilitySegments === undefined
        ? 'smart-pointer-off'
        : segment.cursorVisibilitySegments.length === 0
          ? 'no-visible-segments'
          : showCursor
            ? 'inside-visible-segment'
            : 'outside-visible-segment';
  const signature = [
    motionState,
    showCursor ? 'show' : 'hide',
    visibilityReason,
    interpolatedPosition?.cursor_type || 'none',
    interpolatedPosition?.isClicked ? 'click' : 'noclick',
  ].join('|');
  const debugBucket = motionState === 'moving' ? Math.floor(currentTime * 4) : -1;
  const shouldLog =
    signature !== state.lastCursorPreviewDebugSignature ||
    (motionState === 'moving' && debugBucket !== state.lastCursorPreviewDebugBucket);

  if (shouldLog) {
    logPreviewCursorState({
      previewTime: Math.round(currentTime * 1000) / 1000,
      cursorSampleTime: Math.round(cursorTime * 1000) / 1000,
      x: point ? Math.round(point.x * 100) / 100 : null,
      y: point ? Math.round(point.y * 100) / 100 : null,
      deltaPx: deltaPx !== null ? Math.round(deltaPx * 1000) / 1000 : null,
      motionState,
      visible: showCursor,
      visibilityReason,
      opacity: Math.round(cursorVis.opacity * 1000) / 1000,
      scale: Math.round(cursorVis.scale * 1000) / 1000,
      clicked: Boolean(interpolatedPosition?.isClicked),
      cursorType: interpolatedPosition?.cursor_type || 'none',
      segmentCount: segment.cursorVisibilitySegments?.length ?? null,
    });
  }

  state.lastCursorPreviewDebugSignature = signature;
  state.lastCursorPreviewDebugBucket = debugBucket;
  state.lastCursorPreviewDebugPoint = point;
}

// ---------------------------------------------------------------------------
// updateSquishAnimation - squish animation state machine
// ---------------------------------------------------------------------------

export function updateSquishAnimation(
  state: RendererState,
  video: HTMLVideoElement,
  segment: VideoSegment,
  interpolatedPosition: { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number },
): void {
  const keystrokeDelaySec = getKeystrokeDelaySec(segment);
  const lookupTime = video.currentTime - keystrokeDelaySec;
  const events = segment.keystrokeEvents || [];

  // Find the currently active click event via binary search + local scan.
  // Quick clicks: snappy 0.1s detection window. Holds: stay squished until physical release.
  let activeEvent: typeof events[number] | null = null;
  {
    // Binary search for approximate position in events (sorted by startTime)
    let elo = 0, ehi = events.length;
    while (elo < ehi) { const mid = (elo + ehi) >> 1; if (events[mid].startTime <= lookupTime) elo = mid + 1; else ehi = mid; }
    // Scan backward from insertion point to find active mousedown
    for (let ei = elo - 1; ei >= 0; ei--) {
      const e = events[ei];
      if (lookupTime - e.startTime > 1) break; // events are short, stop scanning
      if (e.type === 'mousedown' && lookupTime >= e.startTime &&
          lookupTime <= (e.isHold ? e.endTime : e.startTime + 0.1)) {
        activeEvent = e;
        break;
      }
    }
  }
  const isActuallyClicked = !!activeEvent;
  // Propagate so resolveCursorRenderType (grab/closehand icon) also sees this
  interpolatedPosition.isClicked = isActuallyClicked;

  // Fuse: briefly stay squished after release so spring-back is perceivable
  const prevLastHoldTime = state.lastHoldTime; // capture before update, used in snap guard below
  if (isActuallyClicked) state.lastHoldTime = video.currentTime;
  const timeSinceLastHold = video.currentTime - state.lastHoldTime;
  const shouldBeSquished = isActuallyClicked ||
    (state.lastHoldTime >= 0 && timeSinceLastHold < CLICK_FUSE_THRESHOLD);
  const targetScale = shouldBeSquished ? SQUISH_TARGET : 1.0;

  const activeEventId = activeEvent?.id ?? null;
  const isNewClick = activeEventId !== null && activeEventId !== state.lastActiveEventId;
  state.lastActiveEventId = activeEventId;

  // Start a new animation segment on target change or new click.
  // All gap lookups happen here (once per segment start) so easing stays consistent.
  if (targetScale !== state.squishTarget || isNewClick) {
    if (isNewClick && state.currentSquishScale < 0.95 && prevLastHoldTime >= 0) {
      // Rapid re-click while already squished from a prior click: snap to 1.0 so each
      // click gets its own pulse. Guard with prevLastHoldTime >= 0 so the very first
      // click of a fresh session never triggers a spurious snap-up.
      state.currentSquishScale = 1.0;
    }
    state.squishAnimFrom = state.currentSquishScale;
    state.squishTarget = targetScale;
    state.squishAnimProgress = 0;

    if (targetScale < state.squishAnimFrom) {
      // -- SQUISH DOWN --
      // Adapt press speed to gap from the previous click:
      // isolated click -> comfortable; rapid sequence -> faster to fit the B-side gap
      let prevEvent: typeof events[number] | null = null;
      {
        const threshold = (activeEvent?.startTime ?? lookupTime) - 0.01;
        for (let ei = events.length - 1; ei >= 0; ei--) {
          if (events[ei].type === 'mousedown' && events[ei].startTime < threshold) {
            prevEvent = events[ei]; break;
          }
        }
      }
      const prevEffectiveEnd = prevEvent
        ? (prevEvent.isHold ? prevEvent.endTime : prevEvent.startTime + 0.1)
        : -Infinity;
      const gapFromPrev = activeEvent
        ? Math.max(0, activeEvent.startTime - prevEffectiveEnd)
        : Infinity;
      state.squishAnimDuration = isFinite(gapFromPrev) && gapFromPrev < SQUISH_DOWN_DUR_BASE * 2
        ? Math.max(SQUISH_DOWN_DUR_MIN, gapFromPrev * 0.4)
        : SQUISH_DOWN_DUR_BASE;
      state.squishHasRoom = false; // unused for down-easing; keep it clean

    } else {
      // -- SPRING BACK --
      // Only animate if we're actually coming out of a real recent click.
      // If the user seeked or there's no click context, snap instantly.
      const recentClick = state.lastHoldTime >= 0 &&
        video.currentTime >= state.lastHoldTime &&
        video.currentTime - state.lastHoldTime < CLICK_FUSE_THRESHOLD + 0.1;

      if (!recentClick) {
        state.squishAnimProgress = 1; // snap -- no click context
      } else {
        // Adapt release speed to gap toward the next click:
        // isolated click -> comfortable + springy overshoot;
        // next click coming soon -> faster + no overshoot
        const activeEffectiveEnd = activeEvent
          ? (activeEvent.isHold ? activeEvent.endTime : activeEvent.startTime + 0.1)
          : lookupTime;
        let nextEvent: typeof events[number] | null = null;
        {
          const threshold = (activeEvent?.startTime ?? lookupTime) + 0.01;
          for (let ei = 0; ei < events.length; ei++) {
            if (events[ei].type === 'mousedown' && events[ei].startTime > threshold) {
              nextEvent = events[ei]; break;
            }
          }
        }
        const gapToNext = nextEvent
          ? Math.max(0, nextEvent.startTime - activeEffectiveEnd)
          : Infinity;
        state.squishHasRoom = gapToNext > RELEASE_DUR_BASE * 2;
        state.squishAnimDuration = isFinite(gapToNext) && gapToNext < RELEASE_DUR_BASE * 2
          ? Math.max(RELEASE_DUR_MIN, gapToNext * 0.5)
          : RELEASE_DUR_BASE;
      }
    }
  }

  // Advance animation by wall-clock elapsed; easing params are locked in at segment start
  if (state.squishAnimProgress < 1) {
    const elapsedSec = state.latestElapsed / 1000;
    state.squishAnimProgress = Math.min(1, state.squishAnimProgress + elapsedSec / state.squishAnimDuration);
    const t = state.squishAnimProgress;
    const goingDown = state.squishTarget < state.squishAnimFrom;
    const eased = goingDown ? squishEaseDown(t) : squishEaseUp(t, state.squishHasRoom);
    state.currentSquishScale = state.squishAnimFrom + (state.squishTarget - state.squishAnimFrom) * eased;
  } else {
    state.currentSquishScale = state.squishTarget;
  }

  // Sync to CursorRenderState -- drawCursorShape reads from there, not RendererState
  state.cursorState.currentSquishScale = state.currentSquishScale;
}
