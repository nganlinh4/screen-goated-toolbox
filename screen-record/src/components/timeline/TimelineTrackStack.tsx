import type { ComponentProps, RefObject } from "react";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  SubtitleSourceGroup,
  VideoSegment,
} from "@/types/video";
import type { SubtitleGenerationIndicator } from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import {
  clampVisibilitySegmentsToDuration,
  mergePointerSegments,
} from "@/lib/cursorHiding";
import type { TimelineVisibleRange } from "./SegmentBlocksCanvas";
import { DeviceAudioTrack } from "./DeviceAudioTrack";
import { ImportedAudioTrack } from "./ImportedAudioTrack";
import { KeystrokeTrack } from "./KeystrokeTrack";
import { MicTrack } from "./MicTrack";
import { NarrationTrack } from "./NarrationTrack";
import { Playhead } from "./Playhead";
import { PointerTrack } from "./PointerTrack";
import { SpeedTrack } from "./SpeedTrack";
import { SubtitleTrack } from "./SubtitleTrack";
import { TextTrack } from "./TextTrack";
import type { TimelineDragState } from "./useTimelineDrag";
import { TrimTrack } from "./TrimTrack";
import { WebcamVisibilityTrack } from "./WebcamVisibilityTrack";
import { ZoomDebugOverlay } from "./ZoomDebugOverlay";
import { ZoomTrack } from "./ZoomTrack";

interface TimelineTrackStackProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  duration: number;
  currentTime: number;
  thumbnails: string[];
  videoRef: RefObject<HTMLVideoElement>;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  editingKeystrokeSegmentId: string | null;
  showZoom: boolean;
  showDebug: boolean;
  showSpeed: boolean;
  showImportedAudio: boolean;
  showDeviceAudio: boolean;
  showMicAudio: boolean;
  showWebcam: boolean;
  showNarration: boolean;
  showKeystroke: boolean;
  showPointer: boolean;
  showTrimLane: boolean;
  volumeViewEnabled: boolean;
  isPlaying?: boolean;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  audioSegments?: ImportedAudioSegment[];
  narrationSegments?: NarrationSegment[];
  liveNarrationProjectId?: string | null;
  selectedAudioSegmentIds?: ReadonlySet<string>;
  selectedAudioSegmentRange?: TrackSelectionRange | null;
  selectedNarrationSegmentIds?: ReadonlySet<string>;
  selectedNarrationSegmentRange?: TrackSelectionRange | null;
  clearSelectionSignal?: number;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  subtitleTranslationChunkPreview?: {
    groups: Record<string, number>;
    groupCount: number;
  } | null;
  audioTrackVolumePoints?: ComponentProps<typeof ImportedAudioTrack>["volumePoints"];
  narrationTrackVolumePoints?: ComponentProps<typeof NarrationTrack>["volumePoints"];
  canvasWidthPx: number;
  visibleTimeRange: TimelineVisibleRange | null;
  playheadHeadCenterY: number;
  playheadLineBottomY: number;
  dragState: TimelineDragState;
  beginBatch: () => void;
  commitBatch: () => void;
  onKeyframeClick: ComponentProps<typeof ZoomTrack>["onKeyframeClick"];
  onZoomDragStart: ComponentProps<typeof ZoomTrack>["onKeyframeDragStart"];
  onAudioSegmentClick?: ComponentProps<typeof ImportedAudioTrack>["onSegmentClick"];
  onUpdateAudioSegment?: ComponentProps<typeof ImportedAudioTrack>["onUpdateSegment"];
  onDeleteAudioSegments?: ComponentProps<typeof ImportedAudioTrack>["onDeleteSegments"];
  onCommitAudioSegments?: ComponentProps<typeof ImportedAudioTrack>["onCommitSegments"];
  onAudioSelectionChange?: ComponentProps<typeof ImportedAudioTrack>["onSelectionChange"];
  onAudioRangeChange?: ComponentProps<typeof ImportedAudioTrack>["onRangeChange"];
  onUpdateAudioTrackVolumePoints?: ComponentProps<typeof ImportedAudioTrack>["onUpdateVolumePoints"];
  onWebcamClick: ComponentProps<typeof WebcamVisibilityTrack>["onWebcamClick"];
  onWebcamDragStart: ComponentProps<typeof WebcamVisibilityTrack>["onHandleDragStart"];
  onDeleteWebcamSegments: ComponentProps<typeof WebcamVisibilityTrack>["onDeleteWebcamSegments"];
  onWebcamSelectionChange?: ComponentProps<typeof WebcamVisibilityTrack>["onSelectionChange"];
  onNarrationSegmentClick?: ComponentProps<typeof NarrationTrack>["onSegmentClick"];
  onUpdateNarrationSegment?: ComponentProps<typeof NarrationTrack>["onUpdateSegment"];
  onDeleteNarrationSegments?: ComponentProps<typeof NarrationTrack>["onDeleteSegments"];
  onCommitNarrationSegments?: ComponentProps<typeof NarrationTrack>["onCommitSegments"];
  onNarrationSelectionChange?: ComponentProps<typeof NarrationTrack>["onSelectionChange"];
  onNarrationRangeChange?: ComponentProps<typeof NarrationTrack>["onRangeChange"];
  onUpdateNarrationTrackVolumePoints?: ComponentProps<typeof NarrationTrack>["onUpdateVolumePoints"];
  onSubtitleClick: ComponentProps<typeof SubtitleTrack>["onSubtitleClick"];
  onSubtitleSplit: ComponentProps<typeof SubtitleTrack>["onSubtitleSplit"];
  onSubtitleDuplicate: ComponentProps<typeof SubtitleTrack>["onSubtitleDuplicate"];
  onSubtitleDragStart: ComponentProps<typeof SubtitleTrack>["onHandleDragStart"];
  onAddSubtitle?: ComponentProps<typeof SubtitleTrack>["onAddSubtitle"];
  onDeleteSubtitleSegments: ComponentProps<typeof SubtitleTrack>["onDeleteSubtitleSegments"];
  onSubtitleSelectionChange?: ComponentProps<typeof SubtitleTrack>["onSelectionChange"];
  onSubtitleRangeChange?: ComponentProps<typeof SubtitleTrack>["onRangeChange"];
  onAssignSubtitleSourceGroup: (ids: string[], sourceGroup: SubtitleSourceGroup) => void;
  onTextClick: ComponentProps<typeof TextTrack>["onTextClick"];
  onTextSplit: ComponentProps<typeof TextTrack>["onTextSplit"];
  onTextDuplicate: ComponentProps<typeof TextTrack>["onTextDuplicate"];
  onTextDragStart: ComponentProps<typeof TextTrack>["onHandleDragStart"];
  onAddText?: ComponentProps<typeof TextTrack>["onAddText"];
  onDeleteTextSegments: ComponentProps<typeof TextTrack>["onDeleteTextSegments"];
  onTextSelectionChange?: ComponentProps<typeof TextTrack>["onSelectionChange"];
  onKeystrokeClick: ComponentProps<typeof KeystrokeTrack>["onKeystrokeClick"];
  onKeystrokeDragStart: ComponentProps<typeof KeystrokeTrack>["onHandleDragStart"];
  onAddKeystrokeSegment?: ComponentProps<typeof KeystrokeTrack>["onAddKeystrokeSegment"];
  onKeystrokeHover: ComponentProps<typeof KeystrokeTrack>["onKeystrokeHover"];
  onDeleteKeystrokeSegments: ComponentProps<typeof KeystrokeTrack>["onDeleteKeystrokeSegments"];
  onKeystrokeSelectionChange?: ComponentProps<typeof KeystrokeTrack>["onSelectionChange"];
  onPointerClick: ComponentProps<typeof PointerTrack>["onPointerClick"];
  onPointerDragStart: ComponentProps<typeof PointerTrack>["onHandleDragStart"];
  onAddPointerSegment?: ComponentProps<typeof PointerTrack>["onAddPointerSegment"];
  onPointerHover: ComponentProps<typeof PointerTrack>["onPointerHover"];
  onDeletePointerSegments: ComponentProps<typeof PointerTrack>["onDeletePointerSegments"];
  onPointerSelectionChange?: ComponentProps<typeof PointerTrack>["onSelectionChange"];
  onTrimDragStart: ComponentProps<typeof TrimTrack>["onTrimDragStart"];
  onTrimSplit: ComponentProps<typeof TrimTrack>["onTrimSplit"];
  onTrimAddSegment: ComponentProps<typeof TrimTrack>["onTrimAddSegment"];
  onEmptyTrackClick: (time: number) => void;
}

export function TimelineTrackStack({
  segment,
  setSegment,
  duration,
  currentTime,
  thumbnails,
  videoRef,
  editingKeyframeId,
  editingTextId,
  editingSubtitleId,
  editingKeystrokeSegmentId,
  showZoom,
  showDebug,
  showSpeed,
  showImportedAudio,
  showDeviceAudio,
  showMicAudio,
  showWebcam,
  showNarration,
  showKeystroke,
  showPointer,
  showTrimLane,
  volumeViewEnabled,
  isPlaying,
  isDeviceAudioAvailable,
  isMicAudioAvailable,
  isWebcamAvailable,
  currentRawVideoPath,
  currentRawMicAudioPath,
  audioSegments,
  narrationSegments,
  liveNarrationProjectId,
  selectedAudioSegmentIds,
  selectedAudioSegmentRange,
  selectedNarrationSegmentIds,
  selectedNarrationSegmentRange,
  clearSelectionSignal,
  subtitleGenerationIndicator,
  subtitleTranslationChunkPreview,
  audioTrackVolumePoints,
  narrationTrackVolumePoints,
  canvasWidthPx,
  visibleTimeRange,
  playheadHeadCenterY,
  playheadLineBottomY,
  dragState,
  beginBatch,
  commitBatch,
  onKeyframeClick,
  onZoomDragStart,
  onAudioSegmentClick,
  onUpdateAudioSegment,
  onDeleteAudioSegments,
  onCommitAudioSegments,
  onAudioSelectionChange,
  onAudioRangeChange,
  onUpdateAudioTrackVolumePoints,
  onWebcamClick,
  onWebcamDragStart,
  onDeleteWebcamSegments,
  onWebcamSelectionChange,
  onNarrationSegmentClick,
  onUpdateNarrationSegment,
  onDeleteNarrationSegments,
  onCommitNarrationSegments,
  onNarrationSelectionChange,
  onNarrationRangeChange,
  onUpdateNarrationTrackVolumePoints,
  onSubtitleClick,
  onSubtitleSplit,
  onSubtitleDuplicate,
  onSubtitleDragStart,
  onAddSubtitle,
  onDeleteSubtitleSegments,
  onSubtitleSelectionChange,
  onSubtitleRangeChange,
  onAssignSubtitleSourceGroup,
  onTextClick,
  onTextSplit,
  onTextDuplicate,
  onTextDragStart,
  onAddText,
  onDeleteTextSegments,
  onTextSelectionChange,
  onKeystrokeClick,
  onKeystrokeDragStart,
  onAddKeystrokeSegment,
  onKeystrokeHover,
  onDeleteKeystrokeSegments,
  onKeystrokeSelectionChange,
  onPointerClick,
  onPointerDragStart,
  onAddPointerSegment,
  onPointerHover,
  onDeletePointerSegments,
  onPointerSelectionChange,
  onTrimDragStart,
  onTrimSplit,
  onTrimAddSegment,
  onEmptyTrackClick,
}: TimelineTrackStackProps) {
  return (
    <>
      <div className="timeline-tracks flex flex-col gap-[2px]">
        {showZoom && (segment ? (
          <ZoomTrack
            segment={segment}
            duration={duration}
            editingKeyframeId={editingKeyframeId}
            onKeyframeClick={onKeyframeClick}
            onKeyframeDragStart={onZoomDragStart}
            onUpdateInfluencePoints={(points) => {
              const nextSegment = {
                ...segment,
                zoomInfluencePoints: points,
              };
              if (points.length === 0) nextSegment.smoothMotionPath = [];
              setSegment(nextSegment);
            }}
            onUpdateBlocks={(zoomBlocks) => {
              setSegment({ ...segment, zoomBlocks });
            }}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
        ) : (
          <div className="zoom-track-empty timeline-track-empty h-7" />
        ))}

        {showZoom && showDebug && segment && (
          <ZoomDebugOverlay segment={segment} duration={duration} />
        )}

        {showSpeed && (segment ? (
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
          <div className="speed-track-empty timeline-track-empty h-7" />
        ))}

        {showImportedAudio && (
          <ImportedAudioTrack
            segments={audioSegments ?? []}
            duration={duration}
            onSegmentClick={onAudioSegmentClick}
            onUpdateSegment={onUpdateAudioSegment}
            onDeleteSegments={onDeleteAudioSegments}
            selectedIds={selectedAudioSegmentIds}
            selectedRange={selectedAudioSegmentRange}
            onSelectionChange={onAudioSelectionChange}
            onRangeChange={onAudioRangeChange}
            viewMode={volumeViewEnabled ? "volume" : "compact"}
            clearSignal={clearSelectionSignal}
            volumePoints={audioTrackVolumePoints}
            onUpdateVolumePoints={onUpdateAudioTrackVolumePoints}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
            onCommitSegments={onCommitAudioSegments}
            onEmptyClick={onEmptyTrackClick}
            canvasWidthPx={canvasWidthPx}
            visibleTimeRange={visibleTimeRange}
          />
        )}

        {showDeviceAudio && (segment ? (
          <DeviceAudioTrack
            segment={segment}
            duration={duration}
            isAvailable={isDeviceAudioAvailable}
            sourcePath={currentRawVideoPath}
            viewMode={volumeViewEnabled ? "volume" : "compact"}
            onUpdateDeviceAudioPoints={(points) => {
              setSegment({ ...segment, deviceAudioPoints: points });
            }}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
        ) : (
          <div className="device-audio-track-empty timeline-track-empty h-7" />
        ))}

        {showMicAudio && (segment ? (
          <MicTrack
            segment={segment}
            duration={duration}
            isAvailable={isMicAudioAvailable}
            sourcePath={currentRawMicAudioPath}
            viewMode={volumeViewEnabled ? "volume" : "compact"}
            onUpdateMicAudioPoints={(points) => {
              setSegment({ ...segment, micAudioPoints: points });
            }}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
          />
        ) : (
          <div className="mic-audio-track-empty timeline-track-empty h-7" />
        ))}

        {showWebcam && (segment ? (
          <WebcamVisibilityTrack
            segment={segment}
            duration={duration}
            isAvailable={isWebcamAvailable}
            onWebcamClick={onWebcamClick}
            onHandleDragStart={onWebcamDragStart}
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
            onDeleteWebcamSegments={onDeleteWebcamSegments}
            onSelectionChange={onWebcamSelectionChange}
            clearSignal={clearSelectionSignal}
            onEmptyClick={onEmptyTrackClick}
          />
        ) : (
          <div className="webcam-visibility-track-empty timeline-track-empty h-7" />
        ))}

        {showNarration && (
          <NarrationTrack
            segments={narrationSegments ?? []}
            liveProjectId={liveNarrationProjectId}
            duration={duration}
            isPlaying={!!isPlaying}
            onSegmentClick={onNarrationSegmentClick}
            onUpdateSegment={onUpdateNarrationSegment}
            onDeleteSegments={onDeleteNarrationSegments}
            selectedIds={selectedNarrationSegmentIds}
            selectedRange={selectedNarrationSegmentRange}
            onSelectionChange={onNarrationSelectionChange}
            onRangeChange={onNarrationRangeChange}
            viewMode={volumeViewEnabled ? "volume" : "compact"}
            clearSignal={clearSelectionSignal}
            volumePoints={narrationTrackVolumePoints}
            onUpdateVolumePoints={onUpdateNarrationTrackVolumePoints}
            beginBatch={beginBatch}
            commitBatch={commitBatch}
            onCommitSegments={onCommitNarrationSegments}
            onEmptyClick={onEmptyTrackClick}
            canvasWidthPx={canvasWidthPx}
            visibleTimeRange={visibleTimeRange}
          />
        )}

        {segment ? (
          <SubtitleTrack
            segment={segment}
            duration={duration}
            editingSubtitleId={editingSubtitleId}
            onSubtitleClick={onSubtitleClick}
            onSubtitleSplit={onSubtitleSplit}
            onSubtitleDuplicate={onSubtitleDuplicate}
            onHandleDragStart={onSubtitleDragStart}
            onAddSubtitle={onAddSubtitle}
            onDeleteSubtitleSegments={onDeleteSubtitleSegments}
            onSelectionChange={onSubtitleSelectionChange}
            onRangeChange={onSubtitleRangeChange}
            clearSignal={clearSelectionSignal}
            generationIndicator={subtitleGenerationIndicator}
            translationChunkPreview={subtitleTranslationChunkPreview}
            audioSegments={audioSegments}
            isDeviceAudioAvailable={isDeviceAudioAvailable}
            isMicAudioAvailable={isMicAudioAvailable}
            onAssignSubtitleSourceGroup={onAssignSubtitleSourceGroup}
            onEmptyClick={onEmptyTrackClick}
            canvasWidthPx={canvasWidthPx}
            visibleTimeRange={visibleTimeRange}
          />
        ) : (
          <div className="subtitle-track-empty timeline-track-empty h-7" />
        )}

        {segment ? (
          <TextTrack
            segment={segment}
            duration={duration}
            editingTextId={editingTextId}
            onTextClick={onTextClick}
            onTextSplit={onTextSplit}
            onTextDuplicate={onTextDuplicate}
            onHandleDragStart={onTextDragStart}
            onAddText={onAddText}
            onDeleteTextSegments={onDeleteTextSegments}
            onSelectionChange={onTextSelectionChange}
            clearSignal={clearSelectionSignal}
            onEmptyClick={onEmptyTrackClick}
            canvasWidthPx={canvasWidthPx}
            visibleTimeRange={visibleTimeRange}
          />
        ) : (
          <div className="text-track-empty timeline-track-empty h-7" />
        )}

        {showKeystroke && (segment ? (
          <KeystrokeTrack
            segment={segment}
            duration={duration}
            editingKeystrokeSegmentId={editingKeystrokeSegmentId}
            onKeystrokeClick={onKeystrokeClick}
            onHandleDragStart={onKeystrokeDragStart}
            onAddKeystrokeSegment={onAddKeystrokeSegment}
            onKeystrokeHover={onKeystrokeHover}
            onDeleteKeystrokeSegments={onDeleteKeystrokeSegments}
            onSelectionChange={onKeystrokeSelectionChange}
            clearSignal={clearSelectionSignal}
            onEmptyClick={onEmptyTrackClick}
          />
        ) : (
          <div className="keystroke-track-empty timeline-track-empty h-7" />
        ))}

        {showPointer && (segment ? (
          <PointerTrack
            segment={segment}
            duration={duration}
            onPointerClick={onPointerClick}
            onHandleDragStart={onPointerDragStart}
            onAddPointerSegment={onAddPointerSegment}
            onPointerHover={onPointerHover}
            onDeletePointerSegments={onDeletePointerSegments}
            onSelectionChange={onPointerSelectionChange}
            clearSignal={clearSelectionSignal}
            onEmptyClick={onEmptyTrackClick}
          />
        ) : (
          <div className="pointer-track-empty timeline-track-empty h-7" />
        ))}

        {showTrimLane && (segment ? (
          <TrimTrack
            segment={segment}
            duration={duration}
            thumbnails={thumbnails}
            onTrimDragStart={onTrimDragStart}
            onTrimSplit={onTrimSplit}
            onTrimAddSegment={onTrimAddSegment}
            isDraggingTrim={
              dragState.isDraggingTrimStart || dragState.isDraggingTrimEnd
            }
            isSeeking={dragState.isDraggingSeek}
          />
        ) : (
          <div className="trim-track-empty-shell relative h-14">
            <div className="trim-track-empty timeline-track-empty h-10" />
          </div>
        ))}
      </div>

      {segment && (
        <Playhead
          currentTime={currentTime}
          duration={duration}
          isPlaying={!!isPlaying}
          videoRef={videoRef}
          segment={segment}
          disableVideoSync={segment.mediaMode === "timelineOnly"}
          headCenterY={playheadHeadCenterY}
          lineBottomY={playheadLineBottomY}
        />
      )}
    </>
  );
}
