export const PREVIEW_MEDIA_MOUNTED = "sr-preview-media-mounted";

export interface PreviewMediaElements {
  video: HTMLVideoElement;
  canvas: HTMLCanvasElement;
}

export function notifyPreviewMediaMounted(video: HTMLVideoElement | null, canvas: HTMLCanvasElement | null): void {
  if (video && canvas) window.dispatchEvent(new CustomEvent<PreviewMediaElements>(
    PREVIEW_MEDIA_MOUNTED, { detail: { video, canvas } },
  ));
}
