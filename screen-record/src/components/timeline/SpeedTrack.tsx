import React, { useEffect, useRef, useState } from 'react';
import { VideoSegment, SpeedPoint } from '@/types/video';

// Logarithmic vertical mapping for intuitive dragging:
// 1x in the middle, 16x at top, 0.1x at bottom.
function speedToY(speed: number) {
  if (speed >= 1) {
    return 0.5 - 0.5 * (Math.log2(speed) / 4);
  }
  return 0.5 + 0.5 * Math.abs(Math.log10(speed));
}

function yToSpeed(y: number) {
  if (y <= 0.5) {
    return Math.pow(2, 4 * ((0.5 - y) / 0.5));
  }
  return Math.pow(10, -((y - 0.5) / 0.5));
}

interface SpeedTrackProps {
  segment: VideoSegment;
  duration: number;
  onUpdateSpeedPoints: (points: SpeedPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const SpeedTrack: React.FC<SpeedTrackProps> = ({
  segment,
  duration,
  onUpdateSpeedPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.speedPoints?.length
    ? segment.speedPoints
    : [{ time: 0, speed: 1 }, { time: duration, speed: 1 }];
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [dragBadge, setDragBadge] = useState<{ x: number; y: number; speed: number } | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && hoveredIdx !== null) {
        // Prevent deleting the anchor points
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) return;
        const next = [...points];
        next.splice(hoveredIdx, 1);
        onUpdateSpeedPoints(next);
        setHoveredIdx(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [hoveredIdx, points, onUpdateSpeedPoints]);

  const generatePath = () => {
    if (points.length === 0) return 'M 0 20 L 100 20';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => 4 + speedToY(speed) * 32;
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].speed);
    let d = `M 0 ${y0} `;
    if (x0 > 0) d += `L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.speed);
      const x2 = toX(p2.time);
      const y2 = toY(p2.speed);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }

    const xLast = toX(sorted[sorted.length - 1].time);
    const yLast = toY(sorted[sorted.length - 1].speed);
    if (xLast < 100) d += `L 100 ${yLast} `;
    return d;
  };

  const generateFillPath = () => {
    if (points.length === 0) return '';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => 4 + speedToY(speed) * 32;
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].speed);
    let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.speed);
      const x2 = toX(p2.time);
      const y2 = toY(p2.speed);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }

    const xLast = toX(sorted[sorted.length - 1].time);
    d += `L ${xLast} 40 L 100 40 Z`;
    return d;
  };

  const startDraggingPoint = (activeIdx: number, startClientY: number, rect: DOMRect) => {
    draggingIdxRef.current = activeIdx;
    const startSpeedY = speedToY(pointsRef.current[activeIdx].speed);

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;

      const mx = me.clientX - rect.left;
      const dy = me.clientY - startClientY;

      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));

      // Lower vertical sensitivity for fine-grained speed tuning.
      let newY = startSpeedY + (dy * 0.15) / rect.height;
      newY = Math.max(0, Math.min(1, newY));

      let v = yToSpeed(newY);
      v = Math.max(0.1, Math.min(16, v));

      const next = [...pointsRef.current];
      if (next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) t = duration;
        next[draggingIdxRef.current] = { time: t, speed: v };
        onUpdateSpeedPoints(next);
        setDragBadge({
          x: me.clientX,
          y: me.clientY - 40,
          speed: v,
        });
      }
    };

    const mu = () => {
      document.body.classList.remove('dragging-speed-point');
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      draggingIdxRef.current = null;
      setDragBadge(null);
      const sorted = [...pointsRef.current].sort((a, b) => a.time - b.time);
      onUpdateSpeedPoints(sorted);
      commitBatch();
    };

    document.body.classList.add('dragging-speed-point');
    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const time = (clickX / rect.width) * duration;
    let nextPoints = [...points];
    beginBatch();

    const sorted = [...nextPoints].sort((a, b) => a.time - b.time);
    let expectedV = 1.0;
    if (sorted.length > 0) {
      const idx = sorted.findIndex((p) => p.time >= time);
      if (idx === -1) expectedV = sorted[sorted.length - 1].speed;
      else if (idx === 0) expectedV = sorted[0].speed;
      else {
        const p1 = sorted[idx - 1];
        const p2 = sorted[idx];
        const ratio = (time - p1.time) / Math.max(0.0001, p2.time - p1.time);
        const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
        expectedV = p1.speed + (p2.speed - p1.speed) * cosT;
      }
    }

    e.stopPropagation();

    const point = { time, speed: expectedV };
    nextPoints.push(point);
    nextPoints.sort((a, b) => a.time - b.time);
    const activeIdx = nextPoints.indexOf(point);
    onUpdateSpeedPoints(nextPoints);

    startDraggingPoint(activeIdx, e.clientY, rect);
  };

  const handlePointPointerDown = (e: React.PointerEvent, i: number) => {
    e.stopPropagation();
    beginBatch();
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();
    startDraggingPoint(i, e.clientY, rect);
  };

  return (
    <>
      <div className="speed-track timeline-lane timeline-lane-strong relative h-10">
        <div
          className="speed-track-curve-clip absolute inset-0 overflow-hidden"
          style={{ borderRadius: "inherit" }}
        >
          <svg className="speed-track-curve h-full w-full overflow-hidden" preserveAspectRatio="none" viewBox="0 0 100 40">
            <line
              x1="0"
              y1="20"
              x2="100"
              y2="20"
              stroke="color-mix(in srgb, var(--timeline-speed-color) 24%, transparent)"
              strokeDasharray="2 2"
              vectorEffect="non-scaling-stroke"
            />
            <path d={generateFillPath()} fill="color-mix(in srgb, var(--timeline-speed-color) 12%, transparent)" />
            <path d={generatePath()} fill="none" stroke="var(--timeline-speed-color)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
          </svg>
        </div>
        <div
          className="speed-influence-layer absolute inset-0 z-10 pointer-events-auto"
          onPointerDown={handlePointerDown}
        >
          {points.map((p, i) => (
            <div
              key={i}
              className={`speed-influence-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                hoveredIdx === i ? 'ring-2 ring-[var(--timeline-speed-color)]/40' : 'hover:scale-110'
              }`}
              data-tone="speed"
              data-state={hoveredIdx === i ? "active" : "idle"}
              style={{
                left: `${(p.time / duration) * 100}%`,
                top: `${4 + speedToY(p.speed) * 32}px`,
              }}
              onMouseEnter={() => setHoveredIdx(i)}
              onMouseLeave={() => setHoveredIdx(null)}
              onPointerDown={(e) => handlePointPointerDown(e, i)}
            />
          ))}
        </div>
      </div>

      {dragBadge && (
        <div
          className="speed-track-drag-badge timeline-chip fixed z-[100] px-3 py-1.5 text-white font-bold text-sm pointer-events-none -translate-x-1/2 -translate-y-full"
          data-tone="speed"
          data-active="true"
          style={{ left: dragBadge.x, top: dragBadge.y }}
        >
          {dragBadge.speed.toFixed(2)}x
        </div>
      )}
    </>
  );
};
