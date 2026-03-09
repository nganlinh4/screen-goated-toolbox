import { invoke } from "@/lib/ipc";
import type { VideoSegment } from "@/types/video";
import { getTotalTrimDuration, toSourceTime } from "@/lib/trimSegments";

export class ThumbnailGenerator {
  private canvas: HTMLCanvasElement;
  private video: HTMLVideoElement;

  constructor() {
    this.canvas = document.createElement("canvas");
    this.video = document.createElement("video");
    this.video.crossOrigin = "anonymous";
    this.video.muted = true;
  }

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
          const px =
            "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=";
          return b64s.map((b) => b || px);
        }
      } catch (e) {
        console.warn(
          "[Thumbnail] Native generation failed, falling back to HTML5",
          e,
        );
      }
    }

    this.video.src = videoUrl;
    await new Promise((r) =>
      this.video.addEventListener("loadeddata", r, { once: true }),
    );

    // Set canvas size
    this.canvas.width = options?.width || 160;
    this.canvas.height = options?.height || 90;

    const ctx = this.canvas.getContext("2d");
    if (!ctx) throw new Error("Could not get canvas context");

    const start = options?.trimStart || 0;
    const end = options?.trimEnd || this.video.duration;
    const duration = end - start;
    const interval = duration / numThumbnails;
    const thumbnails: string[] = [];

    for (let i = 0; i < numThumbnails; i++) {
      const time = start + i * interval;
      this.video.currentTime = time;
      await new Promise((r) =>
        this.video.addEventListener("seeked", r, { once: true }),
      );

      ctx.drawImage(this.video, 0, 0, this.canvas.width, this.canvas.height);
      thumbnails.push(
        this.canvas.toDataURL("image/jpeg", options?.quality || 0.5),
      );
    }

    // Cleanup
    this.video.src = "";
    return thumbnails;
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
    const objectUrl =
      typeof source === "string" ? source : URL.createObjectURL(source);
    const shouldRevoke = typeof source !== "string";

    try {
      this.video.src = objectUrl;
      await new Promise((resolve) =>
        this.video.addEventListener("loadeddata", resolve, { once: true }),
      );

      this.canvas.width = options?.width || 160;
      this.canvas.height = options?.height || 90;

      const ctx = this.canvas.getContext("2d");
      if (!ctx) throw new Error("Could not get canvas context");

      const activeDuration = Math.max(
        getTotalTrimDuration(segment, sourceDuration || this.video.duration),
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
          sourceDuration || this.video.duration,
        );
        this.video.currentTime = sourceTime;
        await new Promise((resolve) =>
          this.video.addEventListener("seeked", resolve, { once: true }),
        );
        ctx.drawImage(this.video, 0, 0, this.canvas.width, this.canvas.height);
        thumbnails.push(
          this.canvas.toDataURL("image/jpeg", options?.quality || 0.5),
        );
      }

      this.video.src = "";
      return thumbnails;
    } finally {
      if (shouldRevoke) URL.revokeObjectURL(objectUrl);
    }
  }

  destroy() {
    this.video.src = "";
    this.video = null!;
    this.canvas = null!;
  }
}

export const thumbnailGenerator = new ThumbnailGenerator();
