import { videoRenderer } from "./videoRenderer";
import type {
  VideoSegment,
  BackgroundConfig,
  MousePosition,
  WebcamConfig,
} from "@/types/video";
import {
  clampToTrimSegments,
  getNextPlayableTime,
  getTrimSegments,
} from "./trimSegments";
import {
  buildFlatDeviceAudioPoints,
  clampDeviceAudioVolume,
  getDeviceAudioVolumeAtTime,
} from "./deviceAudio";
import {
  buildFlatMicAudioPoints,
  clampMicAudioVolume,
  getMicAudioVolumeAtTime,
} from "./micAudio";
import { getSpeedAtTime } from "./videoExporter";
import { DEFAULT_BUILT_IN_BACKGROUND_ID } from "@/lib/backgroundPresets";
import { isNativeMediaUrl } from "@/lib/mediaServer";

interface VideoControllerOptions {
  videoRef: HTMLVideoElement;
  webcamVideoRef?: HTMLVideoElement;
  deviceAudioRef?: HTMLAudioElement;
  micAudioRef?: HTMLAudioElement;
  canvasRef: HTMLCanvasElement;
  tempCanvasRef: HTMLCanvasElement;
  onTimeUpdate?: (time: number) => void;
  onPlayingChange?: (isPlaying: boolean) => void;
  onVideoReady?: (ready: boolean) => void;
  onBufferingChange?: (isBuffering: boolean) => void;
  onError?: (error: string) => void;
  onDurationChange?: (duration: number) => void;
  onMetadataLoaded?: (metadata: {
    duration: number;
    width: number;
    height: number;
  }) => void;
}

interface VideoState {
  isPlaying: boolean;
  isReady: boolean;
  isSeeking: boolean;
  currentTime: number;
  duration: number;
}

interface RenderOptions {
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  interactiveBackgroundPreview?: boolean;
}

const PLAYBACK_RESET_LOG_DEDUPE_MS = 800;
const PLAYBACK_RESET_DEBUG = false;

export class VideoController {
  private video: HTMLVideoElement;
  private webcamVideo?: HTMLVideoElement;
  private deviceAudio?: HTMLAudioElement;
  private micAudio?: HTMLAudioElement;
  private canvas: HTMLCanvasElement;
  private tempCanvas: HTMLCanvasElement;
  private options: VideoControllerOptions;
  private state: VideoState;
  private renderOptions?: RenderOptions;
  private isChangingSource = false;
  private isGeneratingThumbnail = false;
  private pendingSeekTime: number | null = null;
  private lastRequestedSeekTime: number | null = null;
  private readonly SEGMENT_EPS = 0.03;
  private playbackMonitorRaf: number | null = null;
  private renderTimeout: number | null = null;
  private seekSafetyTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly SEEK_SAFETY_TIMEOUT_MS = 3000;
  private lastResetLog: { signature: string; at: number } | null = null;
  private pendingSourceChangeLabel: string | null = null;
  private playRequestSeq = 0;
  private playbackRecoveryAnchorTime: number | null = null;
  private playbackRecoveryAnchorExpiresAt = 0;
  private playbackRecoveryRetryCount = 0;
  private webcamVideoPlayPromise: Promise<void> | null = null;
  private deviceAudioPlayPromise: Promise<void> | null = null;
  private micAudioPlayPromise: Promise<void> | null = null;

  constructor(options: VideoControllerOptions) {
    this.video = options.videoRef;
    this.webcamVideo = options.webcamVideoRef;
    this.deviceAudio = options.deviceAudioRef;
    this.micAudio = options.micAudioRef;
    this.canvas = options.canvasRef;
    this.tempCanvas = options.tempCanvasRef;
    this.options = options;

    this.state = {
      isPlaying: false,
      isReady: false,
      isSeeking: false,
      currentTime: 0,
      duration: 0,
    };

    this.initializeEventListeners();
  }

  private initializeEventListeners() {
    this.video.addEventListener("loadeddata", this.handleLoadedData);
    this.video.addEventListener("play", this.handlePlay);
    this.video.addEventListener("pause", this.handlePause);
    this.video.addEventListener("timeupdate", this.handleTimeUpdate);
    this.video.addEventListener("seeked", this.handleSeeked);
    this.video.addEventListener("waiting", this.handleWaiting);
    this.video.addEventListener("playing", this.handlePlaying);
    this.video.addEventListener("loadedmetadata", this.handleLoadedMetadata);
    this.video.addEventListener("durationchange", this.handleDurationChange);
    this.video.addEventListener("error", this.handleError);
  }

  private handleLoadedData = () => {
    // During source changes, canplaythrough handler manages ready state & rendering
    if (this.isChangingSource) return;
    this.applyAudioTrackVolumes(this.video.currentTime);
    this.renderFrame();
    this.setReady(true);
  };

  private hasValidMediaElement(element?: HTMLMediaElement) {
    return !!(
      element &&
      element.src &&
      element.src !== "" &&
      element.src !== window.location.href
    );
  }

  private get hasValidDeviceAudio(): boolean {
    return this.hasValidMediaElement(this.deviceAudio);
  }

  private get hasValidMicAudio(): boolean {
    return this.hasValidMediaElement(this.micAudio);
  }

  private get hasValidWebcamVideo(): boolean {
    return this.hasValidMediaElement(this.webcamVideo);
  }

  private get hasExternalAudio(): boolean {
    return this.hasValidDeviceAudio || this.hasValidMicAudio;
  }

  private getMicAudioOffsetSec(): number {
    const offset = this.renderOptions?.segment?.micAudioOffsetSec ?? 0;
    return Number.isFinite(offset) ? offset : 0;
  }

  private getWebcamOffsetSec(): number {
    const offset = this.renderOptions?.segment?.webcamOffsetSec ?? 0;
    return Number.isFinite(offset) ? offset : 0;
  }

  private getTimedMediaCurrentTime(videoTime: number, offsetSec: number): number {
    return Math.max(0, videoTime - offsetSec);
  }

  private shouldTimedMediaBeActive(videoTime: number, offsetSec: number): boolean {
    return videoTime >= offsetSec - 0.001;
  }

  private syncAudioElementTime(
    element: HTMLMediaElement | undefined,
    time: number,
  ) {
    if (!element || !this.hasValidMediaElement(element)) return;
    if (Math.abs(element.currentTime - time) > 0.05) {
      element.currentTime = time;
    }
  }

  private syncTimedMediaElementTime(
    element: HTMLMediaElement | undefined,
    videoTime: number,
    offsetSec: number,
  ) {
    this.syncAudioElementTime(
      element,
      this.getTimedMediaCurrentTime(videoTime, offsetSec),
    );
  }

  private syncAudioElementPlaybackRate(
    element: HTMLMediaElement | undefined,
    playbackRate: number,
  ) {
    if (!element || !this.hasValidMediaElement(element)) return;
    element.playbackRate = playbackRate;
  }

  private playAudioElement(
    element: HTMLMediaElement | undefined,
  ): Promise<void> | null {
    if (!element || !this.hasValidMediaElement(element)) return null;
    return element.play().catch(() => {});
  }

  private pauseAudioElement(
    element: HTMLMediaElement | undefined,
    pendingPromise: Promise<void> | null,
  ) {
    if (!element || !this.hasValidMediaElement(element)) return;
    if (pendingPromise) {
      pendingPromise.then(() => element.pause()).catch(() => {});
      return;
    }
    element.pause();
  }

  private syncTimedMediaPlayback(
    element: HTMLMediaElement | undefined,
    isValid: boolean,
    pendingPromise: Promise<void> | null,
    videoTime: number,
    offsetSec: number,
  ): Promise<void> | null {
    if (!element || !isValid) {
      return null;
    }

    const targetTime = this.getTimedMediaCurrentTime(videoTime, offsetSec);
    if (Math.abs(element.currentTime - targetTime) > 0.05) {
      element.currentTime = targetTime;
    }

    if (!this.shouldTimedMediaBeActive(videoTime, offsetSec)) {
      this.pauseAudioElement(element, pendingPromise);
      return null;
    }

    if (element.paused) {
      return this.playAudioElement(element);
    }

    return pendingPromise;
  }

  private handlePlay = () => {
    this.applyAudioTrackVolumes(this.video.currentTime);
    this.syncTimedMediaElementTime(
      this.webcamVideo,
      this.video.currentTime,
      this.getWebcamOffsetSec(),
    );
    this.syncAudioElementTime(this.deviceAudio, this.video.currentTime);
    this.syncTimedMediaElementTime(
      this.micAudio,
      this.video.currentTime,
      this.getMicAudioOffsetSec(),
    );
    this.syncAudioElementPlaybackRate(this.webcamVideo, this.video.playbackRate);
    this.syncAudioElementPlaybackRate(this.deviceAudio, this.video.playbackRate);
    this.syncAudioElementPlaybackRate(this.micAudio, this.video.playbackRate);
    this.webcamVideoPlayPromise = this.syncTimedMediaPlayback(
      this.webcamVideo,
      this.hasValidWebcamVideo,
      this.webcamVideoPlayPromise,
      this.video.currentTime,
      this.getWebcamOffsetSec(),
    );
    this.deviceAudioPlayPromise = this.playAudioElement(this.deviceAudio);
    this.micAudioPlayPromise = this.syncTimedMediaPlayback(
      this.micAudio,
      this.hasValidMicAudio,
      this.micAudioPlayPromise,
      this.video.currentTime,
      this.getMicAudioOffsetSec(),
    );

    // Ensure animation is running
    if (this.renderOptions) {
      videoRenderer.startAnimation({
        video: this.video,
        webcamVideo: this.webcamVideo,
        canvas: this.canvas,
        tempCanvas: this.tempCanvas,
        segment: this.renderOptions.segment,
        backgroundConfig: this.renderOptions.backgroundConfig,
        webcamConfig: this.renderOptions.webcamConfig,
        mousePositions: this.renderOptions.mousePositions,
        currentTime: this.video.currentTime,
        interactiveBackgroundPreview:
          this.renderOptions.interactiveBackgroundPreview,
      });
    }

    this.startPlaybackMonitor();
    this.setPlaying(true);
  };

  private handlePause = () => {
    this.playRequestSeq += 1;
    this.clearPlaybackRecoveryAnchor();
    this.options.onBufferingChange?.(false);
    this.pauseAudioElement(this.webcamVideo, this.webcamVideoPlayPromise);
    this.pauseAudioElement(this.deviceAudio, this.deviceAudioPlayPromise);
    this.pauseAudioElement(this.micAudio, this.micAudioPlayPromise);
    this.webcamVideoPlayPromise = null;
    this.deviceAudioPlayPromise = null;
    this.micAudioPlayPromise = null;
    this.stopPlaybackMonitor();
    this.setPlaying(false);
    this.setCurrentTime(this.video.currentTime);
    // Intentionally NOT re-drawing here. The last animation frame stays visible.
    // Re-drawing on pause can cause a visual shift because the video decoder may
    // have advanced video.currentTime slightly beyond the last rendered frame.
    // Seeking (handleSeeked) and edits (updateRenderOptions) still trigger draws.
  };

  // Handle video decoder stall: the video element is still "playing" but
  // no frames are being decoded (buffer underrun after seek on long videos).
  // Without this handler, the UI shows the pause button but nothing happens.
  private waitingStallTimer: ReturnType<typeof setTimeout> | null = null;

  private waitingStartedAt: number = 0;

  private waitingRecoveryAttempts: number = 0;

  private handleWaiting = () => {
    if (this.isGeneratingThumbnail) return;
    // Reset per-episode state so a new waiting event always starts fresh,
    // even if a previous stall forced a pause and the user pressed play again.
    this.waitingStartedAt = performance.now();
    this.waitingRecoveryAttempts = 0;
    this.options.onBufferingChange?.(true);
    console.warn(
      `[VideoController] WAITING started: readyState=${this.video.readyState} ` +
      `currentTime=${this.video.currentTime.toFixed(3)} paused=${this.video.paused} ` +
      `networkState=${this.video.networkState}`
    );
    if (this.waitingStallTimer !== null) clearTimeout(this.waitingStallTimer);
    this.scheduleStallCheck(3000);
  };

  // Stall-check tick called every 3s while the video is waiting.
  // Strategy:
  //   networkState=2 (NETWORK_LOADING): browser is already fetching — never re-seek
  //     (that would cancel and restart the range request). Just wait up to 25s total.
  //   networkState≠2 (IDLE): browser stopped fetching — nudge it with a re-seek,
  //     then allow up to 15s for the resulting load to complete.
  private scheduleStallCheck(delayMs: number) {
    this.waitingStallTimer = setTimeout(() => {
      this.waitingStallTimer = null;
      if (this.video.paused || this.video.readyState >= 3) return;

      const stallMs = performance.now() - this.waitingStartedAt;
      const stallSec = (stallMs / 1000).toFixed(1);
      const networkState = this.video.networkState;

      if (networkState === 2 /* NETWORK_LOADING */) {
        // Browser is actively fetching — keep waiting, check again shortly.
        if (stallMs < 25_000) {
          console.warn(
            `[VideoController] Still loading at ${stallSec}s (networkState=LOADING, readyState=${this.video.readyState}) — waiting`
          );
          this.scheduleStallCheck(3000);
        } else {
          console.error(
            `[VideoController] STALL for ${stallSec}s: load timed out (networkState=LOADING) — forcing pause`
          );
          this.video.pause();
        }
      } else {
        // Browser went idle — a re-seek will restart the range request.
        if (this.waitingRecoveryAttempts < 2) {
          this.waitingRecoveryAttempts++;
          const rescueTime = this.video.currentTime;
          console.warn(
            `[VideoController] STALL recovery attempt ${this.waitingRecoveryAttempts} at ${stallSec}s: ` +
            `re-seeking to ${rescueTime.toFixed(3)} (readyState=${this.video.readyState})`
          );
          this.video.currentTime = rescueTime;
          // After a re-seek the browser transitions to LOADING — give it 5s before
          // the next tick so the networkState check above applies.
          this.scheduleStallCheck(5000);
        } else {
          console.error(
            `[VideoController] STALL for ${stallSec}s after ${this.waitingRecoveryAttempts} recovery attempts ` +
            `(networkState=${networkState}) — forcing pause`
          );
          this.video.pause();
        }
      }
    }, delayMs);
  }

  private handlePlaying = () => {
    if (this.waitingStartedAt > 0) {
      const recoveryMs = (performance.now() - this.waitingStartedAt).toFixed(0);
      console.log(
        `[VideoController] RECOVERED from waiting in ${recoveryMs}ms` +
        `${this.waitingRecoveryAttempts > 0 ? ` (after ${this.waitingRecoveryAttempts} recovery attempt(s))` : ''}: ` +
        `readyState=${this.video.readyState} currentTime=${this.video.currentTime.toFixed(3)}`
      );
      this.waitingStartedAt = 0;
      this.waitingRecoveryAttempts = 0;
    }
    this.options.onBufferingChange?.(false);
    if (this.waitingStallTimer !== null) {
      clearTimeout(this.waitingStallTimer);
      this.waitingStallTimer = null;
    }
  };

  private getSpeed(time: number): number {
    if (!this.renderOptions?.segment?.speedPoints?.length) return 1.0;
    return getSpeedAtTime(time, this.renderOptions.segment.speedPoints);
  }

  private getDeviceAudioVolume(time: number): number {
    return clampDeviceAudioVolume(
      getDeviceAudioVolumeAtTime(
        time,
        this.renderOptions?.segment?.deviceAudioPoints,
      ),
    );
  }

  private getMicAudioVolume(time: number): number {
    return clampMicAudioVolume(
      getMicAudioVolumeAtTime(time, this.renderOptions?.segment?.micAudioPoints),
    );
  }

  private applyAudioTrackVolumes(time: number = this.video.currentTime) {
    const deviceVolume = this.getDeviceAudioVolume(time);
    const micVolume = this.getMicAudioVolume(time);

    if (this.hasValidDeviceAudio && this.deviceAudio) {
      this.deviceAudio.volume = deviceVolume;
    }
    if (this.hasValidMicAudio && this.micAudio) {
      this.micAudio.volume = micVolume;
    }

    if (this.hasExternalAudio) {
      this.video.muted = true;
      return;
    }

    this.video.muted = false;
    this.video.volume = deviceVolume;
  }

  private handleTimeUpdate = () => {
    if (this.isGeneratingThumbnail) return;
    if (!this.state.isSeeking && this.pendingSeekTime === null) {
      const currentTime = this.video.currentTime;
      if (this.maybeRecoverFromUnexpectedStartRegression(currentTime)) return;

      // Handle segmented trim bounds
      if (this.renderOptions?.segment) {
        const corrected = this.enforceSegmentPlaybackBounds(currentTime, false);
        if (corrected !== null) return;
      }

      // Smooth audio sync: only correct if drift > 150ms to avoid audio stutter
      if (this.hasExternalAudio && !this.video.paused) {
        const speed = this.video.playbackRate;
        this.webcamVideoPlayPromise = this.syncTimedMediaPlayback(
          this.webcamVideo,
          this.hasValidWebcamVideo,
          this.webcamVideoPlayPromise,
          this.video.currentTime,
          this.getWebcamOffsetSec(),
        );
        this.micAudioPlayPromise = this.syncTimedMediaPlayback(
          this.micAudio,
          this.hasValidMicAudio,
          this.micAudioPlayPromise,
          this.video.currentTime,
          this.getMicAudioOffsetSec(),
        );
        if (
          this.webcamVideo &&
          this.hasValidWebcamVideo &&
          Math.abs(
            this.getTimedMediaCurrentTime(
              this.video.currentTime,
              this.getWebcamOffsetSec(),
            ) - this.webcamVideo.currentTime,
          ) >
            Math.max(0.15, 0.1 * speed)
        ) {
          this.webcamVideo.currentTime = this.getTimedMediaCurrentTime(
            this.video.currentTime,
            this.getWebcamOffsetSec(),
          );
        }
        if (
          this.deviceAudio &&
          this.hasValidDeviceAudio &&
          Math.abs(this.video.currentTime - this.deviceAudio.currentTime) >
            Math.max(0.15, 0.1 * speed)
        ) {
          this.deviceAudio.currentTime = this.video.currentTime;
        }
        if (
          this.micAudio &&
          this.hasValidMicAudio &&
          Math.abs(
            this.getTimedMediaCurrentTime(
              this.video.currentTime,
              this.getMicAudioOffsetSec(),
            ) - this.micAudio.currentTime,
          ) >
            Math.max(0.15, 0.1 * speed)
        ) {
          this.micAudio.currentTime = this.getTimedMediaCurrentTime(
            this.video.currentTime,
            this.getMicAudioOffsetSec(),
          );
        }
      }

      // Apply dynamic speed curve smoothly without resetting playback loop
      if (!this.video.paused) {
        const currentSpeed = this.getSpeed(currentTime);
        const safeRate = Math.max(0.0625, Math.min(16.0, currentSpeed));
        if (Math.abs(this.video.playbackRate - safeRate) > 0.05) {
          this.video.playbackRate = safeRate;
          this.syncAudioElementPlaybackRate(this.webcamVideo, safeRate);
          this.syncAudioElementPlaybackRate(this.deviceAudio, safeRate);
          this.syncAudioElementPlaybackRate(this.micAudio, safeRate);
        }
      }

      this.applyAudioTrackVolumes(currentTime);
      this.setCurrentTime(currentTime);
      this.maybeClearPlaybackRecoveryAnchor(currentTime);
      // Removed renderFrame here - allow animation loop to handle updates during playback
      // If paused, handlePause triggers renderFrame.
      // If playing, startAnimation loop handles it.
    }
  };

  private handleSeeked = () => {
    if (this.isGeneratingThumbnail) return;
    this.setSeeking(false);

    // If there's a pending seek (user is still dragging), skip the expensive
    // renderFrame() and audio sync — just update currentTime for the playhead
    // and immediately start the next seek. This prevents decoder thrashing
    // and keeps the decoder working on the latest requested position.
    if (this.pendingSeekTime !== null) {
      this.setCurrentTime(this.video.currentTime);
      this.startPendingSeek();
      return;
    }

    // Final seek position — render the decoded frame and sync audio
    this.renderFrame();
    this.applyAudioTrackVolumes(this.video.currentTime);

    const requestedTime = this.lastRequestedSeekTime;
    const recoveryAnchorTime = this.playbackRecoveryAnchorTime;
    const segmentStart = this.getSegmentStartTime(
      Math.max(this.video.currentTime, requestedTime ?? 0),
    );
    if (
      requestedTime !== null &&
      recoveryAnchorTime !== null &&
      requestedTime > segmentStart + 0.5 &&
      this.video.currentTime <= segmentStart + 0.05 &&
      this.playbackRecoveryRetryCount < 1
    ) {
      this.playbackRecoveryRetryCount += 1;
      this.logPlaybackReset("seeked-regressed-to-start-retry", {
        requestedTime,
        recoveryTime: recoveryAnchorTime,
        videoTime: this.video.currentTime,
        readyState: this.video.readyState,
        networkState: this.video.networkState,
        currentSrc: this.video.currentSrc,
      });
      this.setSeeking(true);
      this.setCurrentTime(recoveryAnchorTime);
      this.video.currentTime = recoveryAnchorTime;
      this.syncTimedMediaElementTime(
        this.webcamVideo,
        recoveryAnchorTime,
        this.getWebcamOffsetSec(),
      );
      this.syncAudioElementTime(this.deviceAudio, recoveryAnchorTime);
      this.syncTimedMediaElementTime(
        this.micAudio,
        recoveryAnchorTime,
        this.getMicAudioOffsetSec(),
      );
      return;
    }

    // If there's a queued seek (from drag moves while decoder was busy),
    // start the next seek immediately to keep the decoder maximally busy.
    if (this.pendingSeekTime !== null) {
      this.startPendingSeek();
    } else {
      const clamped = this.renderOptions?.segment
        ? (getNextPlayableTime(
            this.video.currentTime,
            this.renderOptions.segment,
            this.getEffectiveDuration(this.video.currentTime),
          ) ??
          clampToTrimSegments(
            this.video.currentTime,
            this.renderOptions.segment,
            this.getEffectiveDuration(this.video.currentTime),
          ))
        : this.video.currentTime;
      this.maybeLogPlaybackReset(
        "seeked-correction-reset",
        this.lastRequestedSeekTime ?? this.video.currentTime,
        clamped,
        {
          requestedTime: this.lastRequestedSeekTime,
          videoTime: this.video.currentTime,
        },
      );

      if (Math.abs(clamped - this.video.currentTime) > 0.001) {
        this.video.currentTime = clamped;
        this.syncTimedMediaElementTime(
          this.webcamVideo,
          clamped,
          this.getWebcamOffsetSec(),
        );
        this.syncAudioElementTime(this.deviceAudio, clamped);
        this.syncTimedMediaElementTime(
          this.micAudio,
          clamped,
          this.getMicAudioOffsetSec(),
        );
      }

      let displayTime = clamped;
      // Prevent UI playhead stutter: if decoder snapped near the requested target,
      // keep the user's dragged target as the visual playhead position.
      if (
        this.lastRequestedSeekTime !== null &&
        Math.abs(clamped - this.lastRequestedSeekTime) < 0.1
      ) {
        displayTime = this.lastRequestedSeekTime;
      }
      this.setCurrentTime(displayTime);
    }
  };

  /** Start the next queued seek from pendingSeekTime. Skips audio sync
   *  during rapid scrubbing to minimize main thread work. */
  private startPendingSeek() {
    if (this.pendingSeekTime === null) return;
    const t = this.pendingSeekTime;
    this.pendingSeekTime = null;
    this.setSeeking(true);
    const clamped = this.renderOptions?.segment
      ? (getNextPlayableTime(
          t,
          this.renderOptions.segment,
          this.getEffectiveDuration(t),
        ) ??
        clampToTrimSegments(
          t,
          this.renderOptions.segment,
          this.getEffectiveDuration(t),
        ))
      : t;
    this.setCurrentTime(clamped);
    this.video.currentTime = clamped;
    // Skip audio sync during rapid scrubbing — only sync on final seek
    // (flushPendingSeek or handleSeeked with no more pending seeks).
  }

  private enforceSegmentPlaybackBounds(
    currentTime: number,
    forceTransitionAtEnd: boolean,
  ): number | null {
    if (!this.renderOptions?.segment) return null;

    const segs = getTrimSegments(
      this.renderOptions.segment,
      this.getEffectiveDuration(currentTime),
    );
    const last = segs[segs.length - 1];
    const TRANSITION_EPS = forceTransitionAtEnd ? 0.003 : this.SEGMENT_EPS;

    const currentSegIndex = segs.findIndex(
      (s) =>
        currentTime >= s.startTime - this.SEGMENT_EPS &&
        currentTime <= s.endTime + this.SEGMENT_EPS,
    );
    const isInside = currentSegIndex >= 0;

    if (!isInside) {
      const nextTime = getNextPlayableTime(
        currentTime,
        this.renderOptions.segment,
        this.getEffectiveDuration(currentTime),
      );
      if (nextTime !== null && nextTime - currentTime > this.SEGMENT_EPS) {
        this.video.currentTime = nextTime;
        this.syncTimedMediaElementTime(
          this.webcamVideo,
          nextTime,
          this.getWebcamOffsetSec(),
        );
        this.syncAudioElementTime(this.deviceAudio, nextTime);
        this.syncTimedMediaElementTime(
          this.micAudio,
          nextTime,
          this.getMicAudioOffsetSec(),
        );
        this.setCurrentTime(nextTime);
        return nextTime;
      }
      if (
        nextTime !== null &&
        Math.abs(nextTime - currentTime) <= this.SEGMENT_EPS
      ) {
        this.setCurrentTime(nextTime);
        return nextTime;
      }
      if (currentTime >= last.endTime - TRANSITION_EPS && !this.video.paused) {
        this.video.currentTime = last.endTime;
        this.syncTimedMediaElementTime(
          this.webcamVideo,
          last.endTime,
          this.getWebcamOffsetSec(),
        );
        this.syncAudioElementTime(this.deviceAudio, last.endTime);
        this.syncTimedMediaElementTime(
          this.micAudio,
          last.endTime,
          this.getMicAudioOffsetSec(),
        );
        this.setCurrentTime(last.endTime);
        this.video.pause();
        return last.endTime;
      }
      return null;
    }

    const currentSeg = segs[currentSegIndex];
    if (
      currentSeg &&
      currentTime >= currentSeg.endTime - TRANSITION_EPS &&
      !this.video.paused
    ) {
      const next = segs[currentSegIndex + 1];
      if (next && next.startTime - currentTime > this.SEGMENT_EPS) {
        this.video.currentTime = next.startTime;
        this.syncTimedMediaElementTime(
          this.webcamVideo,
          next.startTime,
          this.getWebcamOffsetSec(),
        );
        this.syncAudioElementTime(this.deviceAudio, next.startTime);
        this.syncTimedMediaElementTime(
          this.micAudio,
          next.startTime,
          this.getMicAudioOffsetSec(),
        );
        this.setCurrentTime(next.startTime);
        return next.startTime;
      }
      if (next && Math.abs(next.startTime - currentTime) <= this.SEGMENT_EPS) {
        this.setCurrentTime(next.startTime);
        return next.startTime;
      }
      this.video.currentTime = currentSeg.endTime;
      this.syncTimedMediaElementTime(
        this.webcamVideo,
        currentSeg.endTime,
        this.getWebcamOffsetSec(),
      );
      this.syncAudioElementTime(this.deviceAudio, currentSeg.endTime);
      this.syncTimedMediaElementTime(
        this.micAudio,
        currentSeg.endTime,
        this.getMicAudioOffsetSec(),
      );
      this.setCurrentTime(currentSeg.endTime);
      this.video.pause();
      return currentSeg.endTime;
    }

    return null;
  }

  private startPlaybackMonitor() {
    this.stopPlaybackMonitor();
    const loop = () => {
      if (this.video.paused) {
        this.playbackMonitorRaf = null;
        return;
      }
      if (!this.state.isSeeking) {
        this.enforceSegmentPlaybackBounds(this.video.currentTime, true);
        this.applyAudioTrackVolumes(this.video.currentTime);
      }
      this.playbackMonitorRaf = requestAnimationFrame(loop);
    };
    this.playbackMonitorRaf = requestAnimationFrame(loop);
  }

  private stopPlaybackMonitor() {
    if (this.playbackMonitorRaf !== null) {
      cancelAnimationFrame(this.playbackMonitorRaf);
      this.playbackMonitorRaf = null;
    }
  }

  private handleLoadedMetadata = () => {
    this.options.onMetadataLoaded?.({
      duration: this.video.duration,
      width: this.video.videoWidth,
      height: this.video.videoHeight,
    });

    if (this.video.duration !== Infinity) {
      this.setDuration(this.video.duration);
      // Initialize segment if none exists
      if (!this.renderOptions?.segment) {
        this.renderOptions = {
          segment: this.initializeSegment(),
          backgroundConfig: {
            scale: 100,
            borderRadius: 8,
            backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID,
          },
          mousePositions: [],
        };
      }
    }
  };

  private handleDurationChange = () => {
    if (this.video.duration !== Infinity) {
      this.setDuration(this.video.duration);

      // Update trimEnd if it was not set correctly or is 0
      if (this.renderOptions?.segment) {
        if (
          this.renderOptions.segment.trimEnd === 0 ||
          this.renderOptions.segment.trimEnd > this.video.duration
        ) {
          this.renderOptions.segment.trimEnd = this.video.duration;
        }
      }
    }
  };

  private handleError = (e: Event) => {
    const video = e.target as HTMLVideoElement;
    const mediaError = video.error;
    const srcAttr = video.getAttribute("src");
    const isIntentionalResetError =
      mediaError?.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED &&
      (!srcAttr || srcAttr.length === 0) &&
      this.isChangingSource;
    if (isIntentionalResetError) {
      return;
    }
    console.error(
      "Video error:",
      mediaError?.message || "Unknown error",
      `(code: ${mediaError?.code})`,
    );
    this.options.onError?.(mediaError?.message || "Unknown video error");
  };

  private clearMediaElementSource(element: HTMLMediaElement) {
    element.pause();
    element.removeAttribute("src");
    element.load();
  }

  private resetTransientPlaybackState() {
    this.playRequestSeq += 1;
    this.clearPlaybackRecoveryAnchor();
    this.maybeLogPlaybackReset("source-change-reset", this.state.currentTime, 0, {
      sourceChange: this.pendingSourceChangeLabel,
      isReady: this.state.isReady,
      isPlaying: this.state.isPlaying,
    });
    this.stopPlaybackMonitor();
    videoRenderer.stopAnimation();
    this.renderOptions = undefined;
    this.pendingSeekTime = null;
    this.lastRequestedSeekTime = null;
    this.setSeeking(false);
    this.setPlaying(false);
    this.setCurrentTime(0);
    this.setDuration(0);
  }

  private setPlaying(playing: boolean) {
    this.state.isPlaying = playing;
    this.options.onPlayingChange?.(playing);
  }

  private setReady(ready: boolean) {
    this.state.isReady = ready;
    this.options.onVideoReady?.(ready);
  }

  private setSeeking(seeking: boolean) {
    this.state.isSeeking = seeking;

    // Safety timeout: if the browser never fires 'seeked', unstick after 3s
    // so the user can continue interacting.
    if (this.seekSafetyTimer !== null) {
      clearTimeout(this.seekSafetyTimer);
      this.seekSafetyTimer = null;
    }
    if (seeking) {
      this.seekSafetyTimer = setTimeout(() => {
        this.seekSafetyTimer = null;
        if (this.state.isSeeking) {
          console.warn("[VideoController] Seek safety timeout — unsticking isSeeking");
          this.state.isSeeking = false;
          // Flush any pending seek that was waiting
          if (this.pendingSeekTime !== null) {
            const t = this.pendingSeekTime;
            this.pendingSeekTime = null;
            this.seek(t);
          }
        }
      }, VideoController.SEEK_SAFETY_TIMEOUT_MS);
    }
  }

  private setCurrentTime(time: number) {
    this.state.currentTime = time;
    this.options.onTimeUpdate?.(time);
  }

  private setDuration(duration: number) {
    this.state.duration = duration;
    this.options.onDurationChange?.(duration);
  }

  private getSegmentStartTime(referenceTime: number): number {
    if (!this.renderOptions?.segment) return 0;
    return (
      getTrimSegments(
        this.renderOptions.segment,
        this.getEffectiveDuration(referenceTime),
      )[0]?.startTime ?? 0
    );
  }

  private clearPlaybackRecoveryAnchor() {
    this.playbackRecoveryAnchorTime = null;
    this.playbackRecoveryAnchorExpiresAt = 0;
    this.playbackRecoveryRetryCount = 0;
  }

  private armPlaybackRecoveryAnchor(time: number) {
    if (!Number.isFinite(time)) {
      this.clearPlaybackRecoveryAnchor();
      return;
    }
    const segmentStart = this.getSegmentStartTime(time);
    if (time <= segmentStart + this.SEGMENT_EPS) {
      this.clearPlaybackRecoveryAnchor();
      return;
    }
    this.playbackRecoveryAnchorTime = time;
    this.playbackRecoveryAnchorExpiresAt = performance.now() + 1500;
    this.playbackRecoveryRetryCount = 0;
  }

  private maybeClearPlaybackRecoveryAnchor(currentTime: number) {
    const anchorTime = this.playbackRecoveryAnchorTime;
    if (anchorTime === null) return;
    if (performance.now() > this.playbackRecoveryAnchorExpiresAt) {
      this.clearPlaybackRecoveryAnchor();
      return;
    }
    if (currentTime >= anchorTime - 0.05) {
      this.clearPlaybackRecoveryAnchor();
    }
  }

  private maybeRecoverFromUnexpectedStartRegression(currentTime: number): boolean {
    const anchorTime = this.playbackRecoveryAnchorTime;
    if (anchorTime === null) return false;
    if (performance.now() > this.playbackRecoveryAnchorExpiresAt) {
      this.clearPlaybackRecoveryAnchor();
      return false;
    }
    if (this.video.paused || this.isChangingSource || this.state.isSeeking) {
      return false;
    }
    const segmentStart = this.getSegmentStartTime(Math.max(currentTime, anchorTime));
    const previousTime = this.state.currentTime;
    const regressedToStart = currentTime <= segmentStart + 0.05;
    const meaningfulRegression =
      previousTime > segmentStart + 0.5 && previousTime - currentTime > 0.5;
    if (!regressedToStart || !meaningfulRegression) return false;

    this.logPlaybackReset("timeupdate-start-regression", {
      fromTime: previousTime,
      toTime: currentTime,
      recoveryTime: anchorTime,
      readyState: this.video.readyState,
      networkState: this.video.networkState,
      paused: this.video.paused,
      seeking: this.video.seeking,
      playbackRate: this.video.playbackRate,
      currentSrc: this.video.currentSrc,
    });

    this.lastRequestedSeekTime = anchorTime;
    this.setSeeking(true);
    this.video.currentTime = anchorTime;
    this.syncTimedMediaElementTime(
      this.webcamVideo,
      anchorTime,
      this.getWebcamOffsetSec(),
    );
    this.syncAudioElementTime(this.deviceAudio, anchorTime);
    this.syncTimedMediaElementTime(
      this.micAudio,
      anchorTime,
      this.getMicAudioOffsetSec(),
    );
    this.setCurrentTime(anchorTime);
    return true;
  }

  private logPlaybackReset(
    reason: string,
    payload: Record<string, unknown>,
  ) {
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
      this.lastResetLog &&
      this.lastResetLog.signature === signature &&
      now - this.lastResetLog.at < PLAYBACK_RESET_LOG_DEDUPE_MS
    ) {
      return;
    }
    this.lastResetLog = { signature, at: now };
    console.warn("[PlaybackReset]", { reason, ...payload });
  }

  private maybeLogPlaybackReset(
    reason: string,
    fromTime: number,
    toTime: number,
    extra: Record<string, unknown> = {},
  ) {
    if (!Number.isFinite(fromTime) || !Number.isFinite(toTime)) return;
    const duration = this.getEffectiveDuration(Math.max(fromTime, toTime));
    const segmentStart = this.renderOptions?.segment
      ? (getTrimSegments(this.renderOptions.segment, duration)[0]?.startTime ?? 0)
      : 0;
    const resetToStart = toTime <= segmentStart + this.SEGMENT_EPS;
    const meaningfulRegression =
      fromTime > segmentStart + 0.5 && fromTime - toTime > 0.5;
    if (!resetToStart || !meaningfulRegression) return;
    this.logPlaybackReset(reason, {
      fromTime,
      toTime,
      segmentStart,
      duration,
      ...extra,
    });
  }

  private getSegmentDurationFallback(fallback: number): number {
    if (!this.renderOptions?.segment) return Math.max(fallback, 0);
    const trimSegments = this.renderOptions.segment.trimSegments ?? [];
    const maxTrimEnd = trimSegments.reduce(
      (max, trimSegment) => Math.max(max, trimSegment.endTime),
      this.renderOptions.segment.trimEnd,
    );
    return Math.max(maxTrimEnd, fallback, 0);
  }

  private getEffectiveDuration(fallback: number): number {
    if (Number.isFinite(this.video.duration) && this.video.duration > 0) {
      return this.video.duration;
    }
    if (Number.isFinite(this.state.duration) && this.state.duration > 0) {
      return this.state.duration;
    }
    return this.getSegmentDurationFallback(fallback);
  }

  private renderFrame() {
    if (!this.renderOptions) return;

    const renderContext = {
      video: this.video,
      webcamVideo: this.webcamVideo,
      canvas: this.canvas,
      tempCanvas: this.tempCanvas,
      segment: this.renderOptions.segment,
      backgroundConfig: this.renderOptions.backgroundConfig,
      webcamConfig: this.renderOptions.webcamConfig,
      mousePositions: this.renderOptions.mousePositions,
      currentTime: this.getAdjustedTime(this.video.currentTime),
      interactiveBackgroundPreview:
        this.renderOptions.interactiveBackgroundPreview,
    };

    // Only draw if video is ready
    if (this.video.readyState >= 2) {
      const effectiveDuration = this.getEffectiveDuration(
        renderContext.video.currentTime,
      );
      // Draw even if paused to support live preview when editing
      // but we can skip if the video is at the end and paused
      if (
        renderContext.video.paused &&
        effectiveDuration > 0 &&
        renderContext.video.currentTime >= effectiveDuration
      ) {
        // No animationFrame here, as renderFrame is called manually or by event listeners
        return;
      }
      // Update the active context for the animation loop
      videoRenderer.updateRenderContext(renderContext);
      videoRenderer.drawFrame(renderContext);
    } else {
      // console.log('[VideoController] Skipping frame - video not ready');
    }
  }

  /** Render the first frame to an offscreen canvas with full pipeline, return as data URL. */
  public async generateThumbnail(
    options: RenderOptions,
  ): Promise<string | undefined> {
    if (this.video.readyState < 2) return undefined;

    this.isGeneratingThumbnail = true;
    const savedTime = this.video.currentTime;

    try {
      // Seek to first visible frame
      this.video.currentTime = options.segment.trimStart;
      // Safety timeout so thumbnail generation never hangs at 100% processing
      await new Promise<void>((r) => {
        const timeout = setTimeout(() => r(), 600);
        this.video.addEventListener(
          "seeked",
          () => {
            clearTimeout(timeout);
            r();
          },
          { once: true },
        );
      });

      // Render to offscreen canvas (doesn't disturb the main display)
      const thumbCanvas = document.createElement("canvas");
      thumbCanvas.width = this.canvas.width;
      thumbCanvas.height = this.canvas.height;
      const thumbTemp = document.createElement("canvas");

      videoRenderer.drawFrame({
        video: this.video,
        webcamVideo: this.webcamVideo,
        canvas: thumbCanvas,
        tempCanvas: thumbTemp,
        segment: options.segment,
        backgroundConfig: options.backgroundConfig,
        webcamConfig: options.webcamConfig,
        mousePositions: options.mousePositions,
        currentTime: options.segment.trimStart,
        interactiveBackgroundPreview: false,
      });

      return thumbCanvas.toDataURL("image/jpeg", 0.7);
    } catch {
      return undefined;
    } finally {
      // Restore position and re-render main canvas
      this.video.currentTime = savedTime;
      await new Promise<void>((r) => {
        const timeout = setTimeout(() => r(), 600);
        this.video.addEventListener(
          "seeked",
          () => {
            clearTimeout(timeout);
            r();
          },
          { once: true },
        );
      }).catch(() => {});
      this.isGeneratingThumbnail = false;
      this.renderFrame();
    }
  }

  /** Draw one frame immediately with the given options (bypasses React state). */
  public renderImmediate(options: RenderOptions) {
    if (this.video.readyState < 2) return;
    this.renderOptions = options;
    this.applyAudioTrackVolumes(this.video.currentTime);
    const ctx = {
      video: this.video,
      webcamVideo: this.webcamVideo,
      canvas: this.canvas,
      tempCanvas: this.tempCanvas,
      segment: options.segment,
      backgroundConfig: options.backgroundConfig,
      webcamConfig: options.webcamConfig,
      mousePositions: options.mousePositions,
      currentTime: options.segment.trimStart,
      interactiveBackgroundPreview: options.interactiveBackgroundPreview,
    };
    videoRenderer.updateRenderContext(ctx);
    videoRenderer.drawFrame(ctx);
  }

  // Public API
  public updateRenderOptions(options: RenderOptions) {
    this.renderOptions = options;
    this.applyAudioTrackVolumes(this.video.currentTime);
    // Throttle heavy re-renders during rapid slider dragging (e.g. motion blur).
    if (this.renderTimeout === null) {
      this.renderTimeout = requestAnimationFrame(() => {
        this.renderFrame();
        this.renderTimeout = null;
      });
    }
  }

  public play() {
    if (!this.state.isReady) {
      return;
    }

    const playRequestId = ++this.playRequestSeq;
    const startPlayback = (targetTime: number) => {
      if (playRequestId !== this.playRequestSeq) return;
      this.syncTimedMediaElementTime(
        this.webcamVideo,
        targetTime,
        this.getWebcamOffsetSec(),
      );
      this.syncAudioElementTime(this.deviceAudio, targetTime);
      this.syncTimedMediaElementTime(
        this.micAudio,
        targetTime,
        this.getMicAudioOffsetSec(),
      );
      this.setCurrentTime(targetTime);
      this.video.play().catch(() => {});
    };

    if (this.renderOptions?.segment) {
      const pausedResumeTime =
        this.video.paused &&
        Number.isFinite(this.state.currentTime) &&
        this.state.currentTime > 0
          ? this.state.currentTime
          : this.video.currentTime;
      const duration = this.getEffectiveDuration(pausedResumeTime);
      const segs = getTrimSegments(this.renderOptions.segment, duration);
      const lastSegmentEnd = segs[segs.length - 1]?.endTime ?? duration;
      const nextTime = getNextPlayableTime(
        pausedResumeTime,
        this.renderOptions.segment,
        duration,
      );
      let targetTime: number;
      if (nextTime !== null) {
        targetTime = nextTime;
      } else if (
        segs.length > 0 &&
        pausedResumeTime >= lastSegmentEnd - this.SEGMENT_EPS
      ) {
        targetTime = segs[0].startTime;
      } else {
        targetTime = clampToTrimSegments(
          pausedResumeTime,
          this.renderOptions.segment,
          duration,
        );
      }
      this.maybeLogPlaybackReset(
        "play-resume-reset",
        pausedResumeTime,
        targetTime,
        {
          requestedTime: pausedResumeTime,
          nextPlayableTime: nextTime,
          lastSegmentEnd,
        },
      );
      this.lastRequestedSeekTime = targetTime;
      this.armPlaybackRecoveryAnchor(targetTime);
      if (Math.abs(this.video.currentTime - targetTime) > 0.05) {
        this.setSeeking(true);
        this.setCurrentTime(targetTime);
        const handleSeekedForPlay = () => {
          this.setSeeking(false);
          if (playRequestId !== this.playRequestSeq) return;
          startPlayback(this.video.currentTime);
        };
        this.video.addEventListener("seeked", handleSeekedForPlay, { once: true });
        this.video.currentTime = targetTime;
        return;
      }
      this.video.currentTime = targetTime;
      startPlayback(targetTime);
      return;
    }

    startPlayback(this.video.currentTime);
  }

  public pause() {
    this.video.pause();
  }

  public seek(time: number) {
    if (!this.state.isReady) return;
    const requestedTime = time;

    if (this.renderOptions?.segment) {
      const duration = this.getEffectiveDuration(time);
      time =
        getNextPlayableTime(time, this.renderOptions.segment, duration) ??
        clampToTrimSegments(time, this.renderOptions.segment, duration);
    }
    this.maybeLogPlaybackReset("seek-clamped-reset", requestedTime, time, {
      requestedTime,
    });

    this.lastRequestedSeekTime = time;
    this.armPlaybackRecoveryAnchor(time);
    // Update playhead state immediately so the UI feels responsive
    this.setCurrentTime(time);

    // isSeeking-based coalescing: if the decoder is already busy with a previous
    // seek, just store the latest time. When the 'seeked' event fires (handleSeeked),
    // we render the decoded frame and immediately start the next seek.
    // This keeps the decoder maximally busy → best possible frame rate.
    if (this.state.isSeeking) {
      this.pendingSeekTime = time;
      return;
    }

    // Decoder is idle — start seeking now
    this.setSeeking(true);
    this.video.currentTime = time;
    this.syncTimedMediaElementTime(
      this.webcamVideo,
      time,
      this.getWebcamOffsetSec(),
    );
    this.syncAudioElementTime(this.deviceAudio, time);
    this.syncTimedMediaElementTime(
      this.micAudio,
      time,
      this.getMicAudioOffsetSec(),
    );
  }

  /** Flush any pending seek immediately (call on drag end). */
  public flushPendingSeek() {
    if (this.pendingSeekTime !== null && !this.state.isSeeking) {
      const t = this.pendingSeekTime;
      this.pendingSeekTime = null;
      const clamped = this.renderOptions?.segment
        ? (getNextPlayableTime(
            t,
            this.renderOptions.segment,
            this.getEffectiveDuration(t),
          ) ??
          clampToTrimSegments(
            t,
            this.renderOptions.segment,
            this.getEffectiveDuration(t),
          ))
        : t;
      this.armPlaybackRecoveryAnchor(clamped);
      this.setSeeking(true);
      this.video.currentTime = clamped;
      this.syncTimedMediaElementTime(
        this.webcamVideo,
        clamped,
        this.getWebcamOffsetSec(),
      );
      this.syncAudioElementTime(this.deviceAudio, clamped);
      this.syncTimedMediaElementTime(
        this.micAudio,
        clamped,
        this.getMicAudioOffsetSec(),
      );
      this.setCurrentTime(clamped);
    }
  }

  public togglePlayPause() {
    if (this.video.paused) {
      this.play();
    } else {
      this.pause();
    }
  }

  public destroy() {
    if (this.renderTimeout !== null) {
      cancelAnimationFrame(this.renderTimeout);
      this.renderTimeout = null;
    }
    if (this.seekSafetyTimer !== null) {
      clearTimeout(this.seekSafetyTimer);
      this.seekSafetyTimer = null;
    }
    this.stopPlaybackMonitor();
    videoRenderer.stopAnimation();
    this.video.removeEventListener("loadeddata", this.handleLoadedData);
    this.video.removeEventListener("play", this.handlePlay);
    this.video.removeEventListener("pause", this.handlePause);
    this.video.removeEventListener("timeupdate", this.handleTimeUpdate);
    this.video.removeEventListener("seeked", this.handleSeeked);
    this.video.removeEventListener("waiting", this.handleWaiting);
    this.video.removeEventListener("playing", this.handlePlaying);
    if (this.waitingStallTimer !== null) {
      clearTimeout(this.waitingStallTimer);
      this.waitingStallTimer = null;
    }
    this.video.removeEventListener("loadedmetadata", this.handleLoadedMetadata);
    this.video.removeEventListener("durationchange", this.handleDurationChange);
    this.video.removeEventListener("error", this.handleError);
    if (this.webcamVideo) {
      this.clearMediaElementSource(this.webcamVideo);
    }
    if (this.deviceAudio) {
      this.clearMediaElementSource(this.deviceAudio);
    }
    if (this.micAudio) {
      this.clearMediaElementSource(this.micAudio);
    }
  }

  // Getters
  public get isPlaying() {
    return this.state.isPlaying;
  }
  public get isReady() {
    return this.state.isReady;
  }
  public get isSeeking() {
    return this.state.isSeeking;
  }
  public get currentTime() {
    return this.state.currentTime;
  }
  public get duration() {
    return this.state.duration;
  }

  // Add this new method
  public async loadVideo({
    videoBlob,
    videoUrl,
    onLoadingProgress,
    debugLabel,
  }: {
    videoBlob?: Blob;
    videoUrl?: string;
    onLoadingProgress?: (p: number) => void;
    debugLabel?: string;
  }): Promise<string> {
    try {
      // Clear previous audio
      if (this.webcamVideo) {
        this.clearMediaElementSource(this.webcamVideo);
      }
      if (this.deviceAudio) {
        this.clearMediaElementSource(this.deviceAudio);
      }
      if (this.micAudio) {
        this.clearMediaElementSource(this.micAudio);
      }

      let blob: Blob;

      if (videoBlob) {
        blob = videoBlob;
      } else if (videoUrl?.startsWith("blob:") || isNativeMediaUrl(videoUrl)) {
        const directVideoUrl = videoUrl!;
        await this.handleVideoSourceChange(directVideoUrl, debugLabel);
        onLoadingProgress?.(100);
        return directVideoUrl;
      } else if (videoUrl) {
        const response = await fetch(videoUrl);
        if (!response.ok) throw new Error("Failed to fetch video");

        const reader = response.body!.getReader();
        const contentLength = +(response.headers.get("Content-Length") ?? 0);
        let receivedLength = 0;
        const chunks = [];

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          chunks.push(value);
          receivedLength += value.length;
          const progress = Math.min(
            (receivedLength / contentLength) * 100,
            100,
          );
          onLoadingProgress?.(progress);
        }

        blob = new Blob(chunks, { type: "video/mp4" });
        if (blob.size === 0) {
          throw new Error(
            "Recording failed: 0 frames captured. If you used Window Capture, ensure the window was visible and updating on screen.",
          );
        }
      } else {
        throw new Error("No video data provided");
      }

      const objectUrl = URL.createObjectURL(blob);
      await this.handleVideoSourceChange(objectUrl, debugLabel);
      return objectUrl;
    } catch (error) {
      throw error;
    }
  }

  private async loadMediaElement(
    element: HTMLMediaElement | undefined,
    label: string,
    {
      audioBlob,
      audioUrl,
      onLoadingProgress,
    }: {
      audioBlob?: Blob;
      audioUrl?: string;
      onLoadingProgress?: (p: number) => void;
    },
  ): Promise<string> {
    try {
      if (!element) return "";

      let blob: Blob;

      if (audioBlob) {
        blob = audioBlob;
      } else if (audioUrl?.startsWith("blob:")) {
        element.src = audioUrl;
        element.load();
        return audioUrl;
      } else if (isNativeMediaUrl(audioUrl)) {
        const directUrl = audioUrl!;
        element.src = directUrl;
        element.load();
        return directUrl;
      } else if (audioUrl) {
        const response = await fetch(audioUrl);
        if (!response.ok) throw new Error(`Failed to fetch ${label}`);

        const reader = response.body!.getReader();
        const contentLength = +(response.headers.get("Content-Length") ?? 0);
        let receivedLength = 0;
        const chunks = [];

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          chunks.push(value);
          receivedLength += value.length;
          const progress = Math.min(
            (receivedLength / contentLength) * 100,
            100,
          );
          onLoadingProgress?.(progress);
        }

        blob = new Blob(chunks, {
          type: element instanceof HTMLVideoElement ? "video/mp4" : "audio/wav",
        });
      } else {
        this.clearMediaElementSource(element);
        return "";
      }

      const objectUrl = URL.createObjectURL(blob);
      element.src = objectUrl;
      element.load();

      return objectUrl;
    } catch (error) {
      console.error(`[${label}]`, error);
      return "";
    }
  }

  public async loadDeviceAudio({
    audioBlob,
    audioUrl,
    onLoadingProgress,
  }: {
    audioBlob?: Blob;
    audioUrl?: string;
    onLoadingProgress?: (p: number) => void;
  }): Promise<string> {
    return this.loadMediaElement(this.deviceAudio, "DeviceAudioLoad", {
      audioBlob,
      audioUrl,
      onLoadingProgress,
    });
  }

  public async loadMicAudio({
    audioBlob,
    audioUrl,
    onLoadingProgress,
  }: {
    audioBlob?: Blob;
    audioUrl?: string;
    onLoadingProgress?: (p: number) => void;
  }): Promise<string> {
    return this.loadMediaElement(this.micAudio, "MicAudioLoad", {
      audioBlob,
      audioUrl,
      onLoadingProgress,
    });
  }

  public async loadWebcamVideo({
    videoBlob,
    videoUrl,
    onLoadingProgress,
  }: {
    videoBlob?: Blob;
    videoUrl?: string;
    onLoadingProgress?: (p: number) => void;
  }): Promise<string> {
    return this.loadMediaElement(this.webcamVideo, "WebcamVideoLoad", {
      audioBlob: videoBlob,
      audioUrl: videoUrl,
      onLoadingProgress,
    });
  }

  // Update existing method to be private
  private async handleVideoSourceChange(
    videoUrl: string,
    debugLabel?: string,
  ): Promise<void> {
    if (!this.video || !this.canvas) return;

    this.isChangingSource = true;
    this.pendingSourceChangeLabel = debugLabel ?? null;

    // Fully reset transient playback/render state so trim bounds, pending seeks,
    // and animation context from the previous project cannot leak into this load.
    this.setReady(false);
    this.resetTransientPlaybackState();

    // Reset video element
    this.clearMediaElementSource(this.video);

    return new Promise<void>((resolve) => {
      const handleCanPlayThrough = () => {
        this.video.removeEventListener("canplaythrough", handleCanPlayThrough);

        // Set up canvas
        this.canvas.width = this.video.videoWidth;
        this.canvas.height = this.video.videoHeight;

        const ctx = this.canvas.getContext("2d");
        if (ctx) {
          ctx.imageSmoothingEnabled = true;
          ctx.imageSmoothingQuality = "high";
        }

        this.isChangingSource = false;
        this.pendingSourceChangeLabel = null;
        this.setReady(true);
        resolve();
      };

      // Set up video
      this.video.addEventListener("canplaythrough", handleCanPlayThrough);
      this.video.preload = "auto";
      this.video.src = videoUrl;
      this.video.load();
    });
  }

  // Add this new method to handle time adjustment
  private getAdjustedTime(time: number): number {
    if (!this.renderOptions?.segment) return time;
    return clampToTrimSegments(
      time,
      this.renderOptions.segment,
      this.getEffectiveDuration(time),
    );
  }

  // Add new method
  public initializeSegment(): VideoSegment {
    // If duration is available, use it, otherwise use a default safe large number
    // It will be corrected by handleDurationChange later
    const duration =
      this.video &&
      this.video.duration !== Infinity &&
      !isNaN(this.video.duration)
        ? this.video.duration
        : 3600;

    const initialSegment: VideoSegment = {
      trimStart: 0,
      trimEnd: duration,
      trimSegments: [
        {
          id: crypto.randomUUID(),
          startTime: 0,
          endTime: duration,
        },
      ],
      zoomKeyframes: [],
      textSegments: [],
      speedPoints: [
        { time: 0, speed: 1 },
        { time: duration, speed: 1 },
      ],
      deviceAudioPoints: buildFlatDeviceAudioPoints(duration),
      micAudioPoints: buildFlatMicAudioPoints(duration),
      deviceAudioAvailable: true,
      micAudioAvailable: false,
    };
    return initialSegment;
  }

  // Add this new method
  public isAtEnd(): boolean {
    if (!this.renderOptions?.segment) return false;
    const segs = getTrimSegments(
      this.renderOptions.segment,
      this.getEffectiveDuration(this.video.currentTime),
    );
    const trimEnd = segs[segs.length - 1].endTime;
    return Math.abs(this.video.currentTime - trimEnd) < 0.1; // Allow 0.1s tolerance
  }
}

export const createVideoController = (options: VideoControllerOptions) => {
  return new VideoController(options);
};
