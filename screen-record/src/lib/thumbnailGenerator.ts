import { invoke } from "@/lib/ipc";
import type { VideoSegment } from "@/types/video";
import { getTotalTrimDuration, toSourceTime } from "@/lib/trimSegments";

const THUMBNAIL_LOAD_TIMEOUT_MS = 8000;
const THUMBNAIL_SEEK_TIMEOUT_MS = 4000;
const TRANSPARENT_PIXEL =
  "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=";

function normalizeNativeThumbnailFrames(
  frames: string[],
): string[] | null {
  const validFrames = frames.filter((frame) => frame && frame.length > 0);
  if (validFrames.length === 0) {
    return null;
  }

  let fallbackFrame = validFrames[0];
  return frames.map((frame) => {
    if (frame && frame.length > 0) {
      fallbackFrame = frame;
      return frame;
    }
    return fallbackFrame || TRANSPARENT_PIXEL;
  });
}

function createThumbnailVideo(): HTMLVideoElement {
  const video = document.createElement("video");
  video.crossOrigin = "anonymous";
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  return video;
}

function createThumbnailCanvas(
  width: number = 160,
  height: number = 90,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

function cleanupVideo(video: HTMLVideoElement) {
  video.pause();
  video.removeAttribute("src");
  video.load();
}

function waitForLoadedFrame(video: HTMLVideoElement): Promise<void> {
  if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) {
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    let settled = false;
    const timeout = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error("Thumbnail video load timed out"));
    }, THUMBNAIL_LOAD_TIMEOUT_MS);

    const cleanup = () => {
      window.clearTimeout(timeout);
      video.removeEventListener("loadeddata", handleLoadedData);
      video.removeEventListener("error", handleError);
    };

    const handleLoadedData = () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve();
    };

    const handleError = () => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error("Thumbnail video failed to load"));
    };

    video.addEventListener("loadeddata", handleLoadedData);
    video.addEventListener("error", handleError);
    video.load();
  });
}

function seekVideo(video: HTMLVideoElement, time: number): Promise<void> {
  const safeDuration = Number.isFinite(video.duration) ? video.duration : time;
  const targetTime = Math.max(0, Math.min(safeDuration, time));

  if (
    Math.abs(video.currentTime - targetTime) <= 0.01 &&
    video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA
  ) {
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    let settled = false;
    const timeout = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error(`Thumbnail seek timed out at ${targetTime.toFixed(3)}s`));
    }, THUMBNAIL_SEEK_TIMEOUT_MS);

    const cleanup = () => {
      window.clearTimeout(timeout);
      video.removeEventListener("seeked", handleSeeked);
      video.removeEventListener("error", handleError);
    };

    const handleSeeked = () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve();
    };

    const handleError = () => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(new Error(`Thumbnail seek failed at ${targetTime.toFixed(3)}s`));
    };

    video.addEventListener("seeked", handleSeeked);
    video.addEventListener("error", handleError);
    video.currentTime = targetTime;
  });
}

async function generateHtmlVideoThumbnails(
  videoUrl: string,
  numThumbnails: number,
  options?: {
    width?: number;
    height?: number;
    quality?: number;
    trimStart?: number;
    trimEnd?: number;
  },
): Promise<string[]> {
  const video = createThumbnailVideo();
  const canvas = createThumbnailCanvas(options?.width || 160, options?.height || 90);

  try {
    video.src = videoUrl;
    await waitForLoadedFrame(video);

    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Could not get canvas context");

    const start = options?.trimStart || 0;
    const end = options?.trimEnd || video.duration;
    const duration = Math.max(end - start, 0.001);
    const safeCount = Math.max(1, numThumbnails);
    const interval = safeCount > 1 ? duration / (safeCount - 1) : 0;
    const thumbnails: string[] = [];

    for (let index = 0; index < safeCount; index++) {
      const time = start + index * interval;
      await seekVideo(video, time);
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      thumbnails.push(canvas.toDataURL("image/jpeg", options?.quality || 0.5));
    }

    return thumbnails;
  } finally {
    cleanupVideo(video);
  }
}

async function generateHtmlSegmentThumbnails(
  source: string | Blob,
  segment: VideoSegment,
  sourceDuration: number,
  numThumbnails: number,
  options?: {
    width?: number;
    height?: number;
    quality?: number;
  },
): Promise<string[]> {
  const objectUrl =
    typeof source === "string" ? source : URL.createObjectURL(source);
  const shouldRevoke = typeof source !== "string";
  const video = createThumbnailVideo();
  const canvas = createThumbnailCanvas(options?.width || 160, options?.height || 90);

  try {
    video.src = objectUrl;
    await waitForLoadedFrame(video);

    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Could not get canvas context");

    const activeDuration = Math.max(
      getTotalTrimDuration(segment, sourceDuration || video.duration),
      0.001,
    );
    const thumbnails: string[] = [];
    const safeCount = Math.max(1, numThumbnails);

    for (let index = 0; index < safeCount; index++) {
      const compactTime =
        safeCount === 1
          ? activeDuration * 0.5
          : (index / (safeCount - 1)) * activeDuration;
      const sourceTime = toSourceTime(
        compactTime,
        segment,
        sourceDuration || video.duration,
      );
      await seekVideo(video, sourceTime);
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      thumbnails.push(canvas.toDataURL("image/jpeg", options?.quality || 0.5));
    }

    return thumbnails;
  } finally {
    cleanupVideo(video);
    if (shouldRevoke) URL.revokeObjectURL(objectUrl);
  }
}

export class ThumbnailGenerator {
  async generateThumbnails(
    videoUrl: string,
    numThumbnails: number = 20,
    options?: {
      width?: number;
      height?: number;
      quality?: number;
      trimStart?: number;
      trimEnd?: number;
      filePath?: string;
    },
  ): Promise<string[]> {
    if (options?.filePath && !options.filePath.startsWith("blob:")) {
      try {
        const b64s = await invoke<string[]>("generate_thumbnails", {
          path: options.filePath,
          count: numThumbnails,
          start: options.trimStart || 0,
          end: options.trimEnd || 0,
        });
        if (b64s && b64s.length > 0) {
          const normalizedFrames = normalizeNativeThumbnailFrames(b64s);
          if (normalizedFrames) {
            return normalizedFrames.map(
              (thumbnail) => thumbnail || TRANSPARENT_PIXEL,
            );
          }
        }
      } catch (error) {
        console.warn(
          "[Thumbnail] Native generation failed, falling back to HTML5",
          error,
        );
      }
    }

    return generateHtmlVideoThumbnails(videoUrl, numThumbnails, options);
  }

  async generateSegmentThumbnails(
    source: string | Blob,
    segment: VideoSegment,
    sourceDuration: number,
    numThumbnails: number = 8,
    options?: {
      width?: number;
      height?: number;
      quality?: number;
    },
  ): Promise<string[]> {
    return generateHtmlSegmentThumbnails(
      source,
      segment,
      sourceDuration,
      numThumbnails,
      options,
    );
  }

  destroy() {}
}

export const thumbnailGenerator = new ThumbnailGenerator();
