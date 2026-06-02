import type { VideoSegment } from '@/types/video';

export interface UseTrackSegmentDragOptions {
  duration: number;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId?: (id: string | null) => void;
  setEditingKeystrokeId?: (id: string | null) => void;
  setEditingPointerId?: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles') => void;
  selectedTextIds: readonly string[];
  selectedSubtitleIds: readonly string[];
  getTimeFromClientX: (clientX: number) => number | null;
  beginBatch: () => void;
  commitBatch: () => void;
}
