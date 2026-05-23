import { useSyncExternalStore } from "react";
import type { NarrationSegment } from "@/types/video";

interface LiveNarrationState {
  segments: NarrationSegment[];
  hiddenSourceSubtitleIds: Set<string>;
}

const EMPTY_STATE: LiveNarrationState = {
  segments: [],
  hiddenSourceSubtitleIds: new Set(),
};

const states = new Map<string, LiveNarrationState>();
const listeners = new Set<() => void>();
let emitScheduled = false;

function getNarrationSourceIds(segment: NarrationSegment): string[] {
  if (segment.sourceSubtitleIds?.length) return segment.sourceSubtitleIds;
  return segment.sourceSubtitleId ? [segment.sourceSubtitleId] : [];
}

function emit() {
  listeners.forEach((listener) => listener());
}

function scheduleEmit() {
  if (emitScheduled) return;
  emitScheduled = true;
  const flush = () => {
    emitScheduled = false;
    emit();
  };
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(flush);
    return;
  }
  globalThis.setTimeout(flush, 0);
}

function getState(projectId: string | null | undefined): LiveNarrationState {
  if (!projectId) return EMPTY_STATE;
  return states.get(projectId) ?? EMPTY_STATE;
}

export function applyLiveNarrationSegments(
  projectId: string | null | undefined,
  segments: NarrationSegment[],
  replaceSubtitleIds: string[],
) {
  if (!projectId) return;
  const previous = getState(projectId);
  const hiddenSourceSubtitleIds = new Set(previous.hiddenSourceSubtitleIds);
  replaceSubtitleIds.forEach((id) => hiddenSourceSubtitleIds.add(id));
  const replaceSet = new Set(replaceSubtitleIds);
  const incomingIds = new Set(segments.map((segment) => segment.id));
  const nextSegments = [
    ...previous.segments.filter((segment) => {
      if (incomingIds.has(segment.id)) return false;
      if (getNarrationSourceIds(segment).some((id) => replaceSet.has(id))) return false;
      return true;
    }),
    ...segments,
  ].sort((left, right) => left.startTime - right.startTime);

  states.set(projectId, { segments: nextSegments, hiddenSourceSubtitleIds });
  scheduleEmit();
}

export function clearLiveNarrationSegments(projectId: string | null | undefined) {
  if (!projectId || !states.has(projectId)) return;
  states.delete(projectId);
  scheduleEmit();
}

export function mergeLiveNarrationSegments(
  baseSegments: NarrationSegment[] | undefined,
  liveState: LiveNarrationState,
): NarrationSegment[] {
  const liveSegments = liveState.segments;
  if (liveSegments.length === 0 && liveState.hiddenSourceSubtitleIds.size === 0) {
    return baseSegments ?? [];
  }
  const liveIds = new Set(liveSegments.map((segment) => segment.id));
  const liveSourceIds = new Set(
    liveSegments
      .flatMap(getNarrationSourceIds),
  );
  return [
    ...(baseSegments ?? []).filter((segment) => {
      if (liveIds.has(segment.id)) return false;
      if (getNarrationSourceIds(segment).some((id) =>
        liveSourceIds.has(id) || liveState.hiddenSourceSubtitleIds.has(id),
      )) {
        return false;
      }
      return true;
    }),
    ...liveSegments,
  ].sort((left, right) => left.startTime - right.startTime);
}

export function useLiveNarrationState(projectId: string | null | undefined) {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => getState(projectId),
    () => EMPTY_STATE,
  );
}
