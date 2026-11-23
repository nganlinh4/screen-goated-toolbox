import type React from "react";
import { isDeleteKey } from "./timelineDeleteKeys";

/**
 * Keyboard access for timeline items (segments, their edge handles and curve
 * control points). Items carry TIMELINE_KEY_OWNER so the window-level editor
 * shortcuts leave their arrow and Delete keys alone while they have focus.
 */
export const TIMELINE_KEY_OWNER = "data-timeline-keys";

/** Arrow steps in seconds; Shift takes the coarse step. */
export const TIMELINE_NUDGE_SEC = 0.1;
export const TIMELINE_COARSE_NUDGE_SEC = 1;

export function isTimelineKeyOwner(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(`[${TIMELINE_KEY_OWNER}]`) !== null;
}

/** Signed seconds for a horizontal arrow key, or null for any other key. */
export function horizontalNudgeSec(event: Pick<KeyboardEvent, "key" | "shiftKey">): number | null {
  const step = event.shiftKey ? TIMELINE_COARSE_NUDGE_SEC : TIMELINE_NUDGE_SEC;
  if (event.key === "ArrowLeft") return -step;
  if (event.key === "ArrowRight") return step;
  return null;
}

/** +1 / -1 for a vertical arrow key, or null for any other key. */
export function verticalNudgeDirection(event: Pick<KeyboardEvent, "key">): 1 | -1 | null {
  if (event.key === "ArrowUp") return 1;
  if (event.key === "ArrowDown") return -1;
  return null;
}

export function formatTimelineSeconds(seconds: number): string {
  return `${Math.max(0, seconds).toFixed(2)} s`;
}

/**
 * Pointer presses keep their old focus behaviour (focus leaves to the page), so a
 * mouse user's arrow keys still seek and Delete still uses the editor selection.
 * Only keyboard focus (Tab) makes an item own its keys.
 */
function keepPointerFocusBehaviour(event: React.MouseEvent) {
  event.preventDefault();
  const active = document.activeElement;
  if (active instanceof HTMLElement && active !== document.body) active.blur();
}

/** A plain letter press (no Ctrl/Cmd/Alt), so editor shortcuts such as Ctrl+S stay untouched. */
function isPlainLetter(event: React.KeyboardEvent, letter: string) {
  return event.key.toLowerCase() === letter && !event.ctrlKey && !event.metaKey && !event.altKey;
}

/** Claims a handled key: no browser default, and no window-level shortcut sees it. */
function claimKey(event: React.KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();
}

export interface TimelineSegmentKeyActions {
  /** Enter: select the segment (same as a click). */
  onActivate?: () => void;
  /** Left/Right (Shift for coarse): move the whole segment by the given seconds. */
  onNudge?: (deltaSec: number) => void;
  /** Delete/Backspace: remove the segment. */
  onDelete?: () => void;
  /** S: split the segment at the playhead. */
  onSplit?: () => void;
  /** A: add a new segment to the lane at the playhead. */
  onAdd?: () => void;
}

const SEGMENT_SHORTCUTS = "Enter ArrowLeft ArrowRight Shift+ArrowLeft Shift+ArrowRight Delete S A";

/** Focus, role and keys for a timeline segment body. */
export function timelineSegmentKeyProps(label: string, actions: TimelineSegmentKeyActions) {
  return {
    tabIndex: 0,
    role: "button",
    "aria-label": label,
    "aria-keyshortcuts": SEGMENT_SHORTCUTS,
    [TIMELINE_KEY_OWNER]: "",
    onMouseDown: keepPointerFocusBehaviour,
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
      if (event.target !== event.currentTarget) return;
      const delta = horizontalNudgeSec(event);
      if (delta !== null && actions.onNudge) {
        claimKey(event);
        actions.onNudge(delta);
      } else if (event.key === "Enter" && actions.onActivate) {
        claimKey(event);
        actions.onActivate();
      } else if (isDeleteKey(event.nativeEvent) && actions.onDelete) {
        claimKey(event);
        actions.onDelete();
      } else if (isPlainLetter(event, "s") && actions.onSplit) {
        claimKey(event);
        actions.onSplit();
      } else if (isPlainLetter(event, "a") && actions.onAdd) {
        claimKey(event);
        actions.onAdd();
      }
    },
  } as const;
}

export interface TimelineEdgeHandle {
  label: string;
  timeSec: number;
  minSec: number;
  maxSec: number;
  onNudge: (deltaSec: number) => void;
  /** A: add a new segment to the lane at the playhead (lanes whose only focusable items are handles). */
  onAdd?: () => void;
}

/** Focus, slider semantics and arrow keys for a segment's start or end handle. */
export function timelineEdgeHandleKeyProps({ label, timeSec, minSec, maxSec, onNudge, onAdd }: TimelineEdgeHandle) {
  return {
    tabIndex: 0,
    role: "slider",
    "aria-label": label,
    "aria-orientation": "horizontal",
    "aria-valuemin": Number(minSec.toFixed(2)),
    "aria-valuemax": Number(maxSec.toFixed(2)),
    "aria-valuenow": Number(timeSec.toFixed(2)),
    "aria-valuetext": formatTimelineSeconds(timeSec),
    [TIMELINE_KEY_OWNER]: "",
    onMouseDown: keepPointerFocusBehaviour,
    "aria-keyshortcuts": onAdd ? "ArrowLeft ArrowRight Shift+ArrowLeft Shift+ArrowRight A" : undefined,
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
      const delta = horizontalNudgeSec(event);
      if (delta !== null) {
        claimKey(event);
        onNudge(delta);
      } else if (onAdd && isPlainLetter(event, "a")) {
        claimKey(event);
        onAdd();
      }
    },
  } as const;
}

export interface TimelinePointKeyActions {
  /** Left/Right: move the point in time (endpoints stay pinned). */
  onNudgeTime: (deltaSec: number) => void;
  /** Up/Down (Shift for coarse): change the point's value. */
  onNudgeValue: (direction: 1 | -1, coarse: boolean) => void;
  /** Delete/Backspace: remove the point; absent for the pinned endpoints. */
  onDelete?: () => void;
  /** Enter: add a point between this one and the next. */
  onAdd?: () => void;
}

const POINT_SHORTCUTS = "ArrowLeft ArrowRight ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown Delete Enter";

/** Focus, slider semantics and keys for a curve control point. */
export function timelinePointKeyProps(
  label: string,
  value: { now: number; min: number; max: number; text: string },
  actions: TimelinePointKeyActions,
) {
  return {
    tabIndex: 0,
    role: "slider",
    "aria-label": label,
    "aria-orientation": "vertical",
    "aria-valuemin": value.min,
    "aria-valuemax": value.max,
    "aria-valuenow": Number(value.now.toFixed(3)),
    "aria-valuetext": value.text,
    "aria-keyshortcuts": POINT_SHORTCUTS,
    [TIMELINE_KEY_OWNER]: "",
    onMouseDown: keepPointerFocusBehaviour,
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => {
      const timeDelta = horizontalNudgeSec(event);
      const direction = verticalNudgeDirection(event);
      if (timeDelta !== null) {
        claimKey(event);
        actions.onNudgeTime(timeDelta);
      } else if (direction !== null) {
        claimKey(event);
        actions.onNudgeValue(direction, event.shiftKey);
      } else if (isDeleteKey(event.nativeEvent)) {
        // Endpoints are pinned: still claim the key so no other item is deleted.
        claimKey(event);
        actions.onDelete?.();
      } else if (event.key === "Enter" && actions.onAdd) {
        claimKey(event);
        actions.onAdd();
      }
    },
  } as const;
}

/** Shortest piece a keyboard split at the playhead may leave on either side. */
const MIN_SPLIT_PIECE_SEC = 0.15;

export interface TimelineRangeKeyOptions {
  /** Spoken name, for example: Subtitle "Hello". */
  name: string;
  startLabel: string;
  endLabel: string;
  range: { startTime: number; endTime: number };
  duration: number;
  /** Moves the body or one edge by the given seconds; absent when the lane cannot move it. */
  nudge?: (edge: "start" | "end" | "body", deltaSec: number) => void;
  onActivate?: () => void;
  onDelete?: () => void;
  onSplit?: (time: number) => void;
  /** Adds a new segment to the lane at the given time (the playhead). */
  onAdd?: (time: number) => void;
  /** Also lets the edge handles add (for lanes such as trim with no focusable body). */
  addFromEdges?: boolean;
  getPlayheadTime?: () => number;
}

/** Body, start-handle and end-handle keyboard props for one timeline range segment. */
export function timelineRangeKeyProps(options: TimelineRangeKeyOptions) {
  const { name, range, duration, nudge, onSplit, onAdd, getPlayheadTime } = options;
  const span = `${formatTimelineSeconds(range.startTime)} - ${formatTimelineSeconds(range.endTime)}`;
  const splitAtPlayhead = onSplit && getPlayheadTime
    ? () => {
        const time = getPlayheadTime();
        if (time > range.startTime + MIN_SPLIT_PIECE_SEC && time < range.endTime - MIN_SPLIT_PIECE_SEC) onSplit(time);
      }
    : undefined;
  const addAtPlayhead = onAdd && getPlayheadTime ? () => onAdd(getPlayheadTime()) : undefined;
  const edge = (which: "start" | "end") => nudge && timelineEdgeHandleKeyProps({
    label: `${name} ${which === "start" ? options.startLabel : options.endLabel}`,
    timeSec: which === "start" ? range.startTime : range.endTime,
    minSec: which === "start" ? 0 : range.startTime,
    maxSec: which === "start" ? range.endTime : duration,
    onNudge: (deltaSec) => nudge(which, deltaSec),
    onAdd: options.addFromEdges ? addAtPlayhead : undefined,
  });
  return {
    body: timelineSegmentKeyProps(`${name}, ${span}`, {
      onActivate: options.onActivate,
      onNudge: nudge && ((deltaSec) => nudge("body", deltaSec)),
      onDelete: options.onDelete,
      onSplit: splitAtPlayhead,
      onAdd: addAtPlayhead,
    }),
    start: edge("start"),
    end: edge("end"),
  };
}
