import React, { useState } from 'react';
import { VideoSegment } from '@/types/video';
import { ZoomTrack } from './ZoomTrack';
import { TextTrack } from './TextTrack';
import { PointerTrack } from './PointerTrack';
import { TrimTrack } from './TrimTrack';
import { Playhead } from './Playhead';
import { useTimelineDrag } from './useTimelineDrag';
import { useSettings } from '@/hooks/useSettings';
import { ZoomDebugOverlay } from './ZoomDebugOverlay';

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
  editingPointerId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  onAddText?: (atTime?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  beginBatch: () => void;
  commitBatch: () => void;
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
  editingPointerId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingPointerId,
  setActivePanel,
  setSegment,
  onSeek,
  onSeekEnd,
  onAddText,
  onAddPointerSegment,
  beginBatch,
  commitBatch,
}) => {
  const { t } = useSettings();
  const [showDebug, setShowDebug] = useState(false);
  const {
    dragState,
    handleTrimDragStart,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handlePointerDragStart,
    handlePointerClick,
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
    setEditingPointerId,
    setActivePanel,
    onSeek,
    onSeekEnd,
    beginBatch,
    commitBatch,
  });

  return (
    <div className="timeline-area select-none mx-2">
      {/* Track container with label gutter + content area */}
      <div className="timeline-tracks-row flex gap-3">
        {/* Label gutter */}
        <div className="timeline-label-gutter w-10 flex-shrink-0 flex flex-col gap-[2px]">
          <div className="timeline-label-zoom h-10 flex items-center gap-0.5">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">{t.trackZoom}</span>
            <button
              onClick={() => setShowDebug(v => !v)}
              className={`timeline-debug-btn w-3 h-3 rounded-sm text-[7px] font-bold leading-none flex items-center justify-center transition-colors ${
                showDebug ? 'bg-blue-500 text-white' : 'bg-[var(--surface-container)] text-[var(--outline)] hover:text-[var(--on-surface)]'
              }`}
              title="Debug zoom curve"
            >
              D
            </button>
          </div>
          {showDebug && (
            <div className="timeline-label-debug h-10 flex items-center">
              <span className="text-[10px] font-medium text-[var(--outline)] leading-none opacity-50">dbg</span>
            </div>
          )}
          <div className="timeline-label-text h-7 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">{t.trackText}</span>
          </div>
          <div className="timeline-label-pointer h-7 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">{t.trackPointer}</span>
          </div>
          <div className="timeline-label-video h-10 flex items-center">
            <span className="text-[10px] font-medium text-[var(--outline)] leading-none">{t.trackVideo}</span>
          </div>
        </div>

        {/* Content area - timelineRef only covers this, so seek math is correct */}
        <div
          ref={timelineRef}
          className="timeline-content flex-1 relative cursor-pointer"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
        >
          <div className="timeline-tracks flex flex-col gap-[2px]">
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
                onUpdateKeyframes={(keyframes) => {
                  setSegment({ ...segment, zoomKeyframes: keyframes });
                }}
                beginBatch={beginBatch}
                commitBatch={commitBatch}
              />
            ) : (
              <div className="zoom-track-empty h-10 rounded bg-[var(--surface-container)]/60" />
            )}

            {/* Debug Overlay */}
            {showDebug && segment && (
              <ZoomDebugOverlay segment={segment} duration={duration} />
            )}

            {/* Text Track */}
            {segment ? (
              <TextTrack
                segment={segment}
                duration={duration}
                editingTextId={editingTextId}
                onTextClick={handleTextClick}
                onHandleDragStart={handleTextDragStart}
                onAddText={onAddText}
              />
            ) : (
              <div className="text-track-empty h-7 rounded bg-[var(--surface)]/80" />
            )}

            {/* Pointer Track */}
            {segment && (
              <PointerTrack
                segment={segment}
                duration={duration}
                editingPointerId={editingPointerId}
                onPointerClick={handlePointerClick}
                onHandleDragStart={handlePointerDragStart}
                onAddPointerSegment={onAddPointerSegment}
              />
            )}

            {/* Video/Trim Track */}
            {segment ? (
              <TrimTrack
                segment={segment}
                duration={duration}
                thumbnails={thumbnails}
                onTrimDragStart={handleTrimDragStart}
                isDraggingTrim={dragState.isDraggingTrimStart || dragState.isDraggingTrimEnd}
              />
            ) : (
              <div className="trim-track-empty h-10 rounded bg-[var(--surface-container)]/60" />
            )}
          </div>

          {/* Playhead spanning all tracks */}
          {segment && (
            <Playhead currentTime={currentTime} duration={duration} />
          )}
        </div>
      </div>

      {/* Time ruler */}
      <div className="timeline-ruler-row flex gap-3 mt-0.5">
        <div className="timeline-ruler-gutter w-10 flex-shrink-0" />
        <div className="timeline-ruler flex-1 relative h-4 select-none">
          {duration > 0 && (() => {
            const tickCount = duration <= 5 ? 5 : duration <= 15 ? 8 : duration <= 30 ? 10 : 12;
            return Array.from({ length: tickCount + 1 }).map((_, i) => {
              const time = (duration * i) / tickCount;
              const left = (i / tickCount) * 100;
              const isMajor = i === 0 || i === tickCount || i % Math.ceil(tickCount / 4) === 0;
              return (
                <div
                  key={i}
                  className="timeline-tick absolute flex flex-col items-center"
                  style={{ left: `${left}%`, transform: 'translateX(-50%)', top: 0 }}
                >
                  <div className={`w-px ${isMajor ? 'h-1.5 bg-[var(--outline)]/40' : 'h-1 bg-[var(--outline)]/20'}`} />
                  {isMajor && (
                    <span className="text-[9px] font-mono text-[var(--outline)] leading-none mt-0.5">
                      {formatTime(time)}
                    </span>
                  )}
                </div>
              );
            });
          })()}
        </div>
      </div>
    </div>
  );
};
