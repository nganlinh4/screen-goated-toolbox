import { coversCanvas, type VideoPlacementRect } from '@/lib/videoGeometry';

type FrameContext = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

export function isUnmaskedCanvasCoveringVideo(
  rect: VideoPlacementRect,
  canvasWidth: number,
  canvasHeight: number,
  borderRadius: number,
): boolean {
  return borderRadius <= 0 && coversCanvas(rect, canvasWidth, canvasHeight);
}

export function traceVideoFramePath(
  context: FrameContext,
  rect: VideoPlacementRect,
  borderRadius: number,
  inset = 0.5,
): void {
  const { left, top, width, height } = rect;
  const radius = Math.max(0, Math.min(borderRadius, width / 2, height / 2));
  context.beginPath();
  context.moveTo(left + radius + inset, top + inset);
  context.lineTo(left + width - radius - inset, top + inset);
  context.quadraticCurveTo(
    left + width - inset,
    top + inset,
    left + width - inset,
    top + radius + inset,
  );
  context.lineTo(left + width - inset, top + height - radius - inset);
  context.quadraticCurveTo(
    left + width - inset,
    top + height - inset,
    left + width - radius - inset,
    top + height - inset,
  );
  context.lineTo(left + radius + inset, top + height - inset);
  context.quadraticCurveTo(
    left + inset,
    top + height - inset,
    left + inset,
    top + height - radius - inset,
  );
  context.lineTo(left + inset, top + radius + inset);
  context.quadraticCurveTo(
    left + inset,
    top + inset,
    left + radius + inset,
    top + inset,
  );
  context.closePath();
}
