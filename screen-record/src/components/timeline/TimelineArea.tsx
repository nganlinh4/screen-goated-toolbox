import React, { useCallback, useEffect, useState } from "react";
import type { VideoSegment } from "@/types/video";
import { useSettings } from "@/hooks/useSettings";
import { KeystrokeTrack } from "./KeystrokeTrack";
import { MicTrack } from "./MicTrack";
import { Playhead } from "./Playhead";
import { PointerTrack } from "./PointerTrack";
import { DeviceAudioTrack } from "./DeviceAudioTrack";
import { SpeedTrack } from "./SpeedTrack";
import { TextTrack } from "./TextTrack";
import { TrimTrack } from "./TrimTrack";
import { WebcamVisibilityTrack } from "./WebcamVisibilityTrack";
import { ZoomDebugOverlay } from "./ZoomDebugOverlay";
import { ZoomTrack } from "./ZoomTrack";
import { buildTimelineRulerTicks } from "./timelineRuler";
import { useTimelineDrag } from "./useTimelineDrag";
import { useTimelineViewport } from "./useTimelineViewport";
import { Slider } from "@/components/ui/Slider";
import {
  clampVisibilitySegmentsToDuration,
  mergePointerSegments,
} from "@/lib/cursorHiding";

const TIMELINE_TRACK_GAP_PX = 2;
const TIMELINE_TRACK_HEIGHTS = {
  zoom: 40,
  debug: 40,
  speed: 40,
  deviceAudio: 40,
  micAudio: 40,
  webcam: 28,
  text: 28,
  keystroke: 28,
  pointer: 28,
  trimLane: 40,
} as const;

interface TimelineAreaProps {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  thumbnails: string[];
  timelineRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text") => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  onAddText?: (atTime?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  isPlaying?: boolean;
  onViewportZoomChange?: (zoom: number) => void;
  onViewportCanvasWidthChange?: (widthPx: number) => void;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  beginBatch: () => void;
  commitBatch: () => void;
  onTextSelectionChange?: (ids: string[]) => void;
  onPointerSelectionChange?: (ids: string[]) => void;
  onKeystrokeSelectionChange?: (ids: string[]) => void;
  onWebcamSelectionChange?: (ids: string[]) => void;
  clearSelectionSignal?: number;
  hasMouseData?: boolean;
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
  editingKeystrokeSegmentId,
  setCurrentTime,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingKeystrokeSegmentId,
  setEditingPointerId,
  setActivePanel,
  setSegment,
  onSeek,
  onSeekEnd,
  onAddText,
  onAddKeystrokeSegment,
  onAddPointerSegment,
  isPlaying,
  onViewportZoomChange,
  onViewportCanvasWidthChange,
  isDeviceAudioAvailable,
  isMicAudioAvailable,
  isWebcamAvailable,
  beginBatch,
  commitBatch,
  onTextSelectionChange,
  onPointerSelectionChange,
  onKeystrokeSelectionChange,
  onWebcamSelectionChange,
  clearSelectionSignal,
  hasMouseData = true,
}) => {
  const { t } = useSettings();
  const [showDebug, setShowDebug] = useState(false);
  const showEmptyRuler = duration <= 0;
  const clampTrackDelay = (value: number) =>
    Math.max(-2, Math.min(2, value));
  const renderTrackDelayLabel = ({
    className,
    groupClassName,
    label,
    value,
    onChange,
    isAvailable,
    heightClassName,
  }: {
    className: string;
    groupClassName: string;
    label: string;
    value: number;
    onChange: (value: number) => void;
    isAvailable: boolean;
    heightClassName: string;
  }) => (
    <div
      className={`${className} ${heightClassName} relative flex items-center ${
        isAvailable ? "" : "timeline-label-unavailable"
      } ${groupClassName}`}
    >
      <div className="timeline-label-hover-bridge absolute left-full inset-y-0 w-3" />
      <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
        {label}
      </span>
      <div className="playback-keystroke-delay-popover absolute left-[calc(100%+8px)] top-1/2 z-30 -translate-y-1/2 w-[218px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-x-1 transition-all duration-150 group-hover:opacity-100 group-hover:translate-x-0 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:translate-x-0 group-focus-within:pointer-events-auto">
        <div className="flex items-center gap-3">
          <div className="flex-1 rounded-full px-1 py-[3px]">
            <Slider
              min={-2}
              max={2}
              step={0.01}
              value={value}
              disabled={!isAvailable || !segment}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(nextValue) => onChange(clampTrackDelay(nextValue))}
              className="playback-keystroke-delay-slider block w-full"
            />
          </div>
          <span className="text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-12 text-right">
            {value.toFixed(2)}s
          </span>
        </div>
      </div>
    </div>
  );
  const keystrokeTrackLabel =
    segment?.keystrokeMode === "keyboard"
      ? t.trackKeyboard
      : segment?.keystrokeMode === "keyboardMouse"
        ? t.trackKeyboardMouse
        : t.trackKeystrokesOff;
  const showDeviceAudio = isDeviceAudioAvailable;
  const showMicAudio = isMicAudioAvailable;
  const showWebcam = isWebcamAvailable;
  const showKeystroke = (segment?.keystrokeMode ?? 'off') !== 'off';

  const trackHeightsBeforeTrim = [
    TIMELINE_TRACK_HEIGHTS.zoom,
    ...(showDebug ? [TIMELINE_TRACK_HEIGHTS.debug] : []),
    TIMELINE_TRACK_HEIGHTS.speed,
    ...(showDeviceAudio ? [TIMELINE_TRACK_HEIGHTS.deviceAudio] : []),
    ...(showMicAudio ? [TIMELINE_TRACK_HEIGHTS.micAudio] : []),
    ...(showWebcam ? [TIMELINE_TRACK_HEIGHTS.webcam] : []),
    TIMELINE_TRACK_HEIGHTS.text,
    ...(showKeystroke ? [TIMELINE_TRACK_HEIGHTS.keystroke] : []),
    ...(hasMouseData ? [TIMELINE_TRACK_HEIGHTS.pointer] : []),
  ];
  const trimHeadCenterY =
    trackHeightsBeforeTrim.reduce((sum, height) => sum + height, 0) +
    trackHeightsBeforeTrim.length * TIMELINE_TRACK_GAP_PX +
    TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const trimLaneBottomY = trimHeadCenterY + TIMELINE_TRACK_HEIGHTS.trimLane / 2;
  const {
    dragState,
    handleTrimDragStart,
    handleTrimSplit,
    handleTrimAddSegment,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handleKeystrokeDragStart,
    handleKeystrokeClick,
    handlePointerDragStart,
    handlePointerClick,
    handleWebcamDragStart,
    handleWebcamClick,
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
    setEditingKeystrokeId: setEditingKeystrokeSegmentId,
    setEditingPointerId,
    setActivePanel,
    onSeek,
    onSeekEnd,
    beginBatch,
    commitBatch,
  });

  const isTimelineInteracting =
    dragState.isDraggingTrimStart ||
    dragState.isDraggingTrimEnd ||
    dragState.isDraggingTextStart ||
    dragState.isDraggingTextEnd ||
    dragState.isDraggingTextBody ||
    dragState.isDraggingKeystrokeStart ||
    dragState.isDraggingKeystrokeEnd ||
    dragState.isDraggingKeystrokeBody ||
    dragState.isDraggingPointerStart ||
    dragState.isDraggingPointerEnd ||
    dragState.isDraggingPointerBody ||
    dragState.isDraggingWebcamStart ||
    dragState.isDraggingWebcamEnd ||
    dragState.isDraggingWebcamBody ||
    dragState.isDraggingZoom ||
    dragState.isDraggingSeek;

  const handleDeletePointerSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.cursorVisibilitySegments || []).filter(s => !idSet.has(s.id));
    setSegment({ ...segment, cursorVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleTextSplit = useCallback((id: string, splitTime: number) => {
    if (!segment) return;
    beginBatch();
    const texts = segment.textSegments ?? [];
    const target = texts.find(t => t.id === id);
    if (!target || splitTime <= target.startTime + 0.1 || splitTime >= target.endTime - 0.1) {
      commitBatch();
      return;
    }
    const left = { ...target, endTime: splitTime - 0.01 };
    const right = { ...target, id: crypto.randomUUID(), startTime: splitTime + 0.01 };
    setSegment({
      ...segment,
      textSegments: texts.map(t => t.id === id ? left : t).concat(right),
    });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteTextSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.textSegments ?? []).filter(t => !idSet.has(t.id));
    setSegment({ ...segment, textSegments: remaining });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteKeystrokeSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const mode = segment.keystrokeMode ?? 'off';
    if (mode === 'keyboard') {
      const remaining = (segment.keyboardVisibilitySegments || []).filter(s => !idSet.has(s.id));
      setSegment({ ...segment, keyboardVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    } else if (mode === 'keyboardMouse') {
      const remaining = (segment.keyboardMouseVisibilitySegments || []).filter(s => !idSet.has(s.id));
      setSegment({ ...segment, keyboardMouseVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    }
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const handleDeleteWebcamSegments = useCallback((ids: string[]) => {
    if (!segment) return;
    beginBatch();
    const idSet = new Set(ids);
    const remaining = (segment.webcamVisibilitySegments || []).filter(s => !idSet.has(s.id));
    setSegment({ ...segment, webcamVisibilitySegments: remaining.length > 0 ? remaining : undefined });
    commitBatch();
  }, [segment, setSegment, beginBatch, commitBatch]);

  const {
    viewportRef,
    scrollbarTrackRef,
    scrollbarThumbRef,
    zoom,
    showScrollbar,
    canvasWidth,
    canvasWidthPx,
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  } = useTimelineViewport({
    duration,
    currentTime,
    segment,
    timelineRef,
    videoRef,
    isPlaying: !!isPlaying,
    isInteracting: isTimelineInteracting,
  });
  const rulerTicks = buildTimelineRulerTicks({
    duration,
    widthPx: canvasWidthPx,
    speedPoints: segment?.speedPoints,
  });

  useEffect(() => {
    onViewportZoomChange?.(zoom);
  }, [onViewportZoomChange, zoom]);

  useEffect(() => {
    onViewportCanvasWidthChange?.(canvasWidthPx);
  }, [canvasWidthPx, onViewportCanvasWidthChange]);

  return (
    <div className="timeline-area select-none mx-2">
      <div className="timeline-shell flex gap-4">
        <div className="timeline-side-column w-[4rem] flex-shrink-0">
          <div className="timeline-label-gutter flex flex-col gap-[2px] border-r border-[var(--ui-border)] pr-2">
            <div className="timeline-label-zoom h-10 flex items-center justify-between">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackZoom}
              </span>
              <button
                onClick={() => setShowDebug((value) => !value)}
                className={`timeline-debug-btn w-3 h-3 rounded-sm text-[7px] font-bold leading-none flex items-center justify-center transition-colors ${
                  showDebug
                    ? "bg-blue-500 text-white"
                    : "ui-surface text-[var(--outline)] hover:text-[var(--on-surface)]"
                }`}
                title="Debug zoom curve"
              >
                D
              </button>
            </div>
            {showDebug && (
              <div className="timeline-label-debug h-10 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none opacity-50">
                  dbg
                </span>
              </div>
            )}
            <div className="timeline-label-speed h-10 flex items-center justify-between">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackSpeed || "Speed"}
              </span>
              <button
                onClick={() => {
                  if (!segment) return;
                  beginBatch();
                  setSegment({
                    ...segment,
                    speedPoints: [
                      { time: 0, speed: 1 },
                      { time: duration, speed: 1 },
                    ],
                  });
                  commitBatch();
                }}
                disabled={!segment}
                className="timeline-speed-reset-btn ui-icon-button p-1 text-[9px] font-mono leading-none disabled:opacity-40 disabled:hover:text-[var(--outline)] disabled:hover:bg-transparent"
                title={t.resetSpeed || "Reset"}
              >
                R
              </button>
            </div>
            {showDeviceAudio && (
              <div className="timeline-label-device-audio h-10 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackDeviceAudio}
                </span>
              </div>
            )}
            {showMicAudio && renderTrackDelayLabel({
              className: "timeline-label-mic-audio",
              groupClassName: "group",
              label: t.trackMicAudio,
              value: segment?.micAudioOffsetSec ?? 0,
              onChange: (value) => {
                if (!segment || !isMicAudioAvailable) return;
                setSegment({ ...segment, micAudioOffsetSec: value });
              },
              isAvailable: true,
              heightClassName: "h-10",
            })}
            {showWebcam && renderTrackDelayLabel({
              className: "timeline-label-webcam",
              groupClassName: "group",
              label: t.trackWebcam,
              value: segment?.webcamOffsetSec ?? 0,
              onChange: (value) => {
                if (!segment || !isWebcamAvailable) return;
                setSegment({ ...segment, webcamOffsetSec: value });
              },
              isAvailable: true,
              heightClassName: "h-7",
            })}
            <div className="timeline-label-text h-7 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackText}
              </span>
            </div>
            {showKeystroke && (
              <div className="timeline-label-keystrokes h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {keystrokeTrackLabel}
                </span>
              </div>
            )}
            {hasMouseData && (
              <div className="timeline-label-pointer h-7 flex items-center">
                <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                  {t.trackPointer}
                </span>
              </div>
            )}
            <div className="timeline-label-video h-10 flex items-center">
              <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
                {t.trackVideo}
              </span>
            </div>
          </div>
          <div className="timeline-ruler-spacer h-4 mt-0.5" />
        </div>

        <div className="timeline-main-column flex-1 min-w-0">
          <div
            ref={viewportRef}
            className="timeline-scroll-viewport"
            data-zoomed={zoom > 1 ? "true" : "false"}
          >
            <div className="timeline-canvas" style={{ width: canvasWidth }}>
              <div
                ref={timelineRef}
                className={`timeline-content relative touch-none w-full ${
                  dragState.isDraggingSeek ? "cursor-grabbing" : "cursor-grab"
                }`}
                onPointerDown={handleMouseDown}
                onPointerMove={handleMouseMove}
                onPointerUp={handleMouseUp}
                onPointerCancel={handleMouseUp}
              >
                <div className="timeline-tracks flex flex-col gap-[2px]">
                  {segment ? (
                    <ZoomTrack
                      segment={segment}
                      duration={duration}
                      editingKeyframeId={editingKeyframeId}
                      onKeyframeClick={handleKeyframeClick}
                      onKeyframeDragStart={handleZoomDragStart}
                      onUpdateInfluencePoints={(points) => {
                        const nextSegment = {
                          ...segment,
                          zoomInfluencePoints: points,
                        };
                        if (points.length === 0) nextSegment.smoothMotionPath = [];
                        setSegment(nextSegment);
                      }}
                      onUpdateKeyframes={(keyframes) => {
                        setSegment({ ...segment, zoomKeyframes: keyframes });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="zoom-track-empty timeline-track-empty h-10" />
                  )}

                  {showDebug && segment && (
                    <ZoomDebugOverlay segment={segment} duration={duration} />
                  )}

                  {segment ? (
                    <SpeedTrack
                      segment={segment}
                      duration={duration}
                      onUpdateSpeedPoints={(points) => {
                        setSegment({ ...segment, speedPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="speed-track-empty timeline-track-empty h-10" />
                  )}

                  {showDeviceAudio && (segment ? (
                    <DeviceAudioTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isDeviceAudioAvailable}
                      onUpdateDeviceAudioPoints={(points) => {
                        setSegment({ ...segment, deviceAudioPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="device-audio-track-empty timeline-track-empty h-10" />
                  ))}

                  {showMicAudio && (segment ? (
                    <MicTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isMicAudioAvailable}
                      onUpdateMicAudioPoints={(points) => {
                        setSegment({ ...segment, micAudioPoints: points });
                      }}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  ) : (
                    <div className="mic-audio-track-empty timeline-track-empty h-10" />
                  ))}

                  {showWebcam && (segment ? (
                    <WebcamVisibilityTrack
                      segment={segment}
                      duration={duration}
                      isAvailable={isWebcamAvailable}
                      onWebcamClick={handleWebcamClick}
                      onHandleDragStart={handleWebcamDragStart}
                      onAddWebcamSegment={(atTime) => {
                        if (!segment || !isWebcamAvailable || typeof atTime !== "number") return;
                        const segDur = Math.min(2, Math.max(0.3, duration * 0.08));
                        let startTime = Math.max(0, atTime - segDur / 2);
                        let endTime = Math.min(duration, startTime + segDur);
                        if (endTime - startTime < 0.1) {
                          startTime = Math.max(0, endTime - 0.1);
                        }
                        beginBatch();
                        setSegment({
                          ...segment,
                          webcamVisibilitySegments: clampVisibilitySegmentsToDuration(
                            mergePointerSegments([
                              ...(segment.webcamVisibilitySegments ?? []),
                              {
                                id: crypto.randomUUID(),
                                startTime,
                                endTime,
                              },
                            ]),
                            duration,
                          ),
                        });
                        commitBatch();
                      }}
                      onDeleteWebcamSegments={handleDeleteWebcamSegments}
                      onSelectionChange={onWebcamSelectionChange}
                      clearSignal={clearSelectionSignal}
                    />
                  ) : (
                    <div className="webcam-visibility-track-empty timeline-track-empty h-7" />
                  ))}

                  {segment ? (
                    <TextTrack
                      segment={segment}
                      duration={duration}
                      editingTextId={editingTextId}
                      onTextClick={handleTextClick}
                      onTextSplit={handleTextSplit}
                      onHandleDragStart={handleTextDragStart}
                      onAddText={onAddText}
                      onDeleteTextSegments={handleDeleteTextSegments}
                      onSelectionChange={onTextSelectionChange}
                      clearSignal={clearSelectionSignal}
                    />
                  ) : (
                    <div className="text-track-empty timeline-track-empty h-7" />
                  )}

                  {showKeystroke && (segment ? (
                    <KeystrokeTrack
                      segment={segment}
                      duration={duration}
                      editingKeystrokeSegmentId={editingKeystrokeSegmentId}
                      onKeystrokeClick={handleKeystrokeClick}
                      onHandleDragStart={handleKeystrokeDragStart}
                      onAddKeystrokeSegment={onAddKeystrokeSegment}
                      onKeystrokeHover={setEditingKeystrokeSegmentId}
                      onDeleteKeystrokeSegments={handleDeleteKeystrokeSegments}
                      onSelectionChange={onKeystrokeSelectionChange}
                      clearSignal={clearSelectionSignal}
                    />
                  ) : (
                    <div className="keystroke-track-empty timeline-track-empty h-7" />
                  ))}

                  {hasMouseData && (segment ? (
                    <PointerTrack
                      segment={segment}
                      duration={duration}
                      onPointerClick={handlePointerClick}
                      onHandleDragStart={handlePointerDragStart}
                      onAddPointerSegment={onAddPointerSegment}
                      onPointerHover={setEditingPointerId}
                      onDeletePointerSegments={handleDeletePointerSegments}
                      onSelectionChange={onPointerSelectionChange}
                      clearSignal={clearSelectionSignal}
                    />
                  ) : (
                    <div className="pointer-track-empty timeline-track-empty h-7" />
                  ))}

                  {segment ? (
                    <TrimTrack
                      segment={segment}
                      duration={duration}
                      thumbnails={thumbnails}
                      onTrimDragStart={handleTrimDragStart}
                      onTrimSplit={handleTrimSplit}
                      onTrimAddSegment={handleTrimAddSegment}
                      isDraggingTrim={
                        dragState.isDraggingTrimStart || dragState.isDraggingTrimEnd
                      }
                      isSeeking={dragState.isDraggingSeek}
                    />
                  ) : (
                    <div className="trim-track-empty-shell relative h-14">
                      <div className="trim-track-empty timeline-track-empty h-10" />
                    </div>
                  )}
                </div>

                {segment && (
                  <Playhead
                    currentTime={currentTime}
                    duration={duration}
                    isPlaying={!!isPlaying}
                    videoRef={videoRef}
                    segment={segment}
                    headCenterY={trimHeadCenterY}
                    lineBottomY={trimLaneBottomY}
                  />
                )}
              </div>

              <div className="timeline-ruler relative h-4 mt-0.5 select-none">
                {!showEmptyRuler &&
                  rulerTicks.map((tick, index) => (
                    <div
                      key={`${tick.time}-${index}`}
                      className="timeline-tick absolute flex flex-col items-center"
                      style={{
                        left: `${tick.leftPct}%`,
                        transform: "translateX(-50%)",
                        top: 0,
                        contentVisibility: "auto",
                        containIntrinsicSize: "auto 16px",
                      } as React.CSSProperties}
                    >
                      <div className="timeline-tick-mark h-1.5 w-px bg-[var(--outline)]/40" />
                      <span className="timeline-tick-label mt-0.5 text-[9px] font-mono text-[var(--outline)] leading-none">
                        {tick.label}
                      </span>
                    </div>
                  ))}
                {showEmptyRuler && (
                  <div
                    className="timeline-ruler-empty absolute inset-0 flex items-start justify-between px-[2%] opacity-65"
                    aria-hidden="true"
                  >
                    {Array.from({ length: 6 }).map((_, index) => (
                      <div
                        key={index}
                        className="timeline-ruler-empty-tick flex flex-col items-center"
                      >
                        <div className="h-1.5 w-px bg-[var(--outline)]/18" />
                        <div className="mt-0.5 h-2.5 w-7 rounded-full bg-[var(--ui-surface-2)]" />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
          <div
            className="timeline-scrollbar-shell mt-1"
            data-visible={showScrollbar ? "true" : "false"}
          >
            <div
              ref={scrollbarTrackRef}
              className="timeline-scrollbar-track"
              onPointerDown={handleScrollbarTrackPointerDown}
            >
              <div
                ref={scrollbarThumbRef}
                className="timeline-scrollbar-thumb"
                onPointerDown={handleScrollbarThumbPointerDown}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
