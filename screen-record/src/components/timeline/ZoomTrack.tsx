import React, { useState, useRef } from 'react';
import { VideoSegment, ZoomKeyframe } from '@/types/video';

const getKeyframeRange = (
  keyframes: ZoomKeyframe[],
  index: number,
  totalDuration: number
): { rangeStart: number; rangeEnd: number } => {
  const kf = keyframes[index];
  const prev = index > 0 ? keyframes[index - 1] : null;
  const next = index < keyframes.length - 1 ? keyframes[index + 1] : null;

  // Left range: use custom duration if set, otherwise auto-calculate
  let rangeStart: number;
  if (kf.duration > 0) {
    rangeStart = Math.max(prev ? prev.time : 0, kf.time - kf.duration);
  } else {
    rangeStart = prev
      ? prev.time + (kf.time - prev.time) * 0.5
      : Math.max(0, kf.time - 2.0);
  }

  // Right range: halfway to next keyframe, or up to 2s after
  const rangeEnd = next
    ? kf.time + (next.time - kf.time) * 0.5
    : Math.min(totalDuration, kf.time + 2.0);

  return { rangeStart, rangeEnd };
};

interface ZoomTrackProps {
  segment: VideoSegment;
  duration: number;
  editingKeyframeId: number | null;
  onKeyframeClick: (time: number, index: number) => void;
  onKeyframeDragStart: (index: number) => void;
  onUpdateInfluencePoints: (points: { time: number; value: number }[]) => void;
  onUpdateKeyframes: (keyframes: ZoomKeyframe[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const ZoomTrack: React.FC<ZoomTrackProps> = ({
  segment,
  duration,
  editingKeyframeId,
  onKeyframeClick,
  onKeyframeDragStart,
  onUpdateInfluencePoints,
  onUpdateKeyframes,
  beginBatch,
  commitBatch,
}) => {
  const hasInfluenceCurve = segment.smoothMotionPath && segment.smoothMotionPath.length > 0;
  const points = segment.zoomInfluencePoints || [];
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const segmentRef = useRef(segment);
  segmentRef.current = segment;
  const callbacksRef = useRef({ onUpdateKeyframes });
  callbacksRef.current = { onUpdateKeyframes };
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // Handle point deletion
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && hoveredIdx !== null) {
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) {
          if (points.length === 2) onUpdateInfluencePoints([]);
          return;
        }
        const newPoints = [...points];
        newPoints.splice(hoveredIdx, 1);
        onUpdateInfluencePoints(newPoints);
        setHoveredIdx(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [hoveredIdx, points, onUpdateInfluencePoints]);

  // Generate SVG path for influence curve
  const generatePath = () => {
    if (points.length === 0) return 'M 0 20 L 100 20';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    let d = '';
    const steps = 100;
    for (let i = 0; i <= steps; i++) {
      const t = (i / steps) * duration;
      let v = 1.0;
      const idx = sorted.findIndex(p => p.time >= t);
      if (idx === -1) v = sorted[sorted.length - 1].value;
      else if (idx === 0) v = sorted[0].value;
      else {
        const p1 = sorted[idx - 1];
        const p2 = sorted[idx];
        const ratio = (t - p1.time) / (p2.time - p1.time);
        const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
        v = p1.value * (1 - cosT) + p2.value * cosT;
      }
      const y = 4 + (1 - v) * 32;
      d += `${i === 0 ? 'M' : 'L'} ${i} ${y} `;
    }
    return d;
  };

  // Generate fill path (area under curve)
  const generateFillPath = () => {
    if (points.length === 0) return '';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    let d = 'M 0 40 ';
    const steps = 100;
    for (let i = 0; i <= steps; i++) {
      const t = (i / steps) * duration;
      let v = 1.0;
      const idx = sorted.findIndex(p => p.time >= t);
      if (idx === -1) v = sorted[sorted.length - 1].value;
      else if (idx === 0) v = sorted[0].value;
      else {
        const p1 = sorted[idx - 1];
        const p2 = sorted[idx];
        const ratio = (t - p1.time) / (p2.time - p1.time);
        const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
        v = p1.value * (1 - cosT) + p2.value * cosT;
      }
      const y = 4 + (1 - v) * 32;
      d += `L ${i} ${y} `;
    }
    d += 'L 100 40 Z';
    return d;
  };

  const handleInfluenceMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const clickY = e.clientY - rect.top;
    const time = (clickX / rect.width) * duration;
    const hitThresholdX = 14;
    let newPoints = [...points];
    beginBatch();

    let activeIdx = newPoints.findIndex(p => {
      const px = (p.time / duration) * rect.width;
      const py = 4 + (1 - p.value) * 32;
      return Math.abs(px - clickX) < hitThresholdX && Math.abs(py - clickY) < hitThresholdX;
    });

    if (activeIdx !== -1) e.stopPropagation();

    if (activeIdx === -1) {
      const sorted = [...newPoints].sort((a, b) => a.time - b.time);
      let expectedV = 1.0;
      if (sorted.length > 0) {
        const idx = sorted.findIndex(p => p.time >= time);
        if (idx === -1) expectedV = sorted[sorted.length - 1].value;
        else if (idx === 0) expectedV = sorted[0].value;
        else {
          const p1 = sorted[idx - 1];
          const p2 = sorted[idx];
          const ratio = (time - p1.time) / (p2.time - p1.time);
          const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
          expectedV = p1.value * (1 - cosT) + p2.value * cosT;
        }
      }
      const expectedY = 4 + (1 - expectedV) * 32;
      if (Math.abs(clickY - expectedY) > 10 && newPoints.length > 0) return;

      e.stopPropagation();
      if (newPoints.length === 0) {
        newPoints.push({ time: 0, value: 1 });
        newPoints.push({ time: duration, value: 1 });
      }
      const p = { time, value: expectedV };
      newPoints.push(p);
      newPoints.sort((a, b) => a.time - b.time);
      activeIdx = newPoints.indexOf(p);
      onUpdateInfluencePoints(newPoints);
    }

    draggingIdxRef.current = activeIdx;

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;
      const mx = me.clientX - rect.left;
      const my = me.clientY - rect.top;
      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));
      let v = 1 - (my - 4) / 32;
      v = Math.max(0, Math.min(1, v));
      const next = [...pointsRef.current];
      if (draggingIdxRef.current !== null && next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) t = duration;
        next[draggingIdxRef.current] = { time: t, value: v };
        onUpdateInfluencePoints(next);
      }
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      draggingIdxRef.current = null;
      const sorted = [...pointsRef.current].sort((a, b) => a.time - b.time);
      onUpdateInfluencePoints(sorted);
      commitBatch();
    };

    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  const handlePointMouseDown = (e: React.MouseEvent, i: number) => {
    e.stopPropagation();
    beginBatch();
    draggingIdxRef.current = i;
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();

    const mm = (me: MouseEvent) => {
      const mx = me.clientX - rect.left;
      const my = me.clientY - rect.top;
      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));
      if (i === 0) t = 0;
      if (i === pointsRef.current.length - 1 && pointsRef.current.length > 1) t = duration;
      let v = 1 - (my - 4) / 32;
      v = Math.max(0, Math.min(1, v));
      const next = [...pointsRef.current];
      if (draggingIdxRef.current !== null && next[draggingIdxRef.current]) {
        next[draggingIdxRef.current] = { time: t, value: v };
        onUpdateInfluencePoints(next);
      }
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      draggingIdxRef.current = null;
      const sorted = [...pointsRef.current].sort((a, b) => a.time - b.time);
      onUpdateInfluencePoints(sorted);
      commitBatch();
    };

    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  return (
    <div className="relative h-10 rounded bg-[var(--surface-container)]/60">
      {/* Influence curve layer */}
      {hasInfluenceCurve && (
        <div
          className="absolute inset-0 z-10 pointer-events-auto"
          onMouseDown={handleInfluenceMouseDown}
        >
          <svg className="w-full h-full overflow-visible" preserveAspectRatio="none" viewBox="0 0 100 40">
            <line x1="0" y1="4" x2="100" y2="4" stroke="rgba(255,255,255,0.06)" vectorEffect="non-scaling-stroke" />
            <line x1="0" y1="36" x2="100" y2="36" stroke="rgba(255,255,255,0.06)" vectorEffect="non-scaling-stroke" />
            {points.length > 0 && (
              <path d={generateFillPath()} fill="rgba(74, 222, 128, 0.08)" />
            )}
            <path d={generatePath()} fill="none" stroke="#4ade80" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
          </svg>
          {points.map((p, i) => (
            <div
              key={i}
              className={`absolute w-2.5 h-2.5 bg-white border-2 border-[#4ade80] rounded-full -translate-x-1/2 -translate-y-1/2 cursor-pointer transition-transform shadow-sm ${
                hoveredIdx === i ? 'scale-125 ring-2 ring-red-500/50' : 'hover:scale-125'
              }`}
              style={{
                left: `${(p.time / duration) * 100}%`,
                top: `${4 + (1 - p.value) * 32}px`,
              }}
              onMouseEnter={() => setHoveredIdx(i)}
              onMouseLeave={() => setHoveredIdx(null)}
              onMouseDown={(e) => handlePointMouseDown(e, i)}
            />
          ))}
        </div>
      )}

      {/* Keyframe markers layer */}
      <div className="absolute inset-0 z-20 pointer-events-none">
        {segment.zoomKeyframes.map((keyframe, index) => {
          const active = editingKeyframeId === index;
          const { rangeStart, rangeEnd } = getKeyframeRange(segment.zoomKeyframes, index, duration);
          const peakOpacity = Math.min(0.35, 0.08 + (keyframe.zoomFactor - 1) * 0.15);
          const rangeWidth = rangeEnd - rangeStart;
          const peakPct = rangeWidth > 0 ? ((keyframe.time - rangeStart) / rangeWidth) * 100 : 50;

          return (
            <React.Fragment key={index}>
              {/* Left range handle */}
              <div
                className="absolute inset-y-0 w-3 cursor-col-resize z-30 pointer-events-auto group/handle"
                style={{ left: `calc(${(rangeStart / duration) * 100}% - 6px)` }}
                onMouseDown={(e) => {
                  e.stopPropagation();
                  beginBatch();
                  const rect = e.currentTarget.parentElement!.getBoundingClientRect();
                  const onMove = (me: MouseEvent) => {
                    const x = me.clientX - rect.left;
                    const t = Math.max(0, Math.min(keyframe.time - 0.1, (x / rect.width) * duration));
                    const newDuration = keyframe.time - t;
                    const updatedKeyframes = segmentRef.current.zoomKeyframes.map((kf, i) =>
                      i === index ? { ...kf, duration: newDuration } : kf
                    );
                    callbacksRef.current.onUpdateKeyframes(updatedKeyframes);
                  };
                  const onUp = () => {
                    window.removeEventListener('mousemove', onMove);
                    window.removeEventListener('mouseup', onUp);
                    commitBatch();
                  };
                  window.addEventListener('mousemove', onMove);
                  window.addEventListener('mouseup', onUp);
                }}
              >
                <div className="absolute inset-y-1 w-0.5 bg-[var(--primary-color)]/40 group-hover/handle:bg-[var(--primary-color)] transition-colors left-1/2 -translate-x-1/2" />
              </div>
              {/* Gradient range background (visual only — pointer-events-none to not block green curve) */}
              <div
                className={`absolute inset-y-0 pointer-events-none ${
                  active ? 'opacity-100' : 'opacity-60'
                }`}
                style={{
                  left: `${(rangeStart / duration) * 100}%`,
                  width: `${((rangeEnd - rangeStart) / duration) * 100}%`,
                  background: `linear-gradient(90deg, rgba(59, 130, 246, 0.02) 0%, rgba(59, 130, 246, ${peakOpacity}) ${peakPct}%, rgba(59, 130, 246, 0.02) 100%)`,
                }}
              />
              {/* Diamond marker + zoom pill */}
              <div
                className="absolute pointer-events-auto cursor-pointer group"
                style={{
                  left: `${(keyframe.time / duration) * 100}%`,
                  transform: 'translateX(-50%)',
                  top: '0',
                  height: '100%',
                }}
                onClick={(e) => { e.stopPropagation(); onKeyframeClick(keyframe.time, index); }}
                onMouseDown={(e) => { e.stopPropagation(); onKeyframeDragStart(index); }}
              >
                <div className="relative flex flex-col items-center h-full justify-center">
                  {/* Zoom % pill */}
                  <div
                    className={`px-1.5 py-0.5 rounded text-[9px] font-medium whitespace-nowrap mb-0.5 ${
                      active
                        ? 'bg-[var(--primary-color)] text-white'
                        : 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
                    }`}
                  >
                    {Math.round((keyframe.zoomFactor - 1) * 100)}%
                  </div>
                  {/* Diamond marker */}
                  <div
                    className={`w-2 h-2 rotate-45 bg-[var(--primary-color)] group-hover:scale-125 transition-transform ${
                      active ? 'ring-1 ring-white' : ''
                    }`}
                  />
                </div>
              </div>
            </React.Fragment>
          );
        })}
      </div>
    </div>
  );
};
