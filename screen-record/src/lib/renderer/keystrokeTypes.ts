import type { CursorVisibilitySegment, KeystrokeEvent, VideoSegment } from '@/types/video';
import {
  filterKeystrokeEventsByMode,
  getKeystrokeVisibilitySegmentsForMode
} from '@/lib/keystrokeVisibility';

// --- CONFIGURATION ---
export const DEFAULT_KEYSTROKE_DELAY_SEC = 0;
export const KEYSTROKE_ANIM_ENTER_SEC = 0.18;
export const KEYSTROKE_ANIM_EXIT_SEC = 0.2;
export const KEYSTROKE_SLOT_SPARSE_GAP_LIMIT = 2;
export const DEFAULT_KEYSTROKE_OVERLAY_X = 50;
export const DEFAULT_KEYSTROKE_OVERLAY_Y = 100;
export const DEFAULT_KEYSTROKE_OVERLAY_SCALE = 1;
export const KEYSTROKE_OVERLAY_MIN_SCALE = 0.45;
export const KEYSTROKE_OVERLAY_MAX_SCALE = 2.4;

// --- INTERFACES ---

export interface KeystrokeVisualState {
  alpha: number;
  scale: number;
  scaleX: number;
  scaleY: number;
  translateY: number;
  wdth: number;
  wght: number;
  slnt: number;
  rond: number;
  holdMix: number;
  laneWeight: number;
}

export interface KeystrokeBubbleLayout {
  label: string;
  showMouseIcon: boolean;
  keyIcon: string | null;
  iconBoxWidth: number;
  iconGap: number;
  fontSize: number;
  paddingX: number;
  paddingY: number;
  radius: number;
  marginBottom: number;
  width: number;
  height: number;
}

export interface KeystrokeRenderCache {
  mode: 'off' | 'keyboard' | 'keyboardMouse';
  segmentRef: VideoSegment | null;
  eventsRef: KeystrokeEvent[] | null;
  visibilityRef: CursorVisibilitySegment[] | null;
  duration: number;
  displayEvents: KeystrokeEvent[];
  startTimes: number[];
  effectiveEnds: number[];
  keyboardStartTimes: number[];
  keyboardIndices: number[];
  mouseStartTimes: number[];
  mouseIndices: number[];
  keyboardMaxDuration: number;
  mouseMaxDuration: number;
  eventSlots: number[];
  eventIdentities: string[];
  keyboardSlotRepresentatives: number[];
  mouseSlotRepresentatives: number[];
}

export interface ActiveKeystrokeEvent {
  event: KeystrokeEvent;
  startTime: number;
  endTime: number;
  slot: number;
  identity: string;
}

export interface ActiveKeystrokeLanes {
  keyboard: ActiveKeystrokeEvent[];
  mouse: ActiveKeystrokeEvent[];
}

export interface KeystrokeLaneRenderItem {
  active: ActiveKeystrokeEvent;
  layout: KeystrokeBubbleLayout;
  visual: KeystrokeVisualState;
  bubbleWidth: number;
}

export interface KeystrokeLanePlacement {
  item: KeystrokeLaneRenderItem;
  x: number;
  y: number;
  align: 'left' | 'right';
}

export interface ActiveKeystrokeFrameLayout {
  keyboard: KeystrokeLanePlacement[];
  mouse: KeystrokeLanePlacement[];
}

export interface KeystrokeOverlayTransform {
  anchorXPx: number;
  baselineYPx: number;
  scale: number;
}

export interface KeystrokeOverlayEditBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  handleSize: number;
}

export interface KeystrokeState {
  keystrokeLanguage: string;
  renderCache: KeystrokeRenderCache;
  layoutCache: Map<string, KeystrokeBubbleLayout>;
}

// --- HELPERS ---

export function clamp01(v: number): number {
  return Math.max(0, Math.min(1, v));
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * clamp01(t);
}

export function computePercentile(values: number[], percentile: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.max(0, Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * percentile)));
  return sorted[idx];
}

export function debugKeystrokeDurations(tag: string, durations: number[], extras: Record<string, unknown> = {}) {
  void tag;
  void durations;
  void extras;
}

export function lerpRgba(
  base: readonly [number, number, number, number],
  target: readonly [number, number, number, number],
  t: number
): [number, number, number, number] {
  return [
    lerp(base[0], target[0], t),
    lerp(base[1], target[1], t),
    lerp(base[2], target[2], t),
    lerp(base[3], target[3], t),
  ];
}

export function rgbaToCss(color: readonly [number, number, number, number]): string {
  const [r, g, b, a] = color;
  return `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${Math.max(0, Math.min(1, a))})`;
}

export function easeOutCubic(t: number): number {
  const p = 1 - clamp01(t);
  return 1 - (p * p * p);
}

export function easeInCubic(t: number): number {
  const p = clamp01(t);
  return p * p * p;
}

// --- FONT VARIATIONS ---

export function applyKeystrokeFontVariations(
  ctx: CanvasRenderingContext2D,
  visual: KeystrokeVisualState
) {
  ctx.canvas.style.fontVariationSettings = `'wdth' ${visual.wdth.toFixed(2)}, 'wght' ${visual.wght.toFixed(2)}, 'slnt' ${visual.slnt.toFixed(2)}, 'ROND' ${visual.rond.toFixed(2)}`;
}

// --- VISUAL STATE ---

export function getKeystrokeVisualState(
  currentTime: number,
  eventStart: number,
  eventEnd: number,
  eventType: KeystrokeEvent['type'],
  isHold: boolean
): KeystrokeVisualState {
  const isMouse = eventType === 'mousedown' || eventType === 'wheel';
  const duration = Math.max(0.001, eventEnd - eventStart);
  const enterSpan = Math.min(KEYSTROKE_ANIM_ENTER_SEC, duration * 0.36);
  const exitSpan = Math.min(KEYSTROKE_ANIM_EXIT_SEC, duration * 0.36);
  const exitStart = Math.max(eventStart, eventEnd - exitSpan);

  const baseSlnt = isMouse ? -6 : 0;
  const baseRond = isMouse ? 96 : 88;
  const state: KeystrokeVisualState = {
    alpha: 1,
    scale: 1,
    scaleX: 1,
    scaleY: 1,
    translateY: 0,
    wdth: 100,
    wght: 600,
    slnt: baseSlnt,
    rond: baseRond,
    holdMix: 0,
    laneWeight: 1,
  };

  if (currentTime < eventStart + enterSpan) {
    const t = clamp01((currentTime - eventStart) / Math.max(0.001, enterSpan));
    const eased = easeOutCubic(t);
    const laneLead = easeOutCubic(clamp01((t - 0.03) / 0.42));
    state.alpha = lerp(0, 1, eased);
    state.scale = lerp(0.93, 1.01, eased);
    state.scaleX = lerp(1.1, 1, eased);
    state.scaleY = lerp(0.9, 1, eased);
    state.translateY = lerp(10, 0, eased);
    state.laneWeight = laneLead;
    if (t > 0.76) {
      const settleT = (t - 0.76) / 0.24;
      state.scale = lerp(state.scale, 1, easeOutCubic(settleT));
    }
  }

  if (isHold) {
    // The hold visual state begins slightly after the enter animation finishes
    const holdStart = eventStart + enterSpan * 0.6;
    // The hold visual state ends just before the exit animation begins
    const holdEnd = eventEnd - exitSpan * 0.4;

    if (holdEnd > holdStart && currentTime >= holdStart && currentTime <= holdEnd) {
      const transitionSec = 0.08; // Fixed 80ms snappy transition
      const holdMixIn = clamp01((currentTime - holdStart) / transitionSec);
      const holdMixOut = clamp01((holdEnd - currentTime) / transitionSec);
      const holdMix = Math.min(holdMixIn, holdMixOut);

      const squish = (isMouse ? 0.06 : 0.045) * holdMix;
      state.holdMix = holdMix;
      state.scale *= lerp(1, 1.014, holdMix);
      state.scaleX *= 1 + squish;
      state.scaleY *= 1 - squish * 0.72;
      state.translateY += lerp(0, -2.6, holdMix);
      state.laneWeight = Math.max(state.laneWeight, lerp(0.84, 1, holdMix));
    }
  }

  if (currentTime > exitStart) {
    const t = clamp01((currentTime - exitStart) / Math.max(0.001, eventEnd - exitStart));
    const eased = easeInCubic(t);
    state.alpha *= lerp(1, 0, eased);
    state.scale *= lerp(1, 0.93, eased);
    state.scaleX *= lerp(1, 1.04, eased);
    state.scaleY *= lerp(1, 0.89, eased);
    state.translateY += lerp(0, -9, eased);
    state.holdMix *= lerp(1, 0.15, eased);
    state.laneWeight *= lerp(1, 0.76, eased);
  }

  // Static lock: font variation axes are strictly derived from holdMix to match GPU export crossfade
  state.wdth = lerp(100, isMouse ? 95 : 97, state.holdMix);
  state.wght = lerp(600, isMouse ? 675 : 655, state.holdMix);
  state.slnt = lerp(baseSlnt, isMouse ? -12 : -2, state.holdMix);
  state.rond = lerp(baseRond, isMouse ? 82 : 78, state.holdMix);

  return state;
}

// --- LABEL & COLOR ---

export function translateLabel(label: string, lang: string): string {
  if (lang === 'en') return label;

  // Arrow keys -> symbols universally for all non-English languages.
  // They appear both as standalone keys and as combo tokens ("Ctrl + Left" -> "Ctrl + <-").
  const ARROW_SYMBOLS: Record<string, string> = {
    'Left': '\u2190', 'Right': '\u2192', 'Up': '\u2191', 'Down': '\u2193',
  };

  // Language-specific overrides. Rules per locale:
  //   ko - transliterate to phonetics; modifier keys (Ctrl/Shift/Alt/Win) stay English
  //   vi - only mouse labels + Space get Vietnamese; all other keys stay English (research-backed)
  //   es - standard Spanish computing terms (Intro, Retroceso, Supr, Inicio/Fin, Re/Av Pag)
  //   ja - katakana phonetics for common keys; Home/End/PageUp/PageDown stay English
  //   zh - standard Chinese computing terms; Tab and Esc stay English
  const LOCALIZATION_MAPS: Record<string, Record<string, string>> = {
    ko: {
      'Left Click': '\uC88C\uD074\uB9AD', 'Right Click': '\uC6B0\uD074\uB9AD', 'Middle Click': '\uD720\uD074\uB9AD',
      '\u2191 Scroll': '\u2191 \uC2A4\uD06C\uB864', '\u2193 Scroll': '\u2193 \uC2A4\uD06C\uB864', 'Mouse Click': '\uD074\uB9AD',
      'Space': '\uC2A4\uD398\uC774\uC2A4', 'Enter': '\uC5D4\uD130', 'Backspace': '\uBC31\uC2A4\uD398\uC774\uC2A4',
      'Esc': 'ESC', 'Tab': '\uD0ED', 'Delete': '\uC0AD\uC81C', 'Insert': '\uC0BD\uC785',
      'Home': 'Home', 'End': 'End', 'PageUp': '\uD398\uC774\uC9C0\uC5C5', 'PageDown': '\uD398\uC774\uC9C0\uB2E4\uC6B4',
      'CapsLock': '\uD55C/\uC601',
    },
    vi: {
      // Mouse labels use Vietnamese; key names stay English per local convention
      'Left Click': 'Chu\u1ED9t Tr\u00E1i', 'Right Click': 'Chu\u1ED9t Ph\u1EA3i', 'Middle Click': 'Chu\u1ED9t Gi\u1EEFa',
      '\u2191 Scroll': '\u2191 Cu\u1ED9n', '\u2193 Scroll': '\u2193 Cu\u1ED9n', 'Mouse Click': 'Nh\u1EA5p Chu\u1ED9t',
      'Space': 'C\u00E1ch',
    },
    es: {
      'Left Click': 'Clic Izq', 'Right Click': 'Clic Der', 'Middle Click': 'Clic Central',
      '\u2191 Scroll': '\u2191 Desplazar', '\u2193 Scroll': '\u2193 Desplazar', 'Mouse Click': 'Clic',
      'Space': 'Espacio', 'Enter': 'Intro', 'Backspace': 'Retroceso',
      'Esc': 'Esc', 'Tab': 'Tab', 'Delete': 'Supr', 'Insert': 'Ins',
      'Home': 'Inicio', 'End': 'Fin', 'PageUp': 'Re P\u00E1g', 'PageDown': 'Av P\u00E1g',
    },
    ja: {
      'Left Click': '\u5DE6\u30AF\u30EA\u30C3\u30AF', 'Right Click': '\u53F3\u30AF\u30EA\u30C3\u30AF', 'Middle Click': '\u4E2D\u30AF\u30EA\u30C3\u30AF',
      '\u2191 Scroll': '\u2191 \u30B9\u30AF\u30ED\u30FC\u30EB', '\u2193 Scroll': '\u2193 \u30B9\u30AF\u30ED\u30FC\u30EB', 'Mouse Click': '\u30AF\u30EA\u30C3\u30AF',
      // Katakana phonetics; Enter!=\u78BA\u5B9A (that's IME confirm), Backspace!=Delete
      'Space': '\u30B9\u30DA\u30FC\u30B9', 'Enter': '\u30A8\u30F3\u30BF\u30FC', 'Backspace': '\u30D0\u30C3\u30AF\u30B9\u30DA\u30FC\u30B9', 'Delete': '\u30C7\u30EA\u30FC\u30C8',
      'Esc': 'ESC', 'Tab': '\u30BF\u30D6', 'Insert': '\u633F\u5165',
      // Home/End/PageUp/PageDown stay English in Japanese convention
    },
    zh: {
      'Left Click': '\u5DE6\u952E\u70B9\u51FB', 'Right Click': '\u53F3\u952E\u70B9\u51FB', 'Middle Click': '\u4E2D\u952E\u70B9\u51FB',
      '\u2191 Scroll': '\u2191 \u6EDA\u52A8', '\u2193 Scroll': '\u2193 \u6EDA\u52A8', 'Mouse Click': '\u70B9\u51FB',
      // Standard Chinese computing terms; Tab and Esc stay English
      'Space': '\u7A7A\u683C', 'Enter': '\u56DE\u8F66', 'Backspace': '\u9000\u683C', 'Delete': '\u5220\u9664',
      'Home': '\u884C\u9996', 'End': '\u884C\u5C3E', 'PageUp': '\u4E0A\u4E00\u9875', 'PageDown': '\u4E0B\u4E00\u9875',
      'CapsLock': '\u5927\u5199\u9501\u5B9A',
    },
  };

  const map = LOCALIZATION_MAPS[lang] || {};
  // Split modifier combos like "Ctrl + Left Click", translate each token independently
  const parts = label.split(' + ');
  const translatedParts = parts.map(part => map[part] ?? ARROW_SYMBOLS[part] ?? part);
  return translatedParts.join(' + ');
}

export function getKeystrokeLabel(state: KeystrokeState, event: KeystrokeEvent): string {
  const label = translateLabel(event.label, state.keystrokeLanguage);
  return event.count > 1 ? `${label} \u00D7${event.count}` : label;
}

export function getKeystrokeBorderColor(event: KeystrokeEvent, holdMix: number = 0): string {
  const t = clamp01(holdMix);
  if (event.type === 'wheel') {
    return rgbaToCss(lerpRgba([56, 189, 248, 0.5], [14, 165, 233, 0.8], t));
  }
  if (event.type === 'mousedown') {
    return rgbaToCss(lerpRgba([251, 191, 36, 0.5], [245, 158, 11, 0.82], t));
  }
  return rgbaToCss(lerpRgba([74, 222, 128, 0.5], [34, 197, 94, 0.84], t));
}

export function getKeystrokeFillColor(event: KeystrokeEvent, holdMix: number): string {
  const t = clamp01(holdMix);
  if (event.type === 'wheel') {
    return rgbaToCss(lerpRgba([18, 18, 20, 0.88], [13, 48, 74, 0.93], t));
  }
  if (event.type === 'mousedown') {
    return rgbaToCss(lerpRgba([18, 18, 20, 0.88], [70, 52, 18, 0.93], t));
  }
  return rgbaToCss(lerpRgba([18, 18, 20, 0.88], [24, 58, 37, 0.93], t));
}

export function getKeystrokeTextColor(holdMix: number): string {
  return rgbaToCss(lerpRgba([255, 255, 255, 0.98], [244, 255, 249, 1], clamp01(holdMix)));
}

// --- CACHE & IDENTITY ---

export function getKeystrokeIdentity(event: KeystrokeEvent): string {
  if (event.type === 'keyboard') {
    const key = event.key || event.label;
    const mods = event.modifiers || {};
    return `keyboard:${key}|c:${mods.ctrl ? 1 : 0}|a:${mods.alt ? 1 : 0}|s:${mods.shift ? 1 : 0}|w:${mods.win ? 1 : 0}`;
  }
  if (event.type === 'mousedown') {
    return `mousedown:${event.btn ?? 'mouse'}`;
  }
  return `wheel:${event.direction ?? 'none'}`;
}

export function upperBound(sorted: number[], value: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (sorted[mid] <= value) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}

export function isTimeInsideSegments(time: number, segments: CursorVisibilitySegment[]): boolean {
  if (!segments.length) return false;
  let lo = 0;
  let hi = segments.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (segments[mid].startTime <= time) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  const idx = lo - 1;
  if (idx < 0) return false;
  const segment = segments[idx];
  return time >= segment.startTime && time <= segment.endTime;
}

// --- LANE SLOT ASSIGNMENT & RENDER CACHE ---

export function assignKeystrokeLaneSlots(
  laneIndices: number[],
  displayEvents: KeystrokeEvent[],
  effectiveEnds: number[],
  eventIdentities: string[],
  eventSlots: number[]
) {
  const active: Array<{ endTime: number; slot: number; identity: string }> = [];
  const usedSlots = new Set<number>();
  const preferredSlots = new Map<string, number>();
  const highestUsedSlot = () => {
    let max = -1;
    for (const slot of usedSlots) {
      if (slot > max) max = slot;
    }
    return max;
  };

  const releaseFinished = (startTime: number) => {
    for (let i = active.length - 1; i >= 0; i--) {
      if (active[i].endTime <= startTime + 0.000001) {
        usedSlots.delete(active[i].slot);
        preferredSlots.set(active[i].identity, active[i].slot);
        active.splice(i, 1);
      }
    }
  };

  for (const eventIndex of laneIndices) {
    const event = displayEvents[eventIndex];
    releaseFinished(event.startTime);
    const identity = eventIdentities[eventIndex];

    // Same identity override: the latest occurrence keeps the slot.
    for (let i = active.length - 1; i >= 0; i--) {
      if (active[i].identity === identity) {
        usedSlots.delete(active[i].slot);
        preferredSlots.set(identity, active[i].slot);
        active.splice(i, 1);
      }
    }

    const preferred = preferredSlots.get(identity);
    let slot: number;
    if (preferred !== undefined && !usedSlots.has(preferred)) {
      slot = preferred;
    } else {
      slot = highestUsedSlot() + 1;
      while (usedSlots.has(slot)) slot += 1;
    }

    preferredSlots.set(identity, slot);
    eventSlots[eventIndex] = slot;
    usedSlots.add(slot);
    active.push({
      endTime: effectiveEnds[eventIndex],
      slot,
      identity,
    });
  }
}

export function rebuildKeystrokeRenderCache(
  state: KeystrokeState,
  segment: VideoSegment,
  duration: number
): KeystrokeRenderCache | null {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'off') {
    if (state.renderCache.mode !== 'off') {
      state.renderCache = {
        mode: 'off',
        segmentRef: null,
        eventsRef: null,
        visibilityRef: null,
        duration: 0,
        displayEvents: [],
        startTimes: [],
        effectiveEnds: [],
        keyboardStartTimes: [],
        keyboardIndices: [],
        mouseStartTimes: [],
        mouseIndices: [],
        keyboardMaxDuration: 0,
        mouseMaxDuration: 0,
        eventSlots: [],
        eventIdentities: [],
        keyboardSlotRepresentatives: [],
        mouseSlotRepresentatives: [],
      };
      state.layoutCache.clear();
    }
    return null;
  }

  const eventsRef = segment.keystrokeEvents ?? [];
  const visibilityRef = getKeystrokeVisibilitySegmentsForMode(segment);
  const safeDuration = Math.max(0, duration);
  const cache = state.renderCache;
  const canReuse =
    cache.mode === mode &&
    cache.segmentRef === segment &&
    cache.eventsRef === eventsRef &&
    cache.visibilityRef === visibilityRef &&
    Math.abs(cache.duration - safeDuration) < 0.000001;

  if (canReuse) {
    return cache;
  }

  const displayEvents = [...filterKeystrokeEventsByMode(eventsRef, mode)].sort(
    (a, b) => a.startTime - b.startTime
  );
  const startTimes: number[] = [];
  const effectiveEnds = new Array<number>(displayEvents.length);
  const keyboardStartTimes: number[] = [];
  const keyboardIndices: number[] = [];
  const mouseStartTimes: number[] = [];
  const mouseIndices: number[] = [];
  const eventIdentities = new Array<string>(displayEvents.length);
  const eventSlots = new Array<number>(displayEvents.length).fill(0);
  const keyboardSlotRepresentatives: number[] = [];
  const keyboardSlotLabelLengths: number[] = [];
  const mouseSlotRepresentatives: number[] = [];
  const mouseSlotLabelLengths: number[] = [];
  let keyboardMaxDuration = 0;
  let mouseMaxDuration = 0;
  for (let i = displayEvents.length - 1; i >= 0; i--) {
    const event = displayEvents[i];
    effectiveEnds[i] = Math.min(event.endTime, safeDuration);
  }
  for (let i = 0; i < displayEvents.length; i++) {
    const event = displayEvents[i];
    eventIdentities[i] = getKeystrokeIdentity(event);
    startTimes.push(event.startTime);
    const effectiveDuration = Math.max(0, effectiveEnds[i] - event.startTime);
    if (event.type === 'keyboard') {
      keyboardStartTimes.push(event.startTime);
      keyboardIndices.push(i);
      if (effectiveDuration > keyboardMaxDuration) keyboardMaxDuration = effectiveDuration;
    } else {
      mouseStartTimes.push(event.startTime);
      mouseIndices.push(i);
      if (effectiveDuration > mouseMaxDuration) mouseMaxDuration = effectiveDuration;
    }
  }
  assignKeystrokeLaneSlots(
    keyboardIndices,
    displayEvents,
    effectiveEnds,
    eventIdentities,
    eventSlots
  );
  assignKeystrokeLaneSlots(
    mouseIndices,
    displayEvents,
    effectiveEnds,
    eventIdentities,
    eventSlots
  );
  for (let i = 0; i < displayEvents.length; i++) {
    const event = displayEvents[i];
    const slot = eventSlots[i];
    const labelLength = getKeystrokeLabel(state, event).length;
    if (event.type === 'keyboard') {
      const currentLen = keyboardSlotLabelLengths[slot] ?? -1;
      if (labelLength >= currentLen) {
        keyboardSlotLabelLengths[slot] = labelLength;
        keyboardSlotRepresentatives[slot] = i;
      }
    } else {
      const currentLen = mouseSlotLabelLengths[slot] ?? -1;
      if (labelLength >= currentLen) {
        mouseSlotLabelLengths[slot] = labelLength;
        mouseSlotRepresentatives[slot] = i;
      }
    }
  }
  debugKeystrokeDurations(
    'RenderCache',
    displayEvents.map((event, index) => Math.max(0, effectiveEnds[index] - event.startTime)),
    { mode, duration: safeDuration }
  );

  state.renderCache = {
    mode,
    segmentRef: segment,
    eventsRef,
    visibilityRef,
    duration: safeDuration,
    displayEvents,
    startTimes,
    effectiveEnds,
    keyboardStartTimes,
    keyboardIndices,
    mouseStartTimes,
    mouseIndices,
    keyboardMaxDuration,
    mouseMaxDuration,
    eventSlots,
    eventIdentities,
    keyboardSlotRepresentatives,
    mouseSlotRepresentatives,
  };
  state.layoutCache.clear();
  return state.renderCache;
}
