/**
 * Media loading helpers for VideoController.
 *
 * Handles fetching video/audio blobs, setting element sources,
 * and the full video source-change lifecycle.
 */

import { isNativeMediaUrl } from "@/lib/mediaServer";
import { clearMediaElementSource, hasValidMediaElement } from "./videoControllerMediaSync";

// ---------------------------------------------------------------------------
// Generic media element loader
// ---------------------------------------------------------------------------

/**
 * Load an audio or video blob/URL into an HTMLMediaElement.
 * Returns the object URL (or direct URL) assigned to the element,
 * or "" if nothing was loaded.
 */
export async function loadMediaElement(
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
      clearMediaElementSource(element);
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

// ---------------------------------------------------------------------------
// Video blob/URL fetching (before source change)
// ---------------------------------------------------------------------------

export interface LoadVideoArgs {
  videoBlob?: Blob;
  videoUrl?: string;
  initialTime?: number;
  onLoadingProgress?: (p: number) => void;
  debugLabel?: string;
}

export class VideoSourceLoader {
  private active: AbortController | null = null;
  public version = 0;

  constructor(
    private video: HTMLVideoElement,
    private canvas: HTMLCanvasElement,
    private webcam: HTMLMediaElement | undefined,
    private deviceAudio: HTMLMediaElement | undefined,
    private micAudio: HTMLMediaElement | undefined,
    private callbacks: {
      start: (label?: string) => void;
      success: () => void;
      failure: () => void;
      finish: () => void;
    },
  ) {}

  destroy(): void {
    this.version += 1;
    this.active?.abort();
    this.active = null;
  }

  async load(args: LoadVideoArgs): Promise<string> {
    this.version += 1;
    this.active?.abort();
    const request = new AbortController();
    this.active = request;
    this.callbacks.start(args.debugLabel);
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const resetTimeout = () => {
      clearTimeout(timeout);
      timeout = setTimeout(() => request.abort(new Error("Video load timed out")), 30000);
    };
    resetTimeout();
    try {
      const result = await fetchVideoSource({ ...args, onLoadingProgress: (progress) => {
        resetTimeout();
        args.onLoadingProgress?.(progress);
      } }, this.webcam, this.deviceAudio, this.micAudio, request.signal,
      (url, initialTime) => performVideoSourceChange(this.video, this.canvas, url, initialTime, request.signal));
      if (request.signal.aborted && (args.videoBlob ||
        (args.videoUrl && !args.videoUrl.startsWith("blob:") && !isNativeMediaUrl(args.videoUrl)))) {
        URL.revokeObjectURL(result);
      }
      request.signal.throwIfAborted();
      if (this.active === request) this.callbacks.success();
      return result;
    } catch (error) {
      if (this.active === request) this.callbacks.failure();
      throw error;
    } finally {
      clearTimeout(timeout);
      if (this.active === request) {
        this.active = null;
        this.callbacks.finish();
      }
    }
  }
}

/**
 * Fetch the video blob/URL and return the usable source URL.
 * `onSourceChange` is called with the resolved URL and optional label
 * to trigger the full source-change lifecycle on the controller.
 */
export async function fetchVideoSource(
  args: LoadVideoArgs,
  webcamVideo: HTMLMediaElement | undefined,
  deviceAudio: HTMLMediaElement | undefined,
  micAudio: HTMLMediaElement | undefined,
  signal: AbortSignal,
  onSourceChange: (
    url: string,
    initialTime?: number,
    debugLabel?: string,
  ) => Promise<void>,
): Promise<string> {
  // Clear previous audio/webcam
  if (webcamVideo && hasValidMediaElement(webcamVideo)) {
    clearMediaElementSource(webcamVideo);
  }
  if (deviceAudio && hasValidMediaElement(deviceAudio)) {
    clearMediaElementSource(deviceAudio);
  }
  if (micAudio && hasValidMediaElement(micAudio)) {
    clearMediaElementSource(micAudio);
  }

  const { videoBlob, videoUrl, initialTime, onLoadingProgress, debugLabel } =
    args;
  let blob: Blob;

  if (videoBlob) {
    blob = videoBlob;
  } else if (videoUrl?.startsWith("blob:") || isNativeMediaUrl(videoUrl)) {
    const directVideoUrl = videoUrl!;
    await onSourceChange(directVideoUrl, initialTime, debugLabel);
    signal.throwIfAborted();
    onLoadingProgress?.(100);
    return directVideoUrl;
  } else if (videoUrl) {
    const response = await awaitWithAbort(fetch(videoUrl, { signal }), signal);
    signal.throwIfAborted();
    if (!response.ok) throw new Error("Failed to fetch video");

    const reader = response.body!.getReader();
    const contentLength = +(response.headers.get("Content-Length") ?? 0);
    let receivedLength = 0;
    const chunks = [];

    while (true) {
      const { done, value } = await awaitWithAbort(reader.read(), signal);
      signal.throwIfAborted();
      if (done) break;
      chunks.push(value);
      receivedLength += value.length;
      const progress = Math.min(
        (receivedLength / contentLength) * 100,
        100,
      );
      onLoadingProgress?.(contentLength > 0 ? progress : 0);
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

  signal.throwIfAborted();
  const objectUrl = URL.createObjectURL(blob);
  try {
    await onSourceChange(objectUrl, initialTime, debugLabel);
    signal.throwIfAborted();
    return objectUrl;
  } catch (error) {
    URL.revokeObjectURL(objectUrl);
    throw error;
  }
}

function awaitWithAbort<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(signal.reason); return; }
    const onAbort = () => { signal.removeEventListener("abort", onAbort); reject(signal.reason); };
    signal.addEventListener("abort", onAbort, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener("abort", onAbort));
  });
}

// ---------------------------------------------------------------------------
// Video source change (the HTML element lifecycle)
// ---------------------------------------------------------------------------

/**
 * Perform the full video source-change: clear the old source, assign the
 * new URL, wait for both `loadedmetadata` and `canplaythrough`, then
 * configure the canvas.
 *
 * The request signal cancels the readiness wait and any initial seek.
 */
export async function performVideoSourceChange(
  video: HTMLVideoElement,
  canvas: HTMLCanvasElement,
  videoUrl: string,
  initialTime: number | undefined,
  signal: AbortSignal,
): Promise<void> {
  signal.throwIfAborted();
  clearMediaElementSource(video);

  return new Promise<void>((resolve, reject) => {
    let metadataLoaded = false;
    let canPlay = false;
    let settled = false;
    let canPlayFallback: number | null = null;
    let seekTimeout: number | null = null;
    const cleanup = () => {
      if (canPlayFallback !== null) window.clearTimeout(canPlayFallback);
      if (seekTimeout !== null) window.clearTimeout(seekTimeout);
      video.removeEventListener("loadedmetadata", onMetadata);
      video.removeEventListener("canplaythrough", onCanPlay);
      video.removeEventListener("seeked", onSeeked);
      video.removeEventListener("error", onError);
      signal.removeEventListener("abort", onAbort);
    };
    const finish = (error?: unknown) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (error !== undefined) reject(error); else resolve();
    };
    const onAbort = () => finish(signal.reason);
    const onError = () => { if (video.error) finish(new Error(video.error.message || "Failed to load video")); };
    const onSeeked = () => finish();

    const tryFinish = () => {
      if (settled) return;
      if (!metadataLoaded || !canPlay) return;
      try {
        if (canPlayFallback !== null) window.clearTimeout(canPlayFallback);
        video.removeEventListener("loadedmetadata", onMetadata);
        video.removeEventListener("canplaythrough", onCanPlay);

        // Set up canvas
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;

        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.imageSmoothingEnabled = true;
          ctx.imageSmoothingQuality = "high";
        }

        const seekTarget =
          typeof initialTime === "number" && Number.isFinite(initialTime)
            ? Math.max(0, Math.min(initialTime, video.duration || initialTime))
            : 0;
        if (seekTarget <= 0.001) {
          finish();
          return;
        }

        if (Math.abs(video.currentTime - seekTarget) <= 0.02) {
          finish();
          return;
        }

        seekTimeout = window.setTimeout(() => finish(), 800);

        video.addEventListener("seeked", onSeeked, { once: true });
        video.currentTime = seekTarget;
      } catch (error) { finish(error); }
    };

    const onMetadata = () => {
      metadataLoaded = true;
      canPlayFallback = window.setTimeout(() => {
        if (settled || !metadataLoaded) return;
        canPlay = true;
        tryFinish();
      }, 900);
      tryFinish();
    };
    const onCanPlay = () => {
      canPlay = true;
      tryFinish();
    };
    // Wait for BOTH metadata (dimensions) and canplaythrough (buffered)
    video.addEventListener("loadedmetadata", onMetadata);
    video.addEventListener("canplaythrough", onCanPlay);
    video.addEventListener("error", onError);
    signal.addEventListener("abort", onAbort, { once: true });
    try {
      video.preload = "auto";
      video.src = videoUrl;
      video.load();
    } catch (error) { finish(error); }
  });
}
