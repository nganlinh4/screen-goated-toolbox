const CURSOR_DEBUG_STORAGE_KEY = 'screen-record-cursor-debug-v1';

function getCursorDebugPreference(): string | null {
  try {
    return localStorage.getItem(CURSOR_DEBUG_STORAGE_KEY);
  } catch {
    return null;
  }
}

export function isCursorDebugEnabled(): boolean {
  return getCursorDebugPreference() !== '0';
}

type SmartPointerDebugTransition = {
  time: number;
  state: 'active' | 'idle';
  meaningfulMovement: boolean;
  nearInteraction: boolean;
  clicked: boolean;
  centerLockOverride: boolean;
  netDistance: number;
  pathDistance: number;
};

type SmartPointerDebugPayload = {
  timelineEnd: number;
  sampleCount: number;
  motionSampleCount?: number;
  sourceWidth: number;
  sourceHeight: number;
  centerLockHalfSize: number;
  visibleSegments: Array<{ start: number; end: number }>;
  idleRanges: Array<{ start: number; end: number }>;
  transitions: SmartPointerDebugTransition[];
};

export function logSmartPointerGeneration(payload: SmartPointerDebugPayload): void {
  if (!isCursorDebugEnabled()) return;

  console.groupCollapsed(
    `[SmartPointer] raw=${payload.sampleCount}, motion=${payload.motionSampleCount ?? payload.sampleCount}, visible=${payload.visibleSegments.length}, timeline=${payload.timelineEnd.toFixed(3)}s`
  );
  console.log('frame', {
    width: Math.round(payload.sourceWidth),
    height: Math.round(payload.sourceHeight),
    centerLockHalfSize: Math.round(payload.centerLockHalfSize * 100) / 100,
  });
  console.log('visibleSegments', payload.visibleSegments);
  console.log('idleRanges', payload.idleRanges);
  console.table(payload.transitions);
  console.groupEnd();
}

type PreviewCursorDebugPayload = {
  previewTime: number;
  cursorSampleTime: number;
  x: number | null;
  y: number | null;
  deltaPx: number | null;
  motionState: 'moving' | 'stopped' | 'missing';
  visible: boolean;
  visibilityReason: string;
  opacity: number;
  scale: number;
  clicked: boolean;
  cursorType: string;
  segmentCount: number | null;
};

export function logPreviewCursorState(payload: PreviewCursorDebugPayload): void {
  if (!isCursorDebugEnabled()) return;

  console.log('[CursorPreview]', payload);
}
