import { useSyncExternalStore } from 'react';
import type { AudioImportWriteFence } from '@/lib/audioImportWriteFence';
import type { ImportedAudioSegment } from '@/types/video';

export interface AudioRecoveryRequest {
  fence: AudioImportWriteFence;
  projectId: string;
  epoch: number;
  clipId: string | null | undefined;
  segments: ImportedAudioSegment[];
}

export interface AudioRecoveryEditor {
  canResume: (request: AudioRecoveryRequest) => boolean;
  resume: (request: AudioRecoveryRequest) => Promise<void>;
}

const editors = new WeakMap<object, React.MutableRefObject<AudioRecoveryEditor | null>>();

interface AudioImportRecovery {
  fence: AudioImportWriteFence;
  isCurrent: () => boolean;
  retry: () => Promise<void>;
  projectName: string;
  busy: boolean;
  failed: boolean;
}

const pending = new Map<AudioImportWriteFence, AudioImportRecovery>();
const listeners = new Set<() => void>();
let snapshot: readonly AudioImportRecovery[] = [];
function changed() {
  snapshot = [...pending.values()];
  listeners.forEach((listener) => listener());
}

export function currentAudioRecoveryEditor(owner: object): AudioRecoveryEditor | null {
  return editors.get(owner)?.current ?? null;
}

export function registerAudioRecoveryEditor(owner: object, editor: React.MutableRefObject<AudioRecoveryEditor | null>): () => void {
  editors.set(owner, editor);
  changed();
  return () => { if (editors.get(owner) === editor) { editors.delete(owner); changed(); } };
}

export function retainAudioImportRecovery(fence: AudioImportWriteFence, projectName: string, isCurrent: () => boolean, retry: () => Promise<void>): void {
  const previous = pending.get(fence);
  pending.set(fence, { fence, projectName, isCurrent, retry, busy: previous?.busy ?? false, failed: previous?.failed ?? false });
  changed();
}

export function clearAudioImportRecovery(fence: AudioImportWriteFence): void {
  if (pending.delete(fence)) changed();
}

export async function retryAudioImportRecovery(fence: AudioImportWriteFence): Promise<void> {
  const recovery = pending.get(fence);
  if (!recovery || recovery.busy) return;
  if (!recovery.isCurrent()) {
    pending.set(fence, { ...recovery, failed: true });
    changed();
    return;
  }
  pending.set(fence, { ...recovery, busy: true, failed: false });
  changed();
  try {
    await recovery.retry();
  } catch {
    const latest = pending.get(fence);
    if (latest) pending.set(fence, { ...latest, busy: false, failed: true });
  } finally {
    const latest = pending.get(fence);
    if (latest?.busy) pending.set(fence, { ...latest, busy: false });
    changed();
  }
}

export function useAudioImportRecovery(): readonly AudioImportRecovery[] {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    () => snapshot, () => snapshot);
}
