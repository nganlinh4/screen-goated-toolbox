export interface AudioImportWriteFence {
  readonly projectId: string;
  committed: boolean;
}

const fences = new Map<string, AudioImportWriteFence>();
const listeners = new Map<string, Set<() => void>>();
const imports = new Set<object>();
let shutdownToken: object | null = null;

/** Freeze new audio writes while the native shutdown acknowledgement is pending. */
export function beginAudioImportShutdown(): object | null {
  if (shutdownToken || fences.size || imports.size) return null;
  shutdownToken = {};
  return shutdownToken;
}

export function cancelAudioImportShutdown(token: object | null): void {
  if (token && shutdownToken === token) shutdownToken = null;
}

/** Keep shutdown from overtaking imports before their project write begins. */
export function beginAudioImport(): object {
  if (shutdownToken) throw new Error('The recorder is closing; audio import cannot start yet.');
  const token = {};
  imports.add(token);
  return token;
}

export function finishAudioImport(token: object): void { imports.delete(token); }

export function hasPendingAudioImport(): boolean { return imports.size > 0 || fences.size > 0; }
export function recorderShutdownPreparing(): boolean { return shutdownToken !== null; }

export function beginAudioImportWrite(projectId: string): AudioImportWriteFence {
  if (shutdownToken) throw new Error('The recorder is closing; audio import cannot start yet.');
  if (fences.has(projectId)) throw new Error('Audio recovery is pending for this project. Retry saving the current edits.');
  const fence = { projectId, committed: false };
  fences.set(projectId, fence);
  listeners.get(projectId)?.forEach((listener) => listener());
  return fence;
}

export function isAudioImportWriteAllowed(projectId: string, fence?: AudioImportWriteFence): boolean {
  const current = fences.get(projectId);
  return !current || current === fence;
}

export function subscribeAudioImportWriteInvalidation(projectId: string, fence: AudioImportWriteFence | undefined,
  listener: () => void): () => void {
  const subscribed = listeners.get(projectId) ?? new Set<() => void>();
  const check = () => { if (!isAudioImportWriteAllowed(projectId, fence)) listener(); };
  subscribed.add(check);
  listeners.set(projectId, subscribed);
  return () => { subscribed.delete(check); if (!subscribed.size) listeners.delete(projectId); };
}

export function audioProjectWriteGuard(projectId: string, enabled: boolean, fence?: AudioImportWriteFence,
  canCommit?: () => boolean, onInvalidated?: (listener: () => void) => () => void) {
  return {
    isCurrent: () => (!enabled || isAudioImportWriteAllowed(projectId, fence)) && (!canCommit || canCommit()),
    subscribe: (listener: () => void) => {
      const releaseFence = enabled ? subscribeAudioImportWriteInvalidation(projectId, fence, listener) : undefined;
      const releaseOwner = onInvalidated?.(listener);
      return () => { releaseFence?.(); releaseOwner?.(); };
    },
  };
}

export function finishAudioImportWrite(fence: AudioImportWriteFence, published: boolean): void {
  if (fences.get(fence.projectId) === fence && (published || !fence.committed)) fences.delete(fence.projectId);
}

export function assertAudioImportWriteAllowed(projectId: string, changesTimeline: boolean, fence?: AudioImportWriteFence): void {
  const current = fences.get(projectId);
  if (current && current !== fence && changesTimeline) {
    throw new Error(current.committed
      ? 'Audio was saved, but later edits are not saved. Retry saving the current edits.'
      : 'Audio import is in progress; try saving again when it finishes.');
  }
}

export function markAudioImportCommitted(projectId: string, fence?: AudioImportWriteFence): void {
  if (fence && fences.get(projectId) === fence) fence.committed = true;
}
