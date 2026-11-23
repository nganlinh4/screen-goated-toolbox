import type { VideoSegment } from '@/types/video';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import { getActiveTimedSegments } from '@/lib/timedSegmentIndex';
import { getTextHitArea } from './overlayTextRenderer';

export interface TextDragState {
  isDraggingText: boolean;
  draggedTextId: string | null;
  draggedOverlayKind: 'text' | 'subtitle' | null;
  dragStartPointer: { x: number; y: number };
  dragTargets: Array<{
    kind: 'text' | 'subtitle';
    id: string;
    startX: number;
    startY: number;
  }>;
}

export interface OverlayDragSelection {
  selectedTextIds?: readonly string[];
  selectedSubtitleIds?: readonly string[];
}

export interface OverlayDragHit {
  kind: 'text' | 'subtitle';
  id: string;
}

export interface OverlayDragMove {
  kind: 'text' | 'subtitle';
  id: string;
  x: number;
  y: number;
}

// ---------------------------------------------------------------------------
// Text drag handlers
// ---------------------------------------------------------------------------

export function handleMouseDown(
  e: MouseEvent,
  segment: VideoSegment,
  canvas: HTMLCanvasElement,
  dragState: TextDragState,
  selection?: OverlayDragSelection,
  currentTime?: number,
): OverlayDragHit | null {
  const rect = canvas.getBoundingClientRect();
  const x = (e.clientX - rect.left) * (canvas.width / rect.width);
  const y = (e.clientY - rect.top) * (canvas.height / rect.height);
  // Only overlays on screen are hittable, filtered exactly as drawFrame draws
  // them. getVisibleSubtitleSegments is the active track, not the active time,
  // so unfiltered hit-testing grabbed hidden captions at the default position.
  const onScreen = <T extends { startTime: number; endTime: number }>(items: readonly T[]) =>
    currentTime !== undefined ? getActiveTimedSegments(items, currentTime) : [...items];
  const visibleTexts = onScreen(segment.textSegments ?? []);
  const overlays = [
    ...onScreen(getVisibleSubtitleSegments(segment)).map((subtitle) => ({
      kind: 'subtitle' as const,
      segment: subtitle,
    })),
    ...visibleTexts.map((text) => ({
      kind: 'text' as const,
      segment: text,
    })),
  ];

  for (let index = overlays.length - 1; index >= 0; index -= 1) {
    const overlay = overlays[index];
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const hitArea = getTextHitArea(ctx, overlay.segment, canvas.width, canvas.height);
    if (x >= hitArea.x && x <= hitArea.x + hitArea.width &&
      y >= hitArea.y && y <= hitArea.y + hitArea.height) {
      const selectionIds = overlay.kind === 'text'
        ? selection?.selectedTextIds
        : selection?.selectedSubtitleIds;
      const targetIds = selectionIds?.includes(overlay.segment.id)
        ? selectionIds
        : [overlay.segment.id];
      const targetSet = new Set(targetIds);
      const targetSource = overlay.kind === 'text'
        ? (segment.textSegments ?? [])
        : getVisibleSubtitleSegments(segment);
      dragState.isDraggingText = true;
      dragState.draggedTextId = overlay.segment.id;
      dragState.draggedOverlayKind = overlay.kind;
      dragState.dragStartPointer = { x, y };
      dragState.dragTargets = targetSource
        .filter((item) => targetSet.has(item.id))
        .map((item) => ({
          kind: overlay.kind,
          id: item.id,
          startX: item.style.x,
          startY: item.style.y,
        }));
      return {
        kind: overlay.kind,
        id: overlay.segment.id,
      };
    }
  }
  return null;
}

export function handleMouseMove(
  e: MouseEvent,
  _segment: VideoSegment,
  canvas: HTMLCanvasElement,
  onTextMove: (moves: OverlayDragMove[]) => void,
  dragState: TextDragState
): void {
  if (!dragState.isDraggingText || dragState.dragTargets.length === 0) return;

  const rect = canvas.getBoundingClientRect();
  const x = (e.clientX - rect.left) * (canvas.width / rect.width);
  const y = (e.clientY - rect.top) * (canvas.height / rect.height);
  const dx = ((x - dragState.dragStartPointer.x) / canvas.width) * 100;
  const dy = ((y - dragState.dragStartPointer.y) / canvas.height) * 100;

  onTextMove(
    dragState.dragTargets.map((target) => ({
      kind: target.kind,
      id: target.id,
      x: Math.max(0, Math.min(100, target.startX + dx)),
      y: Math.max(0, Math.min(100, target.startY + dy)),
    })),
  );
}

export function handleMouseUp(dragState: TextDragState): void {
  dragState.isDraggingText = false;
  dragState.draggedTextId = null;
  dragState.draggedOverlayKind = null;
  dragState.dragStartPointer = { x: 0, y: 0 };
  dragState.dragTargets = [];
}
