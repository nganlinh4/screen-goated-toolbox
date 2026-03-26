import { VideoSegment } from '@/types/video';

export interface TimelineDragState {
  isDraggingTrimStart: boolean;
  isDraggingTrimEnd: boolean;
  isDraggingTextStart: boolean;
  isDraggingTextEnd: boolean;
  isDraggingTextBody: boolean;
  isDraggingKeystrokeStart: boolean;
  isDraggingKeystrokeEnd: boolean;
  isDraggingKeystrokeBody: boolean;
  isDraggingPointerStart: boolean;
  isDraggingPointerEnd: boolean;
  isDraggingPointerBody: boolean;
  isDraggingWebcamStart: boolean;
  isDraggingWebcamEnd: boolean;
  isDraggingWebcamBody: boolean;
  isDraggingZoom: boolean;
  isDraggingSeek: boolean;
  draggingTextId: string | null;
  draggingKeystrokeId: string | null;
  draggingPointerId: string | null;
  draggingWebcamId: string | null;
  draggingZoomIdx: number | null;
}

export interface UseTimelineDragOptions {
  duration: number;
  segment: VideoSegment | null;
  timelineRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  setCurrentTime: (time: number) => void;
  setSegment: (segment: VideoSegment | null) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingKeystrokeId?: (id: string | null) => void;
  setEditingPointerId?: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC = 0.2;
