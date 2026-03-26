import { videoRenderer } from "./videoRenderer";
import type { VideoSegment } from "@/types/video";
import { getTrimSegments } from "./trimSegments";
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

// Split modules
import type {
  VideoControllerOptions,
  VideoState,
  RenderOptions,
} from "./videoControllerTypes";
import { SEEK_SAFETY_TIMEOUT_MS } from "./videoControllerTypes";

import {
  hasValidMediaElement,
  syncTimedMediaElementTime,
  syncAudioElementTime,
  clearMediaElementSource,
} from "./videoControllerMediaSync";

import {
  loadMediaElement,
  fetchVideoSource,
  performVideoSourceChange,
} from "./videoControllerMediaLoading";

import {
  type StallState,
  type RecoveryState,
  type ResetLogState,
  createStallState,
  createRecoveryState,
  createResetLogState,
  cleanupStallState,
  clearRecoveryAnchor,
  armRecoveryAnchor,
  maybeLogPlaybackReset,
  enforceSegmentPlaybackBounds,
} from "./videoControllerStallRecovery";

import {
  type ControllerInternals,
  doHandlePlay,
  doHandlePause,
  doHandleWaiting,
  doHandlePlaying,
  doHandleTimeUpdate,
  doHandleSeeked,
  doPlay,
  doSeek,
  doFlushPendingSeek,
} from "./videoControllerPlayback";

import {
  type RenderHost,
  renderFrame,
  generateThumbnail,
  renderImmediate,
} from "./videoControllerRendering";

// Re-export public types so consumers can keep importing from this file
export type { VideoControllerOptions, VideoState, RenderOptions };

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
  private pendingSourceChangeLabel: string | null = null;
  private playRequestSeq = 0;
  private webcamVideoPlayPromise: Promise<void> | null = null;
  private deviceAudioPlayPromise: Promise<void> | null = null;
  private micAudioPlayPromise: Promise<void> | null = null;

  private stallState: StallState;
  private recoveryState: RecoveryState;
  private resetLogState: ResetLogState;

  constructor(options: VideoControllerOptions) {
    this.video = options.videoRef;
    this.webcamVideo = options.webcamVideoRef;
    this.deviceAudio = options.deviceAudioRef;
    this.micAudio = options.micAudioRef;
    this.canvas = options.canvasRef;
    this.tempCanvas = options.tempCanvasRef;
    this.options = options;

    this.state = {
      isPlaying: false, isReady: false, isSeeking: false,
      currentTime: 0, duration: 0,
    };

    this.stallState = createStallState();
    this.recoveryState = createRecoveryState();
    this.resetLogState = createResetLogState();
    this.initializeEventListeners();
  }

  // -------------------------------------------------------------------
  // Internals accessor for extracted playback module
  // -------------------------------------------------------------------

  private get internals(): ControllerInternals {
    return {
      video: this.video, webcamVideo: this.webcamVideo,
      deviceAudio: this.deviceAudio, micAudio: this.micAudio,
      canvas: this.canvas, tempCanvas: this.tempCanvas,
      state: this.state, renderOptions: this.renderOptions,
      isChangingSource: this.isChangingSource,
      isGeneratingThumbnail: this.isGeneratingThumbnail,
      pendingSeekTime: this.pendingSeekTime,
      lastRequestedSeekTime: this.lastRequestedSeekTime,
      SEGMENT_EPS: this.SEGMENT_EPS,
      playRequestSeq: this.playRequestSeq,
      webcamVideoPlayPromise: this.webcamVideoPlayPromise,
      deviceAudioPlayPromise: this.deviceAudioPlayPromise,
      micAudioPlayPromise: this.micAudioPlayPromise,
      stallState: this.stallState,
      recoveryState: this.recoveryState,
      resetLogState: this.resetLogState,
      hasValidWebcamVideo: this.hasValidWebcamVideo,
      hasValidMicAudio: this.hasValidMicAudio,
      hasValidDeviceAudio: this.hasValidDeviceAudio,
      hasExternalAudio: this.hasExternalAudio,
      getWebcamOffsetSec: () => this.getWebcamOffsetSec(),
      getMicAudioOffsetSec: () => this.getMicAudioOffsetSec(),
      getSpeed: (t) => this.getSpeed(t),
      getEffectiveDuration: (f) => this.getEffectiveDuration(f),
      getSegmentStartTime: (r) => this.getSegmentStartTime(r),
      syncAllMediaToTime: (t) => this.syncAllMediaToTime(t),
      applyAudioTrackVolumes: (t) => this.applyAudioTrackVolumes(t),
      setPlaying: (p) => this.setPlaying(p),
      setReady: (r) => this.setReady(r),
      setSeeking: (s) => this.setSeeking(s),
      setCurrentTime: (t) => this.setCurrentTime(t),
      renderFrame: () => this.doRenderFrame(),
      armPlaybackRecoveryAnchor: (t) => this.armPlaybackRecoveryAnchor(t),
      onBufferingChange: this.options.onBufferingChange,
      startPlaybackMonitor: () => this.startPlaybackMonitor(),
      stopPlaybackMonitor: () => this.stopPlaybackMonitor(),
    };
  }

  private syncBackFromInternals(ci: ControllerInternals) {
    this.pendingSeekTime = ci.pendingSeekTime;
    this.lastRequestedSeekTime = ci.lastRequestedSeekTime;
    this.playRequestSeq = ci.playRequestSeq;
    this.webcamVideoPlayPromise = ci.webcamVideoPlayPromise;
    this.deviceAudioPlayPromise = ci.deviceAudioPlayPromise;
    this.micAudioPlayPromise = ci.micAudioPlayPromise;
  }

  private delegate(fn: (ci: ControllerInternals) => void) {
    const ci = this.internals;
    fn(ci);
    this.syncBackFromInternals(ci);
  }

  // -------------------------------------------------------------------
  // Render host (for extracted rendering module)
  // -------------------------------------------------------------------

  private get renderHost(): RenderHost {
    return {
      video: this.video, webcamVideo: this.webcamVideo,
      canvas: this.canvas, tempCanvas: this.tempCanvas,
      renderOptions: this.renderOptions,
      getEffectiveDuration: (f) => this.getEffectiveDuration(f),
    };
  }

  private doRenderFrame() { renderFrame(this.renderHost); }

  // -------------------------------------------------------------------
  // Event listeners
  // -------------------------------------------------------------------

  private initializeEventListeners() {
    this.video.addEventListener("loadeddata", this.handleLoadedData);
    this.video.addEventListener("play", this.onPlay);
    this.video.addEventListener("pause", this.onPause);
    this.video.addEventListener("timeupdate", this.onTimeUpdate);
    this.video.addEventListener("seeked", this.onSeeked);
    this.video.addEventListener("waiting", this.onWaiting);
    this.video.addEventListener("playing", this.onPlaying);
    this.video.addEventListener("loadedmetadata", this.handleLoadedMetadata);
    this.video.addEventListener("durationchange", this.handleDurationChange);
    this.video.addEventListener("error", this.handleError);
  }

  // -------------------------------------------------------------------
  // Media element helpers
  // -------------------------------------------------------------------

  private get hasValidDeviceAudio() { return hasValidMediaElement(this.deviceAudio); }
  private get hasValidMicAudio() { return hasValidMediaElement(this.micAudio); }
  private get hasValidWebcamVideo() { return hasValidMediaElement(this.webcamVideo); }
  private get hasExternalAudio() { return this.hasValidDeviceAudio || this.hasValidMicAudio; }

  private getMicAudioOffsetSec(): number {
    const offset = this.renderOptions?.segment?.micAudioOffsetSec ?? 0;
    return Number.isFinite(offset) ? offset : 0;
  }

  private getWebcamOffsetSec(): number {
    const offset = this.renderOptions?.segment?.webcamOffsetSec ?? 0;
    return Number.isFinite(offset) ? offset : 0;
  }

  private syncAllMediaToTime(time: number) {
    syncTimedMediaElementTime(this.webcamVideo, time, this.getWebcamOffsetSec());
    syncAudioElementTime(this.deviceAudio, time);
    syncTimedMediaElementTime(this.micAudio, time, this.getMicAudioOffsetSec());
  }

  // -------------------------------------------------------------------
  // Audio volume
  // -------------------------------------------------------------------

  private getSpeed(time: number): number {
    if (!this.renderOptions?.segment?.speedPoints?.length) return 1.0;
    return getSpeedAtTime(time, this.renderOptions.segment.speedPoints);
  }

  private applyAudioTrackVolumes(time: number = this.video.currentTime) {
    const deviceVol = clampDeviceAudioVolume(
      getDeviceAudioVolumeAtTime(time, this.renderOptions?.segment?.deviceAudioPoints),
    );
    const micVol = clampMicAudioVolume(
      getMicAudioVolumeAtTime(time, this.renderOptions?.segment?.micAudioPoints),
    );
    if (this.hasValidDeviceAudio && this.deviceAudio) this.deviceAudio.volume = deviceVol;
    if (this.hasValidMicAudio && this.micAudio) this.micAudio.volume = micVol;
    if (this.hasExternalAudio) { this.video.muted = true; return; }
    this.video.muted = false;
    this.video.volume = deviceVol;
  }

  // -------------------------------------------------------------------
  // Event handlers (delegated)
  // -------------------------------------------------------------------

  private handleLoadedData = () => {
    if (this.isChangingSource) return;
    this.applyAudioTrackVolumes(this.video.currentTime);
    this.doRenderFrame();
    this.setReady(true);
  };

  private onPlay = () => { this.delegate(doHandlePlay); };
  private onPause = () => { this.delegate(doHandlePause); };
  private onWaiting = () => { this.delegate(doHandleWaiting); };
  private onPlaying = () => { this.delegate(doHandlePlaying); };
  private onTimeUpdate = () => { this.delegate(doHandleTimeUpdate); };
  private onSeeked = () => { this.delegate(doHandleSeeked); };

  private handleLoadedMetadata = () => {
    this.options.onMetadataLoaded?.({
      duration: this.video.duration,
      width: this.video.videoWidth,
      height: this.video.videoHeight,
    });
    if (this.video.duration !== Infinity) {
      this.setDuration(this.video.duration);
      if (!this.renderOptions?.segment) {
        this.renderOptions = {
          segment: this.initializeSegment(),
          backgroundConfig: {
            scale: 100, borderRadius: 8,
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
    if (isIntentionalResetError) return;
    console.error("Video error:", mediaError?.message || "Unknown error", `(code: ${mediaError?.code})`);
    this.options.onError?.(mediaError?.message || "Unknown video error");
  };

  // -------------------------------------------------------------------
  // Playback monitor
  // -------------------------------------------------------------------

  private startPlaybackMonitor() {
    this.stopPlaybackMonitor();
    const loop = () => {
      if (this.video.paused) { this.playbackMonitorRaf = null; return; }
      if (!this.state.isSeeking && this.renderOptions?.segment) {
        enforceSegmentPlaybackBounds(
          this.video, this.renderOptions.segment,
          this.getEffectiveDuration(this.video.currentTime),
          this.video.currentTime, true, this.SEGMENT_EPS,
          (t) => this.syncAllMediaToTime(t),
          (t) => this.setCurrentTime(t),
        );
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

  // -------------------------------------------------------------------
  // State setters
  // -------------------------------------------------------------------

  private resetTransientPlaybackState() {
    this.playRequestSeq += 1;
    clearRecoveryAnchor(this.recoveryState);
    maybeLogPlaybackReset(
      this.resetLogState, "source-change-reset",
      this.state.currentTime, 0,
      this.renderOptions?.segment, this.SEGMENT_EPS,
      this.getEffectiveDuration(this.state.currentTime),
      { sourceChange: this.pendingSourceChangeLabel,
        isReady: this.state.isReady, isPlaying: this.state.isPlaying },
    );
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
    if (this.seekSafetyTimer !== null) { clearTimeout(this.seekSafetyTimer); this.seekSafetyTimer = null; }
    if (seeking) {
      this.seekSafetyTimer = setTimeout(() => {
        this.seekSafetyTimer = null;
        if (this.state.isSeeking) {
          console.warn("[VideoController] Seek safety timeout — unsticking isSeeking");
          this.state.isSeeking = false;
          if (this.pendingSeekTime !== null) {
            const t = this.pendingSeekTime;
            this.pendingSeekTime = null;
            this.seek(t);
          }
        }
      }, SEEK_SAFETY_TIMEOUT_MS);
    }
  }

  private setCurrentTime(time: number) { this.state.currentTime = time; this.options.onTimeUpdate?.(time); }
  private setDuration(duration: number) { this.state.duration = duration; this.options.onDurationChange?.(duration); }

  // -------------------------------------------------------------------
  // Segment / duration helpers
  // -------------------------------------------------------------------

  private getSegmentStartTime(referenceTime: number): number {
    if (!this.renderOptions?.segment) return 0;
    return getTrimSegments(this.renderOptions.segment, this.getEffectiveDuration(referenceTime))[0]?.startTime ?? 0;
  }

  private getEffectiveDuration(fallback: number): number {
    if (Number.isFinite(this.video.duration) && this.video.duration > 0) return this.video.duration;
    if (Number.isFinite(this.state.duration) && this.state.duration > 0) return this.state.duration;
    // fallback from segment data
    if (!this.renderOptions?.segment) return Math.max(fallback, 0);
    const trimSegments = this.renderOptions.segment.trimSegments ?? [];
    const maxTrimEnd = trimSegments.reduce((max, ts) => Math.max(max, ts.endTime), this.renderOptions.segment.trimEnd);
    return Math.max(maxTrimEnd, fallback, 0);
  }

  private armPlaybackRecoveryAnchor(time: number) {
    armRecoveryAnchor(this.recoveryState, time, this.getSegmentStartTime(time), this.SEGMENT_EPS);
  }

  // -------------------------------------------------------------------
  // Public API
  // -------------------------------------------------------------------

  public async generateThumbnail(options: RenderOptions): Promise<string | undefined> {
    return generateThumbnail(
      this.renderHost, options,
      (v) => { this.isGeneratingThumbnail = v; },
      () => this.doRenderFrame(),
    );
  }

  public renderImmediate(options: RenderOptions) {
    renderImmediate(
      { ...this.renderHost, renderOptions: options },
      options,
      () => this.applyAudioTrackVolumes(this.video.currentTime),
    );
    this.renderOptions = options;
  }

  public updateRenderOptions(options: RenderOptions) {
    this.renderOptions = options;
    this.applyAudioTrackVolumes(this.video.currentTime);
    if (this.renderTimeout === null) {
      this.renderTimeout = requestAnimationFrame(() => { this.doRenderFrame(); this.renderTimeout = null; });
    }
  }

  public play() { this.delegate(doPlay); }
  public pause() { this.video.pause(); }
  public seek(time: number) { this.delegate((ci) => doSeek(ci, time)); }
  public flushPendingSeek() { this.delegate(doFlushPendingSeek); }
  public togglePlayPause() { if (this.video.paused) this.play(); else this.pause(); }

  public destroy() {
    if (this.renderTimeout !== null) { cancelAnimationFrame(this.renderTimeout); this.renderTimeout = null; }
    if (this.seekSafetyTimer !== null) { clearTimeout(this.seekSafetyTimer); this.seekSafetyTimer = null; }
    this.stopPlaybackMonitor();
    videoRenderer.stopAnimation();
    cleanupStallState(this.stallState);
    this.video.removeEventListener("loadeddata", this.handleLoadedData);
    this.video.removeEventListener("play", this.onPlay);
    this.video.removeEventListener("pause", this.onPause);
    this.video.removeEventListener("timeupdate", this.onTimeUpdate);
    this.video.removeEventListener("seeked", this.onSeeked);
    this.video.removeEventListener("waiting", this.onWaiting);
    this.video.removeEventListener("playing", this.onPlaying);
    this.video.removeEventListener("loadedmetadata", this.handleLoadedMetadata);
    this.video.removeEventListener("durationchange", this.handleDurationChange);
    this.video.removeEventListener("error", this.handleError);
    if (this.webcamVideo) clearMediaElementSource(this.webcamVideo);
    if (this.deviceAudio) clearMediaElementSource(this.deviceAudio);
    if (this.micAudio) clearMediaElementSource(this.micAudio);
  }

  // Getters
  public get isPlaying() { return this.state.isPlaying; }
  public get isReady() { return this.state.isReady; }
  public get isSeeking() { return this.state.isSeeking; }
  public get currentTime() { return this.state.currentTime; }
  public get duration() { return this.state.duration; }

  // -------------------------------------------------------------------
  // Media loading
  // -------------------------------------------------------------------

  public async loadVideo(args: {
    videoBlob?: Blob; videoUrl?: string;
    onLoadingProgress?: (p: number) => void; debugLabel?: string;
  }): Promise<string> {
    return fetchVideoSource(args, this.webcamVideo, this.deviceAudio, this.micAudio,
      (url, label) => this.handleVideoSourceChange(url, label));
  }

  public async loadDeviceAudio(args: {
    audioBlob?: Blob; audioUrl?: string; onLoadingProgress?: (p: number) => void;
  }): Promise<string> { return loadMediaElement(this.deviceAudio, "DeviceAudioLoad", args); }

  public async loadMicAudio(args: {
    audioBlob?: Blob; audioUrl?: string; onLoadingProgress?: (p: number) => void;
  }): Promise<string> { return loadMediaElement(this.micAudio, "MicAudioLoad", args); }

  public async loadWebcamVideo(args: {
    videoBlob?: Blob; videoUrl?: string; onLoadingProgress?: (p: number) => void;
  }): Promise<string> {
    return loadMediaElement(this.webcamVideo, "WebcamVideoLoad", {
      audioBlob: args.videoBlob, audioUrl: args.videoUrl, onLoadingProgress: args.onLoadingProgress,
    });
  }

  private async handleVideoSourceChange(videoUrl: string, debugLabel?: string): Promise<void> {
    if (!this.video || !this.canvas) return;
    this.isChangingSource = true;
    this.pendingSourceChangeLabel = debugLabel ?? null;
    this.setReady(false);
    this.resetTransientPlaybackState();
    await performVideoSourceChange(this.video, this.canvas, videoUrl,
      () => {},
      () => { this.isChangingSource = false; this.pendingSourceChangeLabel = null; this.setReady(true); },
    );
  }

  // -------------------------------------------------------------------
  // Segment initialization
  // -------------------------------------------------------------------

  public initializeSegment(): VideoSegment {
    const dur = this.video && this.video.duration !== Infinity && !isNaN(this.video.duration)
      ? this.video.duration : 3600;
    return {
      trimStart: 0, trimEnd: dur,
      trimSegments: [{ id: crypto.randomUUID(), startTime: 0, endTime: dur }],
      zoomKeyframes: [], textSegments: [],
      speedPoints: [{ time: 0, speed: 1 }, { time: dur, speed: 1 }],
      deviceAudioPoints: buildFlatDeviceAudioPoints(dur),
      micAudioPoints: buildFlatMicAudioPoints(dur),
      deviceAudioAvailable: true, micAudioAvailable: false,
    };
  }

  public isAtEnd(): boolean {
    if (!this.renderOptions?.segment) return false;
    const segs = getTrimSegments(this.renderOptions.segment, this.getEffectiveDuration(this.video.currentTime));
    return Math.abs(this.video.currentTime - segs[segs.length - 1].endTime) < 0.1;
  }
}

export const createVideoController = (options: VideoControllerOptions) => {
  return new VideoController(options);
};
