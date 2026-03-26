/**
 * Stall detection, playback recovery anchors, and debug logging
 * for VideoController.
 *
 * Extracted so the main controller file stays focused on
 * orchestration rather than recovery heuristics.
 */

import {
  getTrimSegments,
} from "./trimSegments";
import type { VideoSegment } from "@/types/video";
import { PLAYBACK_RESET_LOG_DEDUPE_MS, PLAYBACK_RESET_DEBUG } from "./videoControllerTypes";

// ---------------------------------------------------------------------------
// Stall detection (waiting / network stall)
// ---------------------------------------------------------------------------

export interface StallState {
  waitingStallTimer: ReturnType<typeof setTimeout> | null;
  waitingStartedAt: number;
  waitingRecoveryAttempts: number;
}

export function createStallState(): StallState {
  return {
    waitingStallTimer: null,
    waitingStartedAt: 0,
    waitingRecoveryAttempts: 0,
  };
}

export function handleWaitingEvent(
  video: HTMLVideoElement,
  stall: StallState,
  onBufferingChange: ((b: boolean) => void) | undefined,
  isGeneratingThumbnail: boolean,
): void {
  if (isGeneratingThumbnail) return;
  stall.waitingStartedAt = performance.now();
  stall.waitingRecoveryAttempts = 0;
  onBufferingChange?.(true);
  console.warn(
    `[VideoController] WAITING started: readyState=${video.readyState} ` +
    `currentTime=${video.currentTime.toFixed(3)} paused=${video.paused} ` +
    `networkState=${video.networkState}`,
  );
  if (stall.waitingStallTimer !== null) clearTimeout(stall.waitingStallTimer);
  scheduleStallCheck(video, stall, 3000);
}

/**
 * Recursive stall-check timer.
 *
 * Strategy:
 *   networkState=2 (NETWORK_LOADING): browser is already fetching -- never
 *     re-seek (that would cancel and restart the range request). Just wait
 *     up to 25s total.
 *   networkState!=2 (IDLE): browser stopped fetching -- nudge it with a
 *     re-seek, then allow up to 15s for the resulting load to complete.
 */
export function scheduleStallCheck(
  video: HTMLVideoElement,
  stall: StallState,
  delayMs: number,
): void {
  stall.waitingStallTimer = setTimeout(() => {
    stall.waitingStallTimer = null;
    if (video.paused || video.readyState >= 3) return;

    const stallMs = performance.now() - stall.waitingStartedAt;
    const stallSec = (stallMs / 1000).toFixed(1);
    const networkState = video.networkState;

    if (networkState === 2 /* NETWORK_LOADING */) {
      if (stallMs < 25_000) {
        console.warn(
          `[VideoController] Still loading at ${stallSec}s (networkState=LOADING, readyState=${video.readyState}) — waiting`,
        );
        scheduleStallCheck(video, stall, 3000);
      } else {
        console.error(
          `[VideoController] STALL for ${stallSec}s: load timed out (networkState=LOADING) — forcing pause`,
        );
        video.pause();
      }
    } else {
      if (stall.waitingRecoveryAttempts < 2) {
        stall.waitingRecoveryAttempts++;
        const rescueTime = video.currentTime;
        console.warn(
          `[VideoController] STALL recovery attempt ${stall.waitingRecoveryAttempts} at ${stallSec}s: ` +
          `re-seeking to ${rescueTime.toFixed(3)} (readyState=${video.readyState})`,
        );
        video.currentTime = rescueTime;
        scheduleStallCheck(video, stall, 5000);
      } else {
        console.error(
          `[VideoController] STALL for ${stallSec}s after ${stall.waitingRecoveryAttempts} recovery attempts ` +
          `(networkState=${networkState}) — forcing pause`,
        );
        video.pause();
      }
    }
  }, delayMs);
}

export function handlePlayingEvent(
  stall: StallState,
  onBufferingChange: ((b: boolean) => void) | undefined,
): void {
  if (stall.waitingStartedAt > 0) {
    const recoveryMs = (performance.now() - stall.waitingStartedAt).toFixed(0);
    console.log(
      `[VideoController] RECOVERED from waiting in ${recoveryMs}ms` +
      `${stall.waitingRecoveryAttempts > 0 ? ` (after ${stall.waitingRecoveryAttempts} recovery attempt(s))` : ""}: ` +
      `readyState (see video element)`,
    );
    stall.waitingStartedAt = 0;
    stall.waitingRecoveryAttempts = 0;
  }
  onBufferingChange?.(false);
  if (stall.waitingStallTimer !== null) {
    clearTimeout(stall.waitingStallTimer);
    stall.waitingStallTimer = null;
  }
}

export function cleanupStallState(stall: StallState): void {
  if (stall.waitingStallTimer !== null) {
    clearTimeout(stall.waitingStallTimer);
    stall.waitingStallTimer = null;
  }
}

// ---------------------------------------------------------------------------
// Playback recovery anchor
// ---------------------------------------------------------------------------

export interface RecoveryState {
  anchorTime: number | null;
  anchorExpiresAt: number;
  retryCount: number;
}

export function createRecoveryState(): RecoveryState {
  return {
    anchorTime: null,
    anchorExpiresAt: 0,
    retryCount: 0,
  };
}

export function clearRecoveryAnchor(r: RecoveryState): void {
  r.anchorTime = null;
  r.anchorExpiresAt = 0;
  r.retryCount = 0;
}

export function armRecoveryAnchor(
  r: RecoveryState,
  time: number,
  segmentStartTime: number,
  segmentEps: number,
): void {
  if (!Number.isFinite(time)) {
    clearRecoveryAnchor(r);
    return;
  }
  if (time <= segmentStartTime + segmentEps) {
    clearRecoveryAnchor(r);
    return;
  }
  r.anchorTime = time;
  r.anchorExpiresAt = performance.now() + 1500;
  r.retryCount = 0;
}

export function maybeClearRecoveryAnchor(
  r: RecoveryState,
  currentTime: number,
): void {
  if (r.anchorTime === null) return;
  if (performance.now() > r.anchorExpiresAt) {
    clearRecoveryAnchor(r);
    return;
  }
  if (currentTime >= r.anchorTime - 0.05) {
    clearRecoveryAnchor(r);
  }
}

// ---------------------------------------------------------------------------
// Debug logging
// ---------------------------------------------------------------------------

export interface ResetLogState {
  lastResetLog: { signature: string; at: number } | null;
}

export function createResetLogState(): ResetLogState {
  return { lastResetLog: null };
}

export function logPlaybackReset(
  logState: ResetLogState,
  reason: string,
  payload: Record<string, unknown>,
): void {
  if (!PLAYBACK_RESET_DEBUG) return;
  const signature = JSON.stringify({
    reason,
    fromTime:
      typeof payload.fromTime === "number"
        ? Number(payload.fromTime.toFixed(3))
        : payload.fromTime,
    toTime:
      typeof payload.toTime === "number"
        ? Number(payload.toTime.toFixed(3))
        : payload.toTime,
    sourceChange: payload.sourceChange,
    requestedTime:
      typeof payload.requestedTime === "number"
        ? Number(payload.requestedTime.toFixed(3))
        : payload.requestedTime,
  });
  const now = Date.now();
  if (
    logState.lastResetLog &&
    logState.lastResetLog.signature === signature &&
    now - logState.lastResetLog.at < PLAYBACK_RESET_LOG_DEDUPE_MS
  ) {
    return;
  }
  logState.lastResetLog = { signature, at: now };
  console.warn("[PlaybackReset]", { reason, ...payload });
}

export function maybeLogPlaybackReset(
  logState: ResetLogState,
  reason: string,
  fromTime: number,
  toTime: number,
  segment: VideoSegment | undefined,
  segmentEps: number,
  effectiveDuration: number,
  extra: Record<string, unknown> = {},
): void {
  if (!Number.isFinite(fromTime) || !Number.isFinite(toTime)) return;
  const segmentStart = segment
    ? (getTrimSegments(segment, effectiveDuration)[0]?.startTime ?? 0)
    : 0;
  const resetToStart = toTime <= segmentStart + segmentEps;
  const meaningfulRegression =
    fromTime > segmentStart + 0.5 && fromTime - toTime > 0.5;
  if (!resetToStart || !meaningfulRegression) return;
  logPlaybackReset(logState, reason, {
    fromTime,
    toTime,
    segmentStart,
    duration: effectiveDuration,
    ...extra,
  });
}

// ---------------------------------------------------------------------------
// Segment playback bounds enforcement
// ---------------------------------------------------------------------------

/**
 * Check whether `currentTime` falls within a valid trim segment.
 * If not, jump to the next playable segment or pause at the end.
 *
 * Returns the corrected time if a jump was performed, or null if
 * currentTime is already within bounds.
 *
 * `syncAllMedia` is a callback the controller supplies to sync
 * webcam/device/mic audio to a new time.
 */
export function enforceSegmentPlaybackBounds(
  video: HTMLVideoElement,
  segment: VideoSegment,
  effectiveDuration: number,
  currentTime: number,
  forceTransitionAtEnd: boolean,
  segmentEps: number,
  syncAllMedia: (time: number) => void,
  setCurrentTime: (time: number) => void,
): number | null {
  const segs = getTrimSegments(segment, effectiveDuration);
  const last = segs[segs.length - 1];
  const TRANSITION_EPS = forceTransitionAtEnd ? 0.003 : segmentEps;

  const currentSegIndex = segs.findIndex(
    (s) =>
      currentTime >= s.startTime - segmentEps &&
      currentTime <= s.endTime + segmentEps,
  );
  const isInside = currentSegIndex >= 0;

  if (!isInside) {
    const nextTime = _getNextPlayableTime(currentTime, segment, effectiveDuration);
    if (nextTime !== null && nextTime - currentTime > segmentEps) {
      video.currentTime = nextTime;
      syncAllMedia(nextTime);
      setCurrentTime(nextTime);
      return nextTime;
    }
    if (
      nextTime !== null &&
      Math.abs(nextTime - currentTime) <= segmentEps
    ) {
      setCurrentTime(nextTime);
      return nextTime;
    }
    if (currentTime >= last.endTime - TRANSITION_EPS && !video.paused) {
      video.currentTime = last.endTime;
      syncAllMedia(last.endTime);
      setCurrentTime(last.endTime);
      video.pause();
      return last.endTime;
    }
    return null;
  }

  const currentSeg = segs[currentSegIndex];
  if (
    currentSeg &&
    currentTime >= currentSeg.endTime - TRANSITION_EPS &&
    !video.paused
  ) {
    const next = segs[currentSegIndex + 1];
    if (next && next.startTime - currentTime > segmentEps) {
      video.currentTime = next.startTime;
      syncAllMedia(next.startTime);
      setCurrentTime(next.startTime);
      return next.startTime;
    }
    if (next && Math.abs(next.startTime - currentTime) <= segmentEps) {
      setCurrentTime(next.startTime);
      return next.startTime;
    }
    video.currentTime = currentSeg.endTime;
    syncAllMedia(currentSeg.endTime);
    setCurrentTime(currentSeg.endTime);
    video.pause();
    return currentSeg.endTime;
  }

  return null;
}

// Re-import locally to avoid circular dependency with trimSegments
import { getNextPlayableTime as _getNextPlayableTime } from "./trimSegments";
