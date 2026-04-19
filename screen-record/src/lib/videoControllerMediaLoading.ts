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
  await onSourceChange(objectUrl, initialTime, debugLabel);
  return objectUrl;
}

// ---------------------------------------------------------------------------
// Video source change (the HTML element lifecycle)
// ---------------------------------------------------------------------------

/**
 * Perform the full video source-change: clear the old source, assign the
 * new URL, wait for both `loadedmetadata` and `canplaythrough`, then
 * configure the canvas.
 *
 * `beforeChange` is invoked right before the source is cleared so the
 * controller can reset transient state. `afterChange` is called once the
 * video is ready so the controller can set isReady / isChangingSource.
 */
export async function performVideoSourceChange(
  video: HTMLVideoElement,
  canvas: HTMLCanvasElement,
  videoUrl: string,
  initialTime: number | undefined,
  beforeChange: () => void,
  afterChange: () => void,
): Promise<void> {
  beforeChange();
  clearMediaElementSource(video);

  return new Promise<void>((resolve) => {
    let metadataLoaded = false;
    let canPlay = false;

    const tryFinish = () => {
      if (!metadataLoaded || !canPlay) return;
      cleanup();

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
        afterChange();
        resolve();
        return;
      }

      const finishAfterSeek = () => {
        afterChange();
        resolve();
      };

      if (Math.abs(video.currentTime - seekTarget) <= 0.02) {
        finishAfterSeek();
        return;
      }

      const onSeeked = () => {
        cleanupSeek();
        finishAfterSeek();
      };
      const cleanupSeek = () => {
        clearTimeout(seekTimeout);
        video.removeEventListener("seeked", onSeeked);
      };
      const seekTimeout = window.setTimeout(() => {
        cleanupSeek();
        finishAfterSeek();
      }, 800);

      video.addEventListener("seeked", onSeeked, { once: true });
      video.currentTime = seekTarget;
    };

    const onMetadata = () => {
      metadataLoaded = true;
      tryFinish();
    };
    const onCanPlay = () => {
      canPlay = true;
      tryFinish();
    };
    const cleanup = () => {
      video.removeEventListener("loadedmetadata", onMetadata);
      video.removeEventListener("canplaythrough", onCanPlay);
    };

    // Wait for BOTH metadata (dimensions) and canplaythrough (buffered)
    video.addEventListener("loadedmetadata", onMetadata);
    video.addEventListener("canplaythrough", onCanPlay);
    video.preload = "auto";
    video.src = videoUrl;
    video.load();
  });
}
