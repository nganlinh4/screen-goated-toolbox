/**
 * Playback control logic for VideoController.
 *
 * Contains the implementations for play, seek, flushPendingSeek,
 * and the handleSeeked / handleTimeUpdate heavy-lifting that was
 * making the main class file too large.
 */

import { videoRenderer } from "./videoRenderer";
import {
  clampToTrimSegments,
  getNextPlayableTime,
  getTrimSegments,
} from "./trimSegments";
import type { RenderOptions, VideoState } from "./videoControllerTypes";
import {
  syncAudioElementPlaybackRate,
  syncTimedMediaPlayback,
  playAudioElement,
  pauseAudioElement,
} from "./videoControllerMediaSync";
import {
  type RecoveryState,
  type ResetLogState,
  type StallState,
  clearRecoveryAnchor,
  maybeClearRecoveryAnchor,
  logPlaybackReset,
  maybeLogPlaybackReset,
  enforceSegmentPlaybackBounds,
  handleWaitingEvent,
  handlePlayingEvent,
} from "./videoControllerStallRecovery";

// ---------------------------------------------------------------------------
// Narrow interface the extracted functions need from the controller.
// This avoids making all private fields public on VideoController itself.
// ---------------------------------------------------------------------------

export interface ControllerInternals {
  video: HTMLVideoElement;
  webcamVideo?: HTMLVideoElement;
  deviceAudio?: HTMLAudioElement;
  micAudio?: HTMLAudioElement;
  canvas: HTMLCanvasElement;
  tempCanvas: HTMLCanvasElement;
  state: VideoState;
  renderOptions?: RenderOptions;
  isChangingSource: boolean;
  isGeneratingThumbnail: boolean;
  pendingSeekTime: number | null;
  lastRequestedSeekTime: number | null;
  SEGMENT_EPS: number;
  playRequestSeq: number;
  webcamVideoPlayPromise: Promise<void> | null;
  deviceAudioPlayPromise: Promise<void> | null;
  micAudioPlayPromise: Promise<void> | null;
  stallState: StallState;
  recoveryState: RecoveryState;
  resetLogState: ResetLogState;

  // Callback helpers the controller must provide
  hasValidWebcamVideo: boolean;
  hasValidMicAudio: boolean;
  hasValidDeviceAudio: boolean;
  hasExternalAudio: boolean;
  getWebcamOffsetSec(): number;
  getMicAudioOffsetSec(): number;
  getSpeed(time: number): number;
  getEffectiveDuration(fallback: number): number;
  getSegmentStartTime(referenceTime: number): number;
  syncAllMediaToTime(time: number): void;
  applyAudioTrackVolumes(time?: number): void;
  setPlaying(playing: boolean): void;
  setReady(ready: boolean): void;
  setSeeking(seeking: boolean): void;
  setCurrentTime(time: number): void;
  renderFrame(): void;
  armPlaybackRecoveryAnchor(time: number): void;
  onBufferingChange?: (b: boolean) => void;
  startPlaybackMonitor(): void;
  stopPlaybackMonitor(): void;
}

// ---------------------------------------------------------------------------
// handlePlay
// ---------------------------------------------------------------------------

export function doHandlePlay(c: ControllerInternals): void {
  c.applyAudioTrackVolumes(c.video.currentTime);
  c.syncAllMediaToTime(c.video.currentTime);
  syncAudioElementPlaybackRate(c.webcamVideo, c.video.playbackRate);
  syncAudioElementPlaybackRate(c.deviceAudio, c.video.playbackRate);
  syncAudioElementPlaybackRate(c.micAudio, c.video.playbackRate);
  c.webcamVideoPlayPromise = syncTimedMediaPlayback(
    c.webcamVideo,
    c.hasValidWebcamVideo,
    c.webcamVideoPlayPromise,
    c.video.currentTime,
    c.getWebcamOffsetSec(),
  );
  c.deviceAudioPlayPromise = playAudioElement(c.deviceAudio);
  c.micAudioPlayPromise = syncTimedMediaPlayback(
    c.micAudio,
    c.hasValidMicAudio,
    c.micAudioPlayPromise,
    c.video.currentTime,
    c.getMicAudioOffsetSec(),
  );

  if (c.renderOptions) {
    videoRenderer.startAnimation({
      video: c.video,
      webcamVideo: c.webcamVideo,
      canvas: c.canvas,
      tempCanvas: c.tempCanvas,
      segment: c.renderOptions.segment,
      backgroundConfig: c.renderOptions.backgroundConfig,
      webcamConfig: c.renderOptions.webcamConfig,
      mousePositions: c.renderOptions.mousePositions,
      currentTime: c.video.currentTime,
      interactiveBackgroundPreview:
        c.renderOptions.interactiveBackgroundPreview,
    });
  }

  c.startPlaybackMonitor();
  c.setPlaying(true);
}

// ---------------------------------------------------------------------------
// handlePause
// ---------------------------------------------------------------------------

export function doHandlePause(c: ControllerInternals): void {
  c.playRequestSeq += 1;
  clearRecoveryAnchor(c.recoveryState);
  c.onBufferingChange?.(false);
  pauseAudioElement(c.webcamVideo, c.webcamVideoPlayPromise);
  pauseAudioElement(c.deviceAudio, c.deviceAudioPlayPromise);
  pauseAudioElement(c.micAudio, c.micAudioPlayPromise);
  c.webcamVideoPlayPromise = null;
  c.deviceAudioPlayPromise = null;
  c.micAudioPlayPromise = null;
  c.stopPlaybackMonitor();
  c.setPlaying(false);
  c.setCurrentTime(c.video.currentTime);
}

// ---------------------------------------------------------------------------
// handleWaiting / handlePlaying (thin wrappers)
// ---------------------------------------------------------------------------

export function doHandleWaiting(c: ControllerInternals): void {
  handleWaitingEvent(
    c.video,
    c.stallState,
    c.onBufferingChange,
    c.isGeneratingThumbnail,
  );
}

export function doHandlePlaying(c: ControllerInternals): void {
  handlePlayingEvent(c.stallState, c.onBufferingChange);
}

// ---------------------------------------------------------------------------
// handleTimeUpdate
// ---------------------------------------------------------------------------

export function doHandleTimeUpdate(c: ControllerInternals): void {
  if (c.isGeneratingThumbnail) return;
  if (c.state.isSeeking || c.pendingSeekTime !== null) return;

  const currentTime = c.video.currentTime;
  if (doMaybeRecoverFromUnexpectedStartRegression(c, currentTime)) return;

  if (c.renderOptions?.segment) {
    const corrected = enforceSegmentPlaybackBounds(
      c.video,
      c.renderOptions.segment,
      c.getEffectiveDuration(currentTime),
      currentTime,
      false,
      c.SEGMENT_EPS,
      (t) => c.syncAllMediaToTime(t),
      (t) => c.setCurrentTime(t),
    );
    if (corrected !== null) return;
  }

  // Smooth audio sync: only correct if drift > 150ms to avoid audio stutter
  if (c.hasExternalAudio && !c.video.paused) {
    const speed = c.video.playbackRate;
    c.webcamVideoPlayPromise = syncTimedMediaPlayback(
      c.webcamVideo,
      c.hasValidWebcamVideo,
      c.webcamVideoPlayPromise,
      c.video.currentTime,
      c.getWebcamOffsetSec(),
    );
    c.micAudioPlayPromise = syncTimedMediaPlayback(
      c.micAudio,
      c.hasValidMicAudio,
      c.micAudioPlayPromise,
      c.video.currentTime,
      c.getMicAudioOffsetSec(),
    );
    correctMediaDrift(c, speed);
  }

  // Apply dynamic speed curve
  if (!c.video.paused) {
    const currentSpeed = c.getSpeed(currentTime);
    const safeRate = Math.max(0.0625, Math.min(16.0, currentSpeed));
    if (Math.abs(c.video.playbackRate - safeRate) > 0.05) {
      c.video.playbackRate = safeRate;
      syncAudioElementPlaybackRate(c.webcamVideo, safeRate);
      syncAudioElementPlaybackRate(c.deviceAudio, safeRate);
      syncAudioElementPlaybackRate(c.micAudio, safeRate);
    }
  }

  c.applyAudioTrackVolumes(currentTime);
  c.setCurrentTime(currentTime);
  maybeClearRecoveryAnchor(c.recoveryState, currentTime);
}

function correctMediaDrift(c: ControllerInternals, speed: number): void {
  const driftThreshold = Math.max(0.15, 0.1 * speed);
  const vt = c.video.currentTime;

  if (c.webcamVideo && c.hasValidWebcamVideo) {
    const wcTarget = Math.max(0, vt - c.getWebcamOffsetSec());
    if (Math.abs(wcTarget - c.webcamVideo.currentTime) > driftThreshold) {
      c.webcamVideo.currentTime = wcTarget;
    }
  }
  if (c.deviceAudio && c.hasValidDeviceAudio) {
    if (Math.abs(vt - c.deviceAudio.currentTime) > driftThreshold) {
      c.deviceAudio.currentTime = vt;
    }
  }
  if (c.micAudio && c.hasValidMicAudio) {
    const micTarget = Math.max(0, vt - c.getMicAudioOffsetSec());
    if (Math.abs(micTarget - c.micAudio.currentTime) > driftThreshold) {
      c.micAudio.currentTime = micTarget;
    }
  }
}

// ---------------------------------------------------------------------------
// handleSeeked
// ---------------------------------------------------------------------------

export function doHandleSeeked(c: ControllerInternals): void {
  if (c.isGeneratingThumbnail) return;
  c.setSeeking(false);

  if (c.pendingSeekTime !== null) {
    c.setCurrentTime(c.video.currentTime);
    doStartPendingSeek(c);
    return;
  }

  c.renderFrame();
  c.applyAudioTrackVolumes(c.video.currentTime);

  const requestedTime = c.lastRequestedSeekTime;
  const recoveryAnchorTime = c.recoveryState.anchorTime;
  const segmentStart = c.getSegmentStartTime(
    Math.max(c.video.currentTime, requestedTime ?? 0),
  );
  if (
    requestedTime !== null &&
    recoveryAnchorTime !== null &&
    requestedTime > segmentStart + 0.5 &&
    c.video.currentTime <= segmentStart + 0.05 &&
    c.recoveryState.retryCount < 1
  ) {
    c.recoveryState.retryCount += 1;
    logPlaybackReset(c.resetLogState, "seeked-regressed-to-start-retry", {
      requestedTime,
      recoveryTime: recoveryAnchorTime,
      videoTime: c.video.currentTime,
      readyState: c.video.readyState,
      networkState: c.video.networkState,
      currentSrc: c.video.currentSrc,
    });
    c.setSeeking(true);
    c.setCurrentTime(recoveryAnchorTime);
    c.video.currentTime = recoveryAnchorTime;
    c.syncAllMediaToTime(recoveryAnchorTime);
    return;
  }

  if (c.pendingSeekTime !== null) {
    doStartPendingSeek(c);
  } else {
    const clamped = c.renderOptions?.segment
      ? (getNextPlayableTime(
          c.video.currentTime,
          c.renderOptions.segment,
          c.getEffectiveDuration(c.video.currentTime),
        ) ??
        clampToTrimSegments(
          c.video.currentTime,
          c.renderOptions.segment,
          c.getEffectiveDuration(c.video.currentTime),
        ))
      : c.video.currentTime;
    maybeLogPlaybackReset(
      c.resetLogState,
      "seeked-correction-reset",
      c.lastRequestedSeekTime ?? c.video.currentTime,
      clamped,
      c.renderOptions?.segment,
      c.SEGMENT_EPS,
      c.getEffectiveDuration(c.video.currentTime),
      {
        requestedTime: c.lastRequestedSeekTime,
        videoTime: c.video.currentTime,
      },
    );

    if (Math.abs(clamped - c.video.currentTime) > 0.001) {
      c.video.currentTime = clamped;
      c.syncAllMediaToTime(clamped);
    }

    let displayTime = clamped;
    if (
      c.lastRequestedSeekTime !== null &&
      Math.abs(clamped - c.lastRequestedSeekTime) < 0.1
    ) {
      displayTime = c.lastRequestedSeekTime;
    }
    c.setCurrentTime(displayTime);
  }
}

function doStartPendingSeek(c: ControllerInternals): void {
  if (c.pendingSeekTime === null) return;
  const t = c.pendingSeekTime;
  c.pendingSeekTime = null;
  c.setSeeking(true);
  const clamped = c.renderOptions?.segment
    ? (getNextPlayableTime(
        t,
        c.renderOptions.segment,
        c.getEffectiveDuration(t),
      ) ??
      clampToTrimSegments(
        t,
        c.renderOptions.segment,
        c.getEffectiveDuration(t),
      ))
    : t;
  c.setCurrentTime(clamped);
  c.video.currentTime = clamped;
}

// ---------------------------------------------------------------------------
// Playback recovery from unexpected start regression
// ---------------------------------------------------------------------------

function doMaybeRecoverFromUnexpectedStartRegression(
  c: ControllerInternals,
  currentTime: number,
): boolean {
  const anchorTime = c.recoveryState.anchorTime;
  if (anchorTime === null) return false;
  if (performance.now() > c.recoveryState.anchorExpiresAt) {
    clearRecoveryAnchor(c.recoveryState);
    return false;
  }
  if (c.video.paused || c.isChangingSource || c.state.isSeeking) {
    return false;
  }
  const segmentStart = c.getSegmentStartTime(Math.max(currentTime, anchorTime));
  const previousTime = c.state.currentTime;
  const regressedToStart = currentTime <= segmentStart + 0.05;
  const meaningfulRegression =
    previousTime > segmentStart + 0.5 && previousTime - currentTime > 0.5;
  if (!regressedToStart || !meaningfulRegression) return false;

  logPlaybackReset(c.resetLogState, "timeupdate-start-regression", {
    fromTime: previousTime,
    toTime: currentTime,
    recoveryTime: anchorTime,
    readyState: c.video.readyState,
    networkState: c.video.networkState,
    paused: c.video.paused,
    seeking: c.video.seeking,
    playbackRate: c.video.playbackRate,
    currentSrc: c.video.currentSrc,
  });

  c.lastRequestedSeekTime = anchorTime;
  c.setSeeking(true);
  c.video.currentTime = anchorTime;
  c.syncAllMediaToTime(anchorTime);
  c.setCurrentTime(anchorTime);
  return true;
}

// ---------------------------------------------------------------------------
// play()
// ---------------------------------------------------------------------------

export function doPlay(c: ControllerInternals): void {
  if (!c.state.isReady) return;

  const playRequestId = ++c.playRequestSeq;
  const startPlayback = (targetTime: number) => {
    if (playRequestId !== c.playRequestSeq) return;
    c.syncAllMediaToTime(targetTime);
    c.setCurrentTime(targetTime);
    c.video.play().catch(() => {});
  };

  if (c.renderOptions?.segment) {
    const pausedResumeTime =
      c.video.paused &&
      Number.isFinite(c.state.currentTime) &&
      c.state.currentTime > 0
        ? c.state.currentTime
        : c.video.currentTime;
    const duration = c.getEffectiveDuration(pausedResumeTime);
    const segs = getTrimSegments(c.renderOptions.segment, duration);
    const lastSegmentEnd = segs[segs.length - 1]?.endTime ?? duration;
    const nextTime = getNextPlayableTime(
      pausedResumeTime,
      c.renderOptions.segment,
      duration,
    );
    let targetTime: number;
    if (nextTime !== null) {
      targetTime = nextTime;
    } else if (
      segs.length > 0 &&
      pausedResumeTime >= lastSegmentEnd - c.SEGMENT_EPS
    ) {
      targetTime = segs[0].startTime;
    } else {
      targetTime = clampToTrimSegments(
        pausedResumeTime,
        c.renderOptions.segment,
        duration,
      );
    }
    maybeLogPlaybackReset(
      c.resetLogState,
      "play-resume-reset",
      pausedResumeTime,
      targetTime,
      c.renderOptions.segment,
      c.SEGMENT_EPS,
      duration,
      {
        requestedTime: pausedResumeTime,
        nextPlayableTime: nextTime,
        lastSegmentEnd,
      },
    );
    c.lastRequestedSeekTime = targetTime;
    c.armPlaybackRecoveryAnchor(targetTime);
    if (Math.abs(c.video.currentTime - targetTime) > 0.05) {
      c.setSeeking(true);
      c.setCurrentTime(targetTime);
      const handleSeekedForPlay = () => {
        c.setSeeking(false);
        if (playRequestId !== c.playRequestSeq) return;
        startPlayback(c.video.currentTime);
      };
      c.video.addEventListener("seeked", handleSeekedForPlay, { once: true });
      c.video.currentTime = targetTime;
      return;
    }
    c.video.currentTime = targetTime;
    startPlayback(targetTime);
    return;
  }

  startPlayback(c.video.currentTime);
}

// ---------------------------------------------------------------------------
// seek()
// ---------------------------------------------------------------------------

export function doSeek(c: ControllerInternals, time: number): void {
  if (!c.state.isReady) return;
  const requestedTime = time;

  if (c.renderOptions?.segment) {
    const duration = c.getEffectiveDuration(time);
    time =
      getNextPlayableTime(time, c.renderOptions.segment, duration) ??
      clampToTrimSegments(time, c.renderOptions.segment, duration);
  }
  maybeLogPlaybackReset(
    c.resetLogState,
    "seek-clamped-reset",
    requestedTime,
    time,
    c.renderOptions?.segment,
    c.SEGMENT_EPS,
    c.getEffectiveDuration(time),
    { requestedTime },
  );

  c.lastRequestedSeekTime = time;
  c.armPlaybackRecoveryAnchor(time);
  c.setCurrentTime(time);

  if (c.state.isSeeking) {
    c.pendingSeekTime = time;
    return;
  }

  c.setSeeking(true);
  c.video.currentTime = time;
  c.syncAllMediaToTime(time);
}

// ---------------------------------------------------------------------------
// flushPendingSeek()
// ---------------------------------------------------------------------------

export function doFlushPendingSeek(c: ControllerInternals): void {
  if (c.pendingSeekTime !== null && !c.state.isSeeking) {
    const t = c.pendingSeekTime;
    c.pendingSeekTime = null;
    const clamped = c.renderOptions?.segment
      ? (getNextPlayableTime(
          t,
          c.renderOptions.segment,
          c.getEffectiveDuration(t),
        ) ??
        clampToTrimSegments(
          t,
          c.renderOptions.segment,
          c.getEffectiveDuration(t),
        ))
      : t;
    c.armPlaybackRecoveryAnchor(clamped);
    c.setSeeking(true);
    c.video.currentTime = clamped;
    c.syncAllMediaToTime(clamped);
    c.setCurrentTime(clamped);
  }
}
