import type { RefObject } from "react";
import type {
  AudioDownloadTrackKind,
  AudioGainPoint,
  ImportedAudioSegment,
  NarrationSegment,
  VideoSegment,
} from "@/types/video";
import type { SubtitleGenerationIndicator } from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import type { ActivePanel } from "@/components/sidepanel";

export interface TimelineAreaProps {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  thumbnails: string[];
  timelineRef: RefObject<HTMLDivElement>;
  videoRef: RefObject<HTMLVideoElement>;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  setActivePanel: (panel: ActivePanel) => void;
  setSegment: (segment: VideoSegment | null) => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  onClearTimelineFocus?: () => void;
  onAddText?: (atTime?: number) => void;
  onAddSubtitle?: (atTime?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  isPlaying?: boolean;
  onViewportZoomChange?: (zoom: number) => void;
  onViewportCanvasWidthChange?: (widthPx: number) => void;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  beginBatch: () => void;
  commitBatch: () => void;
  selectedTextIds: string[];
  selectedSubtitleIds: string[];
  onTextSelectionChange?: (ids: string[]) => void;
  onSubtitleSelectionChange?: (ids: string[]) => void;
  onSubtitleRangeChange?: (range: TrackSelectionRange | null) => void;
  onPointerSelectionChange?: (ids: string[]) => void;
  onKeystrokeSelectionChange?: (ids: string[]) => void;
  onWebcamSelectionChange?: (ids: string[]) => void;
  clearSelectionSignal?: number;
  hasMouseData?: boolean;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  subtitleTranslationChunkPreview?: {
    groups: Record<string, number>;
    groupCount: number;
  } | null;
  audioSegments?: ImportedAudioSegment[];
  onPickImportedAudioFile?: (file: File) => void;
  onPickSubtitleFile?: (file: File) => void;
  onPickSubtitleSrtFile?: (file: File) => void;
  onAudioSegmentClick?: (id: string) => void;
  onUpdateAudioSegment?: (id: string, patch: Partial<ImportedAudioSegment>) => void;
  onDeleteAudioSegments?: (ids: string[]) => void;
  onCommitAudioSegments?: () => void;
  selectedAudioSegmentIds?: ReadonlySet<string>;
  selectedAudioSegmentRange?: TrackSelectionRange | null;
  onAudioSelectionChange?: (ids: string[]) => void;
  onAudioRangeChange?: (range: TrackSelectionRange | null) => void;
  audioTrackVolumePoints?: AudioGainPoint[];
  onUpdateAudioTrackVolumePoints?: (points: AudioGainPoint[]) => void;
  narrationSegments?: NarrationSegment[];
  liveNarrationProjectId?: string | null;
  onNarrationSegmentClick?: (id: string) => void;
  onUpdateNarrationSegment?: (id: string, patch: Partial<NarrationSegment>) => void;
  onDeleteNarrationSegments?: (ids: string[]) => void;
  onCommitNarrationSegments?: () => void;
  selectedNarrationSegmentIds?: ReadonlySet<string>;
  selectedNarrationSegmentRange?: TrackSelectionRange | null;
  onNarrationSelectionChange?: (ids: string[]) => void;
  onNarrationRangeChange?: (range: TrackSelectionRange | null) => void;
  narrationTrackVolumePoints?: AudioGainPoint[];
  onUpdateNarrationTrackVolumePoints?: (points: AudioGainPoint[]) => void;
  onAudioTrackDownload?: (trackKind: AudioDownloadTrackKind, trackLabel: string) => void;
}
