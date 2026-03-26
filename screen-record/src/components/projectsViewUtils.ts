import { Project } from "@/types/video";

export const PROJECTS_FLIP_DEBUG = false;
export const PROJECTS_FLIP_EASING = "cubic-bezier(0.8, 0, 0.2, 1)";
export const PROJECTS_FLIP_SETTLE_EASING = "cubic-bezier(0.7, 0, 0.18, 1)";

export interface ProjectsPreviewRectSnapshot {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ProjectsPreviewTargetSnapshot {
  stageRect: ProjectsPreviewRectSnapshot | null;
  canvasRect: ProjectsPreviewRectSnapshot | null;
}

export function formatDuration(s: number): string {
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

/** Compute the object-fit:contain rect for an aspect ratio inside a container. */
export function containRect(
  cw: number,
  ch: number,
  nw: number,
  nh: number,
): { left: number; top: number; width: number; height: number } {
  const ca = cw / ch,
    na = nw / nh;
  let w: number, h: number;
  if (ca > na) {
    h = ch;
    w = h * na;
  } else {
    w = cw;
    h = w / na;
  }
  return { left: (cw - w) / 2, top: (ch - h) / 2, width: w, height: h };
}

export function getProjectPreviewTargetRect(
  cw: number,
  ch: number,
  project: Omit<Project, "videoBlob"> | undefined,
  fallbackWidth: number,
  fallbackHeight: number,
): { left: number; top: number; width: number; height: number } {
  const canvasWidth = project?.backgroundConfig?.canvasWidth;
  const canvasHeight = project?.backgroundConfig?.canvasHeight;
  if (
    typeof canvasWidth === "number" &&
    canvasWidth > 0 &&
    typeof canvasHeight === "number" &&
    canvasHeight > 0
  ) {
    return containRect(cw, ch, canvasWidth, canvasHeight);
  }
  return containRect(cw, ch, fallbackWidth, fallbackHeight);
}

export function getProjectThumbnailRadius(scaleX: number, scaleY: number): string {
  const safeScaleX = Math.max(scaleX, 0.0001);
  const safeScaleY = Math.max(scaleY, 0.0001);
  return `${12 / safeScaleX}px ${12 / safeScaleX}px 0 0 / ${12 / safeScaleY}px ${12 / safeScaleY}px 0 0`;
}

export function rectSnapshotToDomRect(
  rect: ProjectsPreviewRectSnapshot | null | undefined,
): DOMRect | null {
  if (!rect || rect.width <= 0 || rect.height <= 0) return null;
  return new DOMRect(rect.left, rect.top, rect.width, rect.height);
}

export function getPreviewStageRect(
  snapshot?: ProjectsPreviewTargetSnapshot | null,
): DOMRect | null {
  const snapshotRect = rectSnapshotToDomRect(snapshot?.stageRect);
  if (snapshotRect) return snapshotRect;
  const previewStage = document.querySelector(
    ".preview-canvas",
  ) as HTMLElement | null;
  if (!previewStage) return null;
  const rect = previewStage.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

export function getLiveCanvasRect(): DOMRect | null {
  const canvas = document.querySelector(
    ".preview-canvas-element",
  ) as HTMLCanvasElement | null;
  if (!canvas) return null;
  const rect = canvas.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

export function getOptionalElementRect(selector: string): DOMRect | null {
  const element = document.querySelector(selector) as HTMLElement | null;
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

export function toRectSnapshot(rect: DOMRect | null) {
  if (!rect) return null;
  return {
    left: Number(rect.left.toFixed(2)),
    top: Number(rect.top.toFixed(2)),
    width: Number(rect.width.toFixed(2)),
    height: Number(rect.height.toFixed(2)),
  };
}

export function logProjectsFlip(
  event: string,
  details: Record<string, unknown> = {},
) {
  if (!PROJECTS_FLIP_DEBUG) return;
  console.log("[ProjectsFlip]", { event, ...details });
}

/** Resolve the rendered preview canvas rect relative to a parent, if present. */
export function getPreviewCanvasRect(
  parent: HTMLElement,
): { left: number; top: number; width: number; height: number } | null {
  const canvas = parent.querySelector(
    ".preview-canvas-element",
  ) as HTMLCanvasElement | null;
  if (!canvas) return null;
  const parentRect = parent.getBoundingClientRect();
  const canvasRect = canvas.getBoundingClientRect();
  if (canvasRect.width <= 0 || canvasRect.height <= 0) return null;
  return {
    left: canvasRect.left - parentRect.left,
    top: canvasRect.top - parentRect.top,
    width: canvasRect.width,
    height: canvasRect.height,
  };
}
void getPreviewCanvasRect;
