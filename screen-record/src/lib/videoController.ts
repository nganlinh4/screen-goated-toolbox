import { videoRenderer } from './videoRenderer';
import type { VideoSegment, BackgroundConfig, MousePosition } from '@/types/video';
import { clampToTrimSegments, getNextPlayableTime, getTrimSegments } from './trimSegments';

interface VideoControllerOptions {
  videoRef: HTMLVideoElement;
  audioRef?: HTMLAudioElement;
  canvasRef: HTMLCanvasElement;
  tempCanvasRef: HTMLCanvasElement;
  onTimeUpdate?: (time: number) => void;
  onPlayingChange?: (isPlaying: boolean) => void;
  onVideoReady?: (ready: boolean) => void;
  onError?: (error: string) => void;
  onDurationChange?: (duration: number) => void;
  onMetadataLoaded?: (metadata: { duration: number, width: number, height: number }) => void;
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
  mousePositions: MousePosition[];
}


export class VideoController {
  private video: HTMLVideoElement;
  private audio?: HTMLAudioElement;
  private canvas: HTMLCanvasElement;
  private tempCanvas: HTMLCanvasElement;
  private options: VideoControllerOptions;
  private state: VideoState;
  private renderOptions?: RenderOptions;
  private isChangingSource = false;
  private isGeneratingThumbnail = false;
  private audioPlayPromise: Promise<void> | null = null;
  private pendingSeekTime: number | null = null;
  private readonly SEGMENT_EPS = 0.03;
  private playbackMonitorRaf: number | null = null;

  constructor(options: VideoControllerOptions) {
    this.video = options.videoRef;
    this.audio = options.audioRef;
    this.canvas = options.canvasRef;
    this.tempCanvas = options.tempCanvasRef;
    this.options = options;

    this.state = {
      isPlaying: false,
      isReady: false,
      isSeeking: false,
      currentTime: 0,
      duration: 0
    };

    this.initializeEventListeners();
  }

  private initializeEventListeners() {
    console.log('[VideoController] Initializing listeners (v2-no-waiting)');
    this.video.addEventListener('loadeddata', this.handleLoadedData);
    this.video.addEventListener('play', this.handlePlay);
    this.video.addEventListener('pause', this.handlePause);
    this.video.addEventListener('timeupdate', this.handleTimeUpdate);
    this.video.addEventListener('seeked', this.handleSeeked);
    this.video.addEventListener('loadedmetadata', this.handleLoadedMetadata);
    this.video.addEventListener('durationchange', this.handleDurationChange);
    this.video.addEventListener('error', this.handleError);
  }

  private handleLoadedData = () => {
    console.log('[VideoController] Video loaded data');
    // During source changes, canplaythrough handler manages ready state & rendering
    if (this.isChangingSource) return;
    this.renderFrame();
    this.setReady(true);
  };

  private handlePlay = () => {
    console.log('[VideoController] Play event');
    if (this.audio) {
      this.audio.currentTime = this.video.currentTime;
      this.audio.playbackRate = this.video.playbackRate;
      // Store promise so handlePause can await it before pausing (avoids AbortError)
      this.audioPlayPromise = this.audio.play().catch(() => {});
    }

    // Ensure animation is running
    if (this.renderOptions) {
      videoRenderer.startAnimation({
        video: this.video,
        canvas: this.canvas,
        tempCanvas: this.tempCanvas,
        segment: this.renderOptions.segment,
        backgroundConfig: this.renderOptions.backgroundConfig,
        mousePositions: this.renderOptions.mousePositions,
        currentTime: this.video.currentTime
      });
    }

    this.startPlaybackMonitor();
    this.setPlaying(true);
  };

  private handlePause = () => {
    console.log('[VideoController] Pause event');
    if (this.audio) {
      // Wait for pending play() promise before pausing to avoid AbortError
      const promise = this.audioPlayPromise;
      this.audioPlayPromise = null;
      if (promise) {
        promise.then(() => this.audio?.pause()).catch(() => {});
      } else {
        this.audio.pause();
      }
    }
    this.stopPlaybackMonitor();
    this.setPlaying(false);
    // Intentionally NOT re-drawing here. The last animation frame stays visible.
    // Re-drawing on pause can cause a visual shift because the video decoder may
    // have advanced video.currentTime slightly beyond the last rendered frame.
    // Seeking (handleSeeked) and edits (updateRenderOptions) still trigger draws.
  };

  private handleTimeUpdate = () => {
    if (this.isGeneratingThumbnail) return;
    if (!this.state.isSeeking) {
      const currentTime = this.video.currentTime;

      // Handle segmented trim bounds
      if (this.renderOptions?.segment) {
        const corrected = this.enforceSegmentPlaybackBounds(currentTime, false);
        if (corrected !== null) return;
      }

      // Smooth audio sync: only correct if drift > 150ms to avoid audio stutter
      if (this.audio && !this.video.paused) {
        const drift = Math.abs(this.video.currentTime - this.audio.currentTime);
        if (drift > 0.15) {
          this.audio.currentTime = this.video.currentTime;
        }
      }

      this.setCurrentTime(currentTime);
      // Removed renderFrame here - allow animation loop to handle updates during playback
      // If paused, handlePause triggers renderFrame.
      // If playing, startAnimation loop handles it.
    }
  };

  private handleSeeked = () => {
    if (this.isGeneratingThumbnail) return;
    this.setSeeking(false);

    // Render the just-decoded frame immediately
    this.renderFrame();

    // If there's a queued seek (from drag moves while decoder was busy),
    // start the next seek immediately to keep the decoder maximally busy.
    if (this.pendingSeekTime !== null) {
      const t = this.pendingSeekTime;
      this.pendingSeekTime = null;
      this.setSeeking(true);
      const clamped = this.renderOptions?.segment
        ? (getNextPlayableTime(t, this.renderOptions.segment, this.video.duration || this.state.duration || t)
            ?? clampToTrimSegments(t, this.renderOptions.segment, this.video.duration || this.state.duration || t))
        : t;
      this.setCurrentTime(clamped);
      this.video.currentTime = clamped;
      if (this.audio) this.audio.currentTime = clamped;
    } else {
      const clamped = this.renderOptions?.segment
        ? (getNextPlayableTime(this.video.currentTime, this.renderOptions.segment, this.video.duration || this.state.duration || this.video.currentTime)
            ?? clampToTrimSegments(this.video.currentTime, this.renderOptions.segment, this.video.duration || this.state.duration || this.video.currentTime))
        : this.video.currentTime;
      if (Math.abs(clamped - this.video.currentTime) > 0.001) {
        this.video.currentTime = clamped;
        if (this.audio) this.audio.currentTime = clamped;
      }
      this.setCurrentTime(clamped);
    }
  };

  private enforceSegmentPlaybackBounds(currentTime: number, forceTransitionAtEnd: boolean): number | null {
    if (!this.renderOptions?.segment) return null;

    const segs = getTrimSegments(this.renderOptions.segment, this.video.duration || this.state.duration || currentTime);
    const last = segs[segs.length - 1];
    const TRANSITION_EPS = forceTransitionAtEnd ? 0.003 : this.SEGMENT_EPS;

    const currentSegIndex = segs.findIndex(
      s => currentTime >= s.startTime - this.SEGMENT_EPS && currentTime <= s.endTime + this.SEGMENT_EPS
    );
    const isInside = currentSegIndex >= 0;

    if (!isInside) {
      const nextTime = getNextPlayableTime(currentTime, this.renderOptions.segment, this.video.duration || this.state.duration || currentTime);
      if (nextTime !== null && nextTime - currentTime > this.SEGMENT_EPS) {
        this.video.currentTime = nextTime;
        if (this.audio) this.audio.currentTime = nextTime;
        this.setCurrentTime(nextTime);
        return nextTime;
      }
      if (nextTime !== null && Math.abs(nextTime - currentTime) <= this.SEGMENT_EPS) {
        this.setCurrentTime(nextTime);
        return nextTime;
      }
      if (currentTime >= last.endTime - TRANSITION_EPS && !this.video.paused) {
        this.video.currentTime = last.endTime;
        if (this.audio) this.audio.currentTime = last.endTime;
        this.setCurrentTime(last.endTime);
        this.video.pause();
        return last.endTime;
      }
      return null;
    }

    const currentSeg = segs[currentSegIndex];
    if (currentSeg && currentTime >= currentSeg.endTime - TRANSITION_EPS && !this.video.paused) {
      const next = segs[currentSegIndex + 1];
      if (next && next.startTime - currentTime > this.SEGMENT_EPS) {
        this.video.currentTime = next.startTime;
        if (this.audio) this.audio.currentTime = next.startTime;
        this.setCurrentTime(next.startTime);
        return next.startTime;
      }
      if (next && Math.abs(next.startTime - currentTime) <= this.SEGMENT_EPS) {
        this.setCurrentTime(next.startTime);
        return next.startTime;
      }
      this.video.currentTime = currentSeg.endTime;
      if (this.audio) this.audio.currentTime = currentSeg.endTime;
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
    console.log('Video metadata loaded:', {
      duration: this.video.duration,
      width: this.video.videoWidth,
      height: this.video.videoHeight
    });

    this.options.onMetadataLoaded?.({
      duration: this.video.duration,
      width: this.video.videoWidth,
      height: this.video.videoHeight
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
            backgroundType: 'solid'
          },
          mousePositions: []
        };
      }
    }
  };

  private handleDurationChange = () => {
    console.log('[VideoController] Duration changed:', this.video.duration);
    if (this.video.duration !== Infinity) {
      this.setDuration(this.video.duration);

      // Update trimEnd if it was not set correctly or is 0
      if (this.renderOptions?.segment) {
        if (this.renderOptions.segment.trimEnd === 0 || this.renderOptions.segment.trimEnd > this.video.duration) {
          console.log('[VideoController] Updating segment trimEnd to duration:', this.video.duration);
          this.renderOptions.segment.trimEnd = this.video.duration;
        }
      }
    }
  };

  private handleError = (e: Event) => {
    const video = e.target as HTMLVideoElement;
    const mediaError = video.error;
    console.error('Video error:', mediaError?.message || 'Unknown error', `(code: ${mediaError?.code})`);
    this.options.onError?.(mediaError?.message || 'Unknown video error');
  };

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
  }

  private setCurrentTime(time: number) {
    this.state.currentTime = time;
    this.options.onTimeUpdate?.(time);
  }

  private setDuration(duration: number) {
    this.state.duration = duration;
    this.options.onDurationChange?.(duration);
  }

  private renderFrame() {
    if (!this.renderOptions) return;

    const renderContext = {
      video: this.video,
      canvas: this.canvas,
      tempCanvas: this.tempCanvas,
      segment: this.renderOptions.segment,
      backgroundConfig: this.renderOptions.backgroundConfig,
      mousePositions: this.renderOptions.mousePositions,
      currentTime: this.getAdjustedTime(this.video.currentTime)
    };


    // Only draw if video is ready
    if (this.video.readyState >= 2) {
      // Draw even if paused to support live preview when editing
      // but we can skip if the video is at the end and paused
      if (renderContext.video.paused && renderContext.video.currentTime >= renderContext.video.duration) {
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
  public async generateThumbnail(options: RenderOptions): Promise<string | undefined> {
    if (this.video.readyState < 2) return undefined;

    this.isGeneratingThumbnail = true;
    const savedTime = this.video.currentTime;

    try {
      // Seek to first visible frame
      this.video.currentTime = options.segment.trimStart;
      await new Promise<void>(r => this.video.addEventListener('seeked', () => r(), { once: true }));

      // Render to offscreen canvas (doesn't disturb the main display)
      const thumbCanvas = document.createElement('canvas');
      thumbCanvas.width = this.canvas.width;
      thumbCanvas.height = this.canvas.height;
      const thumbTemp = document.createElement('canvas');

      videoRenderer.drawFrame({
        video: this.video, canvas: thumbCanvas, tempCanvas: thumbTemp,
        segment: options.segment, backgroundConfig: options.backgroundConfig,
        mousePositions: options.mousePositions, currentTime: options.segment.trimStart
      });

      return thumbCanvas.toDataURL('image/jpeg', 0.7);
    } catch {
      return undefined;
    } finally {
      // Restore position and re-render main canvas
      this.video.currentTime = savedTime;
      await new Promise<void>(r => this.video.addEventListener('seeked', () => r(), { once: true })).catch(() => {});
      this.isGeneratingThumbnail = false;
      this.renderFrame();
    }
  }

  /** Draw one frame immediately with the given options (bypasses React state). */
  public renderImmediate(options: RenderOptions) {
    if (this.video.readyState < 2) return;
    this.renderOptions = options;
    const ctx = {
      video: this.video, canvas: this.canvas, tempCanvas: this.tempCanvas,
      segment: options.segment, backgroundConfig: options.backgroundConfig,
      mousePositions: options.mousePositions,
      currentTime: options.segment.trimStart
    };
    videoRenderer.updateRenderContext(ctx);
    videoRenderer.drawFrame(ctx);
  }

  // Public API
  public updateRenderOptions(options: RenderOptions) {
    this.renderOptions = options;
    this.renderFrame();
  }

  public play() {
    // Reset seeking state to ensure play works after seek
    this.setSeeking(false);

    if (!this.state.isReady) {
      console.warn('[VideoController] Play ignored: not ready');
      return;
    }

    if (this.renderOptions?.segment) {
      const duration = this.video.duration || this.state.duration || this.video.currentTime;
      const segs = getTrimSegments(this.renderOptions.segment, duration);
      const nextTime = getNextPlayableTime(this.video.currentTime, this.renderOptions.segment, duration);
      if (nextTime !== null) this.video.currentTime = nextTime;
      else if (segs.length > 0) this.video.currentTime = segs[0].startTime;
      if (this.audio) this.audio.currentTime = this.video.currentTime;
      this.setCurrentTime(this.video.currentTime);
    }

    this.video.play().catch(e => console.warn('[VideoController] Play attempt failed:', e));
  }

  public pause() {
    this.video.pause();
  }

  public seek(time: number) {
    if (!this.state.isReady) return;

    if (this.renderOptions?.segment) {
      const duration = this.video.duration || this.state.duration || time;
      time = getNextPlayableTime(time, this.renderOptions.segment, duration)
        ?? clampToTrimSegments(time, this.renderOptions.segment, duration);
    }

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
    if (this.audio) this.audio.currentTime = time;
  }

  /** Flush any pending seek immediately (call on drag end). */
  public flushPendingSeek() {
    if (this.pendingSeekTime !== null && !this.state.isSeeking) {
      const t = this.pendingSeekTime;
      this.pendingSeekTime = null;
      const clamped = this.renderOptions?.segment
        ? (getNextPlayableTime(t, this.renderOptions.segment, this.video.duration || this.state.duration || t)
            ?? clampToTrimSegments(t, this.renderOptions.segment, this.video.duration || this.state.duration || t))
        : t;
      this.setSeeking(true);
      this.video.currentTime = clamped;
      if (this.audio) this.audio.currentTime = clamped;
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

  public setVolume(volume: number) {
    if (this.audio) {
      this.audio.volume = volume;
    }
    this.video.volume = volume;
  }

  public destroy() {
    this.stopPlaybackMonitor();
    videoRenderer.stopAnimation();
    this.video.removeEventListener('loadeddata', this.handleLoadedData);
    this.video.removeEventListener('play', this.handlePlay);
    this.video.removeEventListener('pause', this.handlePause);
    this.video.removeEventListener('timeupdate', this.handleTimeUpdate);
    this.video.removeEventListener('seeked', this.handleSeeked);
    this.video.removeEventListener('loadedmetadata', this.handleLoadedMetadata);
    this.video.removeEventListener('durationchange', this.handleDurationChange);
    this.video.removeEventListener('error', this.handleError);
  }

  // Getters
  public get isPlaying() { return this.state.isPlaying; }
  public get isReady() { return this.state.isReady; }
  public get isSeeking() { return this.state.isSeeking; }
  public get currentTime() { return this.state.currentTime; }
  public get duration() { return this.state.duration; }

  // Add this new method
  public async loadVideo({ videoBlob, videoUrl, onLoadingProgress }: { videoBlob?: Blob, videoUrl?: string, onLoadingProgress?: (p: number) => void }): Promise<string> {
    try {
      // Clear previous audio
      if (this.audio) {
        this.audio.pause();
        this.audio.src = "";
        this.audio.load();
        this.audio.removeAttribute('src');
      }

      let blob: Blob;

      if (videoBlob) {
        blob = videoBlob;
      } else if (videoUrl) {
        console.log('[VideoController] Fetching video data from:', videoUrl);
        const response = await fetch(videoUrl);
        if (!response.ok) throw new Error('Failed to fetch video');

        const reader = response.body!.getReader();
        const contentLength = +(response.headers.get('Content-Length') ?? 0);
        let receivedLength = 0;
        const chunks = [];

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          chunks.push(value);
          receivedLength += value.length;
          const progress = Math.min(((receivedLength / contentLength) * 100), 100);
          onLoadingProgress?.(progress);
        }

        blob = new Blob(chunks, { type: 'video/mp4' });
      } else {
        throw new Error('No video data provided');
      }

      const objectUrl = URL.createObjectURL(blob);
      await this.handleVideoSourceChange(objectUrl);
      return objectUrl;
    } catch (error) {
      console.error('[VideoController] Failed to load video:', error);
      throw error;
    }
  }

  public async loadAudio({ audioBlob, audioUrl, onLoadingProgress }: { audioBlob?: Blob, audioUrl?: string, onLoadingProgress?: (p: number) => void }): Promise<string> {
    try {
      if (!this.audio) return "";

      let blob: Blob;

      if (audioBlob) {
        blob = audioBlob;
      } else if (audioUrl) {
        console.log('[VideoController] Fetching audio data from:', audioUrl);
        const response = await fetch(audioUrl);
        if (!response.ok) throw new Error('Failed to fetch audio');

        const reader = response.body!.getReader();
        const contentLength = +(response.headers.get('Content-Length') ?? 0);
        let receivedLength = 0;
        const chunks = [];

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          chunks.push(value);
          receivedLength += value.length;
          const progress = Math.min(((receivedLength / contentLength) * 100), 100);
          onLoadingProgress?.(progress);
        }

        blob = new Blob(chunks, { type: 'audio/wav' });
      } else {
        return "";
      }

      const objectUrl = URL.createObjectURL(blob);
      this.audio.src = objectUrl;
      this.audio.load();

      return objectUrl;
    } catch (error) {
      console.error('[VideoController] Failed to load audio:', error);
      return "";
    }
  }

  // Update existing method to be private
  private async handleVideoSourceChange(videoUrl: string): Promise<void> {
    if (!this.video || !this.canvas) return;

    this.isChangingSource = true;

    // Reset states — single place for cleanup
    this.setReady(false);
    this.setSeeking(false);
    this.setPlaying(false);

    // Reset video element
    this.video.pause();
    this.video.src = "";
    this.video.removeAttribute('src');

    return new Promise<void>((resolve) => {
      const handleCanPlayThrough = () => {
        console.log('[VideoController] Video can play through');
        this.video.removeEventListener('canplaythrough', handleCanPlayThrough);

        // Set up canvas
        this.canvas.width = this.video.videoWidth;
        this.canvas.height = this.video.videoHeight;

        const ctx = this.canvas.getContext('2d');
        if (ctx) {
          ctx.imageSmoothingEnabled = true;
          ctx.imageSmoothingQuality = 'high';
        }

        this.isChangingSource = false;
        this.setReady(true);
        resolve();
      };

      // Set up video
      this.video.addEventListener('canplaythrough', handleCanPlayThrough);
      this.video.preload = 'auto';
      this.video.src = videoUrl;
      this.video.load();
    });
  }

  // Add this new method to handle time adjustment
  private getAdjustedTime(time: number): number {
    if (!this.renderOptions?.segment) return time;
    return clampToTrimSegments(time, this.renderOptions.segment, this.video.duration || this.state.duration || time);
  }

  // Add new method
  public initializeSegment(): VideoSegment {
    // If duration is available, use it, otherwise use a default safe large number
    // It will be corrected by handleDurationChange later
    const duration = (this.video && this.video.duration !== Infinity && !isNaN(this.video.duration))
      ? this.video.duration
      : 3600;

    const initialSegment: VideoSegment = {
      trimStart: 0,
      trimEnd: duration,
      trimSegments: [{
        id: crypto.randomUUID(),
        startTime: 0,
        endTime: duration,
      }],
      zoomKeyframes: [],
      textSegments: []
    };
    return initialSegment;
  }

  // Add this new method
  public isAtEnd(): boolean {
    if (!this.renderOptions?.segment) return false;
    const segs = getTrimSegments(this.renderOptions.segment, this.video.duration || this.state.duration || this.video.currentTime);
    const trimEnd = segs[segs.length - 1].endTime;
    return Math.abs(this.video.currentTime - trimEnd) < 0.1; // Allow 0.1s tolerance
  }
}

export const createVideoController = (options: VideoControllerOptions) => {
  return new VideoController(options);
};
