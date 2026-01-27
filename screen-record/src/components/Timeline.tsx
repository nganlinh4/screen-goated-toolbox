import React, { useState } from 'react';
import { VideoSegment, ZoomKeyframe } from '@/types/video';

// Helper function to format time
function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

// Helper function to calculate keyframe range
const getKeyframeRange = (
  keyframes: ZoomKeyframe[],
  index: number
): { rangeStart: number; rangeEnd: number } => {
  const keyframe = keyframes[index];
  const prevKeyframe = index > 0 ? keyframes[index - 1] : null;
  const rangeStart =
    prevKeyframe && keyframe.time - prevKeyframe.time <= 1.0
      ? prevKeyframe.time
      : Math.max(0, keyframe.time - 1.0);
  return { rangeStart, rangeEnd: keyframe.time };
};

interface TimelineProps {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  thumbnails: string[];
  timelineRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
}

const TimeMarkers: React.FC<{ duration: number }> = ({ duration }) => (
  <div className="absolute left-0 right-0 bottom-[-24px] flex justify-between text-xs text-[#d7dadc] z-40 pointer-events-none px-1">
    {Array.from({ length: 11 }).map((_, i) => {
      const time = (duration * i) / 10;
      return (
        <div key={i} className="flex flex-col items-center">
          <span className="mb-1">{formatTime(time)}</span>
          <div className="h-2 w-0.5 bg-[#d7dadc]/20" />
        </div>
      );
    })}
  </div>
);

const VideoTrack: React.FC<{ segment: VideoSegment; duration: number; thumbnails: string[] }> = ({
  segment,
  duration,
  thumbnails
}) => (
  <div className="absolute inset-0">
    {/* Background track */}
    <div className="absolute inset-0 bg-[#272729] rounded-lg overflow-hidden">
      {/* Thumbnails */}
      <div className="absolute inset-0 flex gap-[2px]">
        {thumbnails.map((thumbnail, index) => (
          <div
            key={index}
            className="h-full flex-shrink-0"
            style={{
              width: `calc(${100 / thumbnails.length}% - 2px)`,
              backgroundImage: `url(${thumbnail})`,
              backgroundSize: 'cover',
              backgroundPosition: 'center',
              opacity: 0.5
            }}
          />
        ))}
      </div>
    </div>

    {/* Trimmed sections */}
    <div
      className="absolute inset-y-0 left-0 bg-black/50 rounded-l-lg"
      style={{ width: `${(segment.trimStart / duration) * 100}%` }}
    />
    <div
      className="absolute inset-y-0 right-0 bg-black/50 rounded-r-lg"
      style={{ width: `${((duration - segment.trimEnd) / duration) * 100}%` }}
    />

    {/* Active section */}
    <div
      className="absolute inset-y-0 bg-white/2 border border-white/20"
      style={{
        left: `${(segment.trimStart / duration) * 100}%`,
        right: `${((duration - segment.trimEnd) / duration) * 100}%`
      }}
    />
  </div>
);

const ZoomKeyframes: React.FC<{
  segment: VideoSegment;
  duration: number;
  editingKeyframeId: number | null;
  onKeyframeClick: (time: number, index: number) => void;
  onKeyframeDragStart: (index: number) => void;
}> = ({ segment, duration, editingKeyframeId, onKeyframeClick, onKeyframeDragStart }) => (
  <div className="absolute inset-x-0 h-full">
    {segment.zoomKeyframes.map((keyframe, index) => {
      const active = editingKeyframeId === index;
      const { rangeStart, rangeEnd } = getKeyframeRange(segment.zoomKeyframes, index);

      return (
        <div key={index}>
          {/* Gradient background for zoom range */}
          <div
            className={`absolute h-full cursor-pointer transition-colors border-r border-[#0079d3] ${active ? "opacity-100" : "opacity-80"
              }`}
            style={{
              left: `${(rangeStart / duration) * 100}%`,
              width: `${((rangeEnd - rangeStart) / duration) * 100}%`,
              zIndex: 20,
              background: `linear-gradient(90deg, rgba(0, 121, 211, 0.1) 0%, rgba(0, 121, 211, ${0.1 + (keyframe.zoomFactor - 1) * 0.3
                }) 100%)`
            }}
          />
          {/* Keyframe marker with label */}
          <div
            className="absolute cursor-pointer group"
            style={{
              left: `${(keyframe.time / duration) * 100}%`,
              transform: "translateX(-50%)",
              top: "-40px",
              height: "64px"
            }}
            onClick={(e) => {
              e.stopPropagation();
              onKeyframeClick(keyframe.time, index);
            }}
            onMouseDown={(e) => {
              e.stopPropagation();
              onKeyframeDragStart(index);
            }}
          >
            <div className="relative flex flex-col items-center">
              <div
                className={`px-2 py-1 mb-1 rounded-full text-xs font-medium whitespace-nowrap ${active ? "bg-[#0079d3] text-white" : "bg-[#0079d3]/20 text-[#0079d3]"
                  }`}
              >
                {Math.round((keyframe.zoomFactor - 1) * 100)}%
              </div>
              <div
                className={`w-3 h-3 bg-[#0079d3] rounded-full hover:scale-125 transition-transform ${active ? "ring-2 ring-white" : ""
                  }`}
              />
              <div className="w-[1px] h-10 bg-[#0079d3]/30 group-hover:bg-[#0079d3]/50" />
            </div>
          </div>
        </div>
      );
    })}
  </div>
);

const ZoomInfluenceTrack: React.FC<{
  segment: VideoSegment;
  duration: number;
  onUpdatePoints: (points: { time: number; value: number }[]) => void;
}> = ({ segment, duration, onUpdatePoints }) => {
  const points = segment.zoomInfluencePoints || [];
  const draggingIdxRef = React.useRef<number | null>(null);
  const pointsRef = React.useRef(points);
  pointsRef.current = points;

  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // Handle Point Deletion
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && hoveredIdx !== null) {
        // Don't delete start/end anchors if they are the only ones left (length 2)
        // But actually we force start/end to exist. User can't delete index 0 or length-1
        // unless we want to allow re-creating them? 
        // Requirement: "start and end handle must be able to be adjusted... not generating new handle" indicates anchors are permanent.
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) {
          if (points.length === 2) {
            onUpdatePoints([]);
          }
          return;
        }

        const newPoints = [...points];
        newPoints.splice(hoveredIdx, 1);
        onUpdatePoints(newPoints);
        setHoveredIdx(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [hoveredIdx, points, onUpdatePoints]);

  // Generate SVG Path
  const generatePath = () => {
    if (points.length === 0) return `M 0 10 L 100 10`;

    // We visualize based on current points state (even if unsorted during drag for instant feedback)
    // But curve generator expects sorted inputs to draw correctly left-to-right?
    // Actually, if points are unsorted, drawing lines between them in array-order might be messy.
    // So for visualization, we MUST sort a copy.
    const sortedPoints = [...points].sort((a, b) => a.time - b.time);

    let d = "";
    // Sample 100 points
    const steps = 100;
    for (let i = 0; i <= steps; i++) {
      const t = (i / steps) * duration;

      // Inline getValueAt logic with sortedPoints
      let v = 1.0;
      const idx = sortedPoints.findIndex(p => p.time >= t);
      if (idx === -1) v = sortedPoints[sortedPoints.length - 1].value;
      else if (idx === 0) v = sortedPoints[0].value;
      else {
        const p1 = sortedPoints[idx - 1];
        const p2 = sortedPoints[idx];
        const ratio = (t - p1.time) / (p2.time - p1.time);
        const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
        v = p1.value * (1 - cosT) + p2.value * cosT;
      }

      const y = 8 + (1 - v) * 32;
      const x = i;
      d += `${i === 0 ? 'M' : 'L'} ${x} ${y} `;
    }
    return d;
  };

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const clickY = e.clientY - rect.top;

    const time = (clickX / rect.width) * duration;
    let val = 1 - (clickY - 8) / 32;
    val = Math.max(0, Math.min(1, val));

    const hitThresholdX = 14;
    let newPoints = [...points];

    let activeIdx = newPoints.findIndex(p => {
      const px = (p.time / duration) * rect.width;
      const py = 8 + (1 - p.value) * 32;
      return Math.abs(px - clickX) < hitThresholdX && Math.abs(py - clickY) < hitThresholdX;
    });

    if (activeIdx !== -1) {
      e.stopPropagation();
    }

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

      const expectedY = 8 + (1 - expectedV) * 32;

      if (Math.abs(clickY - expectedY) > 10 && newPoints.length > 0) {
        return;
      }

      e.stopPropagation();

      if (newPoints.length === 0) {
        newPoints.push({ time: 0, value: 1 });
        newPoints.push({ time: duration, value: 1 });
      }

      const p = { time, value: expectedV };
      newPoints.push(p);
      newPoints.sort((a, b) => a.time - b.time);
      activeIdx = newPoints.indexOf(p);
      onUpdatePoints(newPoints);
    }

    draggingIdxRef.current = activeIdx;

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;

      const mx = me.clientX - rect.left;
      const my = me.clientY - rect.top;

      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));

      let v = 1 - (my - 8) / 32;
      v = Math.max(0, Math.min(1, v));

      const next = [...pointsRef.current];
      if (draggingIdxRef.current !== null && next[draggingIdxRef.current]) {
        // Lock start/end time
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) t = duration;

        next[draggingIdxRef.current] = { time: t, value: v };
        onUpdatePoints(next);
      }
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      draggingIdxRef.current = null;
      // Final sort to keep consistency
      const sorted = [...pointsRef.current].sort((a, b) => a.time - b.time);
      onUpdatePoints(sorted);
    };

    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  return (
    <div
      className="absolute inset-x-0 top-0 h-12 z-20 pointer-events-auto"
      onMouseDown={handleMouseDown}
    >
      <svg className="w-full h-full overflow-visible" preserveAspectRatio="none" viewBox="0 0 100 48">
        {/* Guide Lines */}
        <line x1="0" y1="8" x2="100" y2="8" stroke="rgba(255,255,255,0.1)" vectorEffect="non-scaling-stroke" />
        <line x1="0" y1="40" x2="100" y2="40" stroke="rgba(255,255,255,0.1)" vectorEffect="non-scaling-stroke" />

        <path d={generatePath()} fill="none" stroke="#4ade80" strokeWidth="2" vectorEffect="non-scaling-stroke" />
      </svg>
      {points.map((p, i) => (
        <div
          key={i}
          className={`absolute w-3 h-3 bg-white border-2 border-[#4ade80] rounded-full transform -translate-x-1/2 -translate-y-1/2 cursor-pointer transition-transform shadow-sm ${hoveredIdx === i ? 'scale-125 ring-2 ring-red-500/50' : 'hover:scale-125'
            }`}
          style={{
            left: `${(p.time / duration) * 100}%`,
            top: `${8 + (1 - p.value) * 32}px`
          }}
          onMouseEnter={() => setHoveredIdx(i)}
          onMouseLeave={() => setHoveredIdx(null)}
          onMouseDown={(e) => {
            e.stopPropagation();
            draggingIdxRef.current = i;

            const rect = e.currentTarget.parentElement!.getBoundingClientRect();

            const mm = (me: MouseEvent) => {
              const mx = me.clientX - rect.left;
              const my = me.clientY - rect.top;

              let t = (mx / rect.width) * duration;
              t = Math.max(0, Math.min(duration, t));

              if (i === 0) t = 0;
              if (i === pointsRef.current.length - 1 && pointsRef.current.length > 1) t = duration;

              let v = 1 - (my - 8) / 32;
              v = Math.max(0, Math.min(1, v));

              const next = [...pointsRef.current];
              if (draggingIdxRef.current !== null && next[draggingIdxRef.current]) {
                next[draggingIdxRef.current] = { time: t, value: v };
                onUpdatePoints(next);
              }
            };

            const mu = () => {
              window.removeEventListener('mousemove', mm);
              window.removeEventListener('mouseup', mu);
              draggingIdxRef.current = null;
              const sorted = [...pointsRef.current].sort((a, b) => a.time - b.time);
              onUpdatePoints(sorted);
            };

            window.addEventListener('mousemove', mm);
            window.addEventListener('mouseup', mu);
          }}
        />
      ))}
    </div>
  );
};

const TrimHandles: React.FC<{
  segment: VideoSegment;
  duration: number;
  onTrimDragStart: (type: 'start' | 'end') => void;
}> = ({ segment, duration, onTrimDragStart }) => (
  <>
    <div
      className="absolute -top-2 -bottom-2 w-4 cursor-col-resize z-30 group"
      style={{ left: `calc(${(segment.trimStart / duration) * 100}% - 8px)` }}
      onMouseDown={() => onTrimDragStart('start')}
    >
      <div className="absolute inset-y-0 w-2 bg-white/80 group-hover:bg-[#0079d3] group-hover:w-2.5 transition-all rounded-full left-1/2 transform -translate-x-1/2" />
      <div className="absolute inset-y-2 left-1/2 transform -translate-x-1/2 flex flex-col justify-center gap-1">
        <div className="w-0.5 h-1 bg-black/40 rounded-full" />
        <div className="w-0.5 h-1 bg-black/40 rounded-full" />
      </div>
    </div>

    <div
      className="absolute -top-2 -bottom-2 w-4 cursor-col-resize z-30 group"
      style={{ left: `calc(${(segment.trimEnd / duration) * 100}% - 8px)` }}
      onMouseDown={() => onTrimDragStart('end')}
    >
      <div className="absolute inset-y-0 w-2 bg-white/80 group-hover:bg-[#0079d3] group-hover:w-2.5 transition-all rounded-full left-1/2 transform -translate-x-1/2" />
      <div className="absolute inset-y-2 left-1/2 transform -translate-x-1/2 flex flex-col justify-center gap-1">
        <div className="w-0.5 h-1 bg-black/40 rounded-full" />
        <div className="w-0.5 h-1 bg-black/40 rounded-full" />
      </div>
    </div>
  </>
);

const Playhead: React.FC<{ currentTime: number; duration: number }> = ({ currentTime, duration }) => (
  <div
    className="absolute top-0 bottom-[-24px] flex flex-col items-center pointer-events-none z-50"
    style={{
      left: `${(currentTime / duration) * 100}%`,
      transform: 'translateX(-50%)'
    }}
  >
    <div className="w-4 h-3 bg-red-500 rounded-t" />
    <div className="w-0.5 flex-1 bg-red-500" />
  </div>
);

const TextTrack: React.FC<{
  segment: VideoSegment;
  duration: number;
  editingTextId: string | null;
  isDraggingTextStart: boolean;
  isDraggingTextEnd: boolean;
  onTextClick: (id: string) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
}> = ({ segment, duration, editingTextId, isDraggingTextStart, isDraggingTextEnd, onTextClick, onHandleDragStart }) => (
  <div className="absolute inset-x-0 bottom-14 h-8 bg-[#272729] rounded-lg z-30">
    {segment.textSegments?.map((text) => (
      <div
        key={text.id}
        onMouseDown={(e) => {
          e.stopPropagation();
          const rect = e.currentTarget.parentElement!.getBoundingClientRect();
          const clickX = e.clientX - rect.left;
          const clickTime = (clickX / rect.width) * duration;
          onHandleDragStart(text.id, 'body', clickTime - text.startTime);
        }}
        onClick={(e) => {
          e.stopPropagation();
          // Prevent click when dragging
          if (!isDraggingTextStart && !isDraggingTextEnd) {
            onTextClick(text.id);
          }
        }}
        className={`absolute h-full cursor-move group ${editingTextId === text.id ? 'bg-[#0079d3]/40 ring-1 ring-[#0079d3]' : 'bg-[#0079d3]/20 hover:bg-[#0079d3]/25'
          }`}
        style={{
          left: `${(text.startTime / duration) * 100}%`,
          width: `${((text.endTime - text.startTime) / duration) * 100}%`
        }}
      >
        <div className="absolute inset-y-0 flex items-center justify-center w-full">
          <div className="px-2 truncate text-xs font-medium text-[#d7dadc]">
            {text.text}
          </div>
        </div>
        {/* Drag handles */}
        <div
          className="absolute inset-y-0 left-0 w-1 cursor-ew-resize group-hover:bg-[#0079d3]"
          onMouseDown={(e) => {
            e.stopPropagation();
            onHandleDragStart(text.id, 'start');
          }}
        />
        <div
          className="absolute inset-y-0 right-0 w-1 cursor-ew-resize group-hover:bg-[#0079d3]"
          onMouseDown={(e) => {
            e.stopPropagation();
            onHandleDragStart(text.id, 'end');
          }}
        />
      </div>
    ))}
  </div>
);

export const Timeline: React.FC<TimelineProps> = ({
  duration,
  currentTime,
  segment,
  thumbnails,
  timelineRef,
  videoRef,
  editingKeyframeId,
  editingTextId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setActivePanel,
  setSegment,
  onSeek
}) => {
  const [isDraggingTrimStart, setIsDraggingTrimStart] = useState(false);
  const [isDraggingTrimEnd, setIsDraggingTrimEnd] = useState(false);
  const [isDraggingTextStart, setIsDraggingTextStart] = useState(false);
  const [isDraggingTextEnd, setIsDraggingTextEnd] = useState(false);
  const [isDraggingTextBody, setIsDraggingTextBody] = useState(false);
  const [textDragOffset, setTextDragOffset] = useState(0);
  const [draggingTextId, setDraggingTextId] = useState<string | null>(null);
  const [isDraggingZoom, setIsDraggingZoom] = useState(false);
  const [draggingZoomIdx, setDraggingZoomIdx] = useState<number | null>(null);
  const [isDraggingSeek, setIsDraggingSeek] = useState(false);

  const handleSeek = (clientX: number) => {
    const timeline = timelineRef.current;
    if (!timeline || !segment) return;

    const rect = timeline.getBoundingClientRect();
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    const time = (x / rect.width) * duration;

    // Use controller's seek method to properly sync video and audio
    if (onSeek) {
      onSeek(time);
    } else if (videoRef.current && Math.abs(videoRef.current.currentTime - time) > 0.05) {
      // Fallback to direct video manipulation
      videoRef.current.currentTime = time;
      setCurrentTime(time);
    }
  };

  const handleZoomDragStart = (index: number) => {
    setIsDraggingZoom(true);
    setDraggingZoomIdx(index);
    setEditingKeyframeId(index);
    setActivePanel("zoom");
  };

  const handleZoomDrag = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDraggingZoom || draggingZoomIdx === null || !segment) return;

    const timeline = timelineRef.current;
    if (!timeline) return;

    const rect = timeline.getBoundingClientRect();
    const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    const newTime = (x / rect.width) * duration;

    // Check bounds against neighbors
    const prevKeyframe = draggingZoomIdx > 0 ? segment.zoomKeyframes[draggingZoomIdx - 1] : null;
    const nextKeyframe = draggingZoomIdx < segment.zoomKeyframes.length - 1 ? segment.zoomKeyframes[draggingZoomIdx + 1] : null;

    let finalTime = newTime;
    if (prevKeyframe && finalTime <= prevKeyframe.time + 0.1) finalTime = prevKeyframe.time + 0.1;
    if (nextKeyframe && finalTime >= nextKeyframe.time - 0.1) finalTime = nextKeyframe.time - 0.1;

    setSegment({
      ...segment,
      zoomKeyframes: segment.zoomKeyframes.map((kf, i) =>
        i === draggingZoomIdx ? { ...kf, time: finalTime } : kf
      )
    });

    if (videoRef.current) {
      videoRef.current.currentTime = finalTime;
      setCurrentTime(finalTime);
    }
  };





  const handleTrimDragStart = (type: 'start' | 'end') => {
    if (type === 'start') setIsDraggingTrimStart(true);
    else setIsDraggingTrimEnd(true);
  };

  const handleTrimDrag = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDraggingTrimStart && !isDraggingTrimEnd) return;

    const timeline = timelineRef.current;
    if (!timeline || !segment) return;

    const rect = timeline.getBoundingClientRect();
    const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    const percent = x / rect.width;
    const newTime = percent * duration;

    if (isDraggingTrimStart) {
      const newTrimStart = Math.min(newTime, segment.trimEnd - 0.1);
      setSegment({
        ...segment,
        trimStart: Math.max(0, newTrimStart)
      });
      if (videoRef.current) {
        videoRef.current.currentTime = newTime;
      }
    }

    if (isDraggingTrimEnd) {
      const newTrimEnd = Math.max(newTime, segment.trimStart + 0.1);
      setSegment({
        ...segment,
        trimEnd: Math.min(duration, newTrimEnd)
      });
      if (videoRef.current) {
        videoRef.current.currentTime = newTime;
      }
    }
  };

  const handleTrimDragEnd = () => {
    setIsDraggingTrimStart(false);
    setIsDraggingTrimEnd(false);
  };

  const handleTextDrag = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDraggingTextStart && !isDraggingTextEnd && !isDraggingTextBody || !draggingTextId || !segment) return;

    const timeline = timelineRef.current;
    if (!timeline) return;

    const rect = timeline.getBoundingClientRect();
    const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    const newTime = (x / rect.width) * duration;

    setSegment({
      ...segment,
      textSegments: segment.textSegments.map(text => {
        if (text.id !== draggingTextId) return text;

        if (isDraggingTextStart) {
          return {
            ...text,
            startTime: Math.min(Math.max(0, newTime), text.endTime - 0.1)
          };
        } else if (isDraggingTextEnd) {
          return {
            ...text,
            endTime: Math.max(Math.min(duration, newTime), text.startTime + 0.1)
          };
        } else if (isDraggingTextBody) {
          const currentDuration = text.endTime - text.startTime;
          let newStart = newTime - textDragOffset;

          // Clamp to stay within timeline bounds
          if (newStart < 0) newStart = 0;
          if (newStart + currentDuration > duration) newStart = duration - currentDuration;

          return {
            ...text,
            startTime: newStart,
            endTime: newStart + currentDuration
          };
        }
        return text;
      })
    });
  };

  return (
    <div className="relative h-48">
      <div
        ref={timelineRef}
        className="h-32 bg-[#1a1a1b] rounded-lg cursor-pointer relative mt-12"
        onMouseDown={(e) => {
          if (isDraggingTrimStart || isDraggingTrimEnd || isDraggingTextStart || isDraggingTextEnd || isDraggingTextBody || isDraggingZoom) return;
          setIsDraggingSeek(true);
          handleSeek(e.clientX);
        }}
        onMouseMove={(e) => {
          handleTrimDrag(e);
          handleTextDrag(e);
          handleZoomDrag(e);
          if (isDraggingSeek) {
            handleSeek(e.clientX);
          }
        }}
        onMouseUp={() => {
          handleTrimDragEnd();
          setIsDraggingTextStart(false);
          setIsDraggingTextEnd(false);
          setIsDraggingTextBody(false);
          setIsDraggingZoom(false);
          setDraggingZoomIdx(null);
          setDraggingTextId(null);
          setIsDraggingSeek(false);
        }}
        onMouseLeave={() => {
          handleTrimDragEnd();
          setIsDraggingTextStart(false);
          setIsDraggingTextEnd(false);
          setIsDraggingTextBody(false);
          setIsDraggingZoom(false);
          setDraggingZoomIdx(null);
          setDraggingTextId(null);
          setIsDraggingSeek(false);
        }}


      >
        <TimeMarkers duration={duration} />
        {segment && (
          <>
            {/* Render Influence Track on top of video track if path exists meaning Smart Zoom is ON */}
            {segment.smoothMotionPath && segment.smoothMotionPath.length > 0 && (
              <ZoomInfluenceTrack
                segment={segment}
                duration={duration}
                onUpdatePoints={(points) => {
                  const newSegment = { ...segment!, zoomInfluencePoints: points };
                  if (points.length === 0) {
                    newSegment.smoothMotionPath = [];
                  }
                  setSegment(newSegment);
                }}
              />
            )}

            {/* Base track with thumbnails */}
            <div className="absolute inset-x-0 bottom-0 h-12">
              <VideoTrack
                segment={segment}
                duration={duration}
                thumbnails={thumbnails}
              />

              <div className="absolute inset-0">
                <ZoomKeyframes
                  segment={segment}
                  duration={duration}
                  editingKeyframeId={editingKeyframeId}
                  onKeyframeClick={(time, index) => {
                    if (videoRef.current) {
                      videoRef.current.currentTime = time;
                      setCurrentTime(time);
                      setEditingKeyframeId(index);
                      setActivePanel("zoom");
                    }
                  }}
                  onKeyframeDragStart={handleZoomDragStart}
                />
              </div>

              {/* Trim handles */}
              <TrimHandles
                segment={segment}
                duration={duration}
                onTrimDragStart={handleTrimDragStart}
              />
            </div>

            {/* Text track */}
            <TextTrack
              segment={segment}
              duration={duration}
              editingTextId={editingTextId}
              isDraggingTextStart={isDraggingTextStart}
              isDraggingTextEnd={isDraggingTextEnd}
              onTextClick={(id) => {
                setEditingTextId(id);
                setActivePanel('text');
              }}
              onHandleDragStart={(id, type, offset) => {
                setDraggingTextId(id);
                if (type === 'start') setIsDraggingTextStart(true);
                else if (type === 'end') setIsDraggingTextEnd(true);
                else if (type === 'body') {
                  setIsDraggingTextBody(true);
                  if (offset !== undefined) setTextDragOffset(offset);
                }
              }}
            />
          </>
        )}

        {/* Playhead */}
        <Playhead
          currentTime={currentTime}
          duration={duration}
        />
      </div>

      {/* Duration display */}
      <div className="absolute bottom-0 left-1/2 transform -translate-x-1/2 text-sm text-[#818384]">
        {segment ? formatTime(segment.trimEnd - segment.trimStart) : formatTime(duration)}
      </div>
    </div>
  );
};
