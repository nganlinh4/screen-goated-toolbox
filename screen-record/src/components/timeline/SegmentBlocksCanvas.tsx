import React, { useEffect, useRef, useState } from "react";

export interface TimelineVisibleRange {
  startTime: number;
  endTime: number;
}

export interface CanvasTimelineSegment {
  id: string;
  startTime: number;
  endTime: number;
  color?: string | null;
  selected?: boolean;
}

interface SegmentBlocksCanvasProps {
  segments: CanvasTimelineSegment[];
  duration: number;
  visibleRange?: TimelineVisibleRange | null;
  colorVar: string;
  fallbackColor: string;
  alpha?: number;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

export function overlapsVisibleRange(
  startTime: number,
  endTime: number,
  visibleRange: TimelineVisibleRange | null | undefined,
) {
  if (!visibleRange) return true;
  return endTime >= visibleRange.startTime && startTime <= visibleRange.endTime;
}

export const SegmentBlocksCanvas: React.FC<SegmentBlocksCanvasProps> = ({
  segments,
  duration,
  visibleRange,
  colorVar,
  fallbackColor,
  alpha = 0.42,
}) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      setSize({
        width: Math.max(0, Math.round(entry.contentRect.width)),
        height: Math.max(0, Math.round(entry.contentRect.height)),
      });
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const cssWidth = size.width;
    const cssHeight = size.height;
    const dpr = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(cssWidth * dpr));
    const height = Math.max(1, Math.round(cssHeight * dpr));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (duration <= 0 || cssWidth <= 0 || cssHeight <= 0 || segments.length === 0) return;

    ctx.scale(dpr, dpr);
    const computed = getComputedStyle(canvas);
    const defaultColor = computed.getPropertyValue(colorVar).trim() || fallbackColor;
    const y = 3;
    const h = Math.max(2, cssHeight - 6);

    for (const segment of segments) {
      if (!overlapsVisibleRange(segment.startTime, segment.endTime, visibleRange)) continue;
      const start = clamp(segment.startTime, 0, duration);
      const end = clamp(segment.endTime, 0, duration);
      if (end <= start) continue;
      const x = (start / duration) * cssWidth;
      const w = Math.max(1, ((end - start) / duration) * cssWidth);
      ctx.globalAlpha = segment.selected ? Math.min(0.78, alpha + 0.24) : alpha;
      ctx.fillStyle = segment.color || defaultColor;
      ctx.fillRect(x, y, w, h);
    }
    ctx.globalAlpha = 1;
  }, [alpha, colorVar, duration, fallbackColor, segments, size.height, size.width, visibleRange]);

  return (
    <canvas
      ref={canvasRef}
      className="segment-blocks-canvas pointer-events-none absolute inset-0 h-full w-full"
      aria-hidden="true"
    />
  );
};
