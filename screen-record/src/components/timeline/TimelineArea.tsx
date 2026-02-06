import React from 'react';
import { VideoSegment } from '@/types/video';
import { TimeRuler } from './TimeRuler';
import { ZoomTrack } from './ZoomTrack';
import { TextTrack } from './TextTrack';
import { TrimTrack } from './TrimTrack';
import { Playhead } from './Playhead';
import { useTimelineDrag } from './useTimelineDrag';

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

interface TimelineAreaProps {
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
  autoZoomButton?: React.ReactNode;
}

export const TimelineArea: React.FC<TimelineAreaProps> = ({
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
  onSeek,
  autoZoomButton,
}) => {
  const {
    handleTrimDragStart,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handleKeyframeClick,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
  } = useTimelineDrag({
    duration,
    segment,
    timelineRef,
    videoRef,
    setCurrentTime,
    setSegment,
    setEditingKeyframeId,
    setEditingTextId,
    setActivePanel,
    onSeek,
  });

  return (
    <div className="select-none">
      {/* Auto zoom button */}
      {autoZoomButton && (
        <div className="flex justify-end mb-1">
          {autoZoomButton}
        </div>
      )}

      {/* Ruler row */}
      <div className="flex items-end mb-0.5">
        <div className="w-9 flex-shrink-0" />
        <div className="flex-1">
          <TimeRuler duration={duration} />
        </div>
      </div>

      {/* Track container with label gutter + content area */}
      <div className="flex">
        {/* Label gutter - outside the time-mapped area */}
        <div className="w-9 flex-shrink-0 flex flex-col gap-[2px]">
          <div className="h-10 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">Zoom</span>
          </div>
          <div className="h-7 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">Text</span>
          </div>
          <div className="h-10 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">Video</span>
          </div>
        </div>

        {/* Content area - timelineRef only covers this, so seek math is correct */}
        <div
          ref={timelineRef}
          className="flex-1 relative cursor-pointer"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
        >
          <div className="flex flex-col gap-[2px]">
            {/* Zoom Track */}
            {segment ? (
              <ZoomTrack
                segment={segment}
                duration={duration}
                editingKeyframeId={editingKeyframeId}
                onKeyframeClick={handleKeyframeClick}
                onKeyframeDragStart={handleZoomDragStart}
                onUpdateInfluencePoints={(points) => {
                  const newSegment = { ...segment, zoomInfluencePoints: points };
                  if (points.length === 0) newSegment.smoothMotionPath = [];
                  setSegment(newSegment);
                }}
              />
            ) : (
              <div className="h-10 rounded bg-[var(--surface-container)]/60" />
            )}

            {/* Text Track */}
            {segment ? (
              <TextTrack
                segment={segment}
                duration={duration}
                editingTextId={editingTextId}
                onTextClick={handleTextClick}
                onHandleDragStart={handleTextDragStart}
              />
            ) : (
              <div className="h-7 rounded bg-[var(--surface)]/80" />
            )}

            {/* Video/Trim Track */}
            {segment ? (
              <TrimTrack
                segment={segment}
                duration={duration}
                thumbnails={thumbnails}
                onTrimDragStart={handleTrimDragStart}
              />
            ) : (
              <div className="h-10 rounded bg-[var(--surface-container)]/60" />
            )}
          </div>

          {/* Playhead spanning all tracks - positioned within content area */}
          {segment && (
            <Playhead currentTime={currentTime} duration={duration} />
          )}
        </div>
      </div>

      {/* Duration display */}
      <div className="text-center mt-1">
        <span className="text-[11px] text-[var(--outline)] tabular-nums">
          {segment ? formatTime(segment.trimEnd - segment.trimStart) : formatTime(duration)}
        </span>
      </div>
    </div>
  );
};
