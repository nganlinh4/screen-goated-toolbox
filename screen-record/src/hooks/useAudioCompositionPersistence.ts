import { useCallback, useRef, type MutableRefObject } from 'react';
import type { Project, ProjectComposition } from '@/types/video';
import { logToHost } from '@/lib/ipc';
import { projectManager } from '@/lib/projectManager';
import { AudioSavedNotPublishedError, type TimelineWriteOwner } from './timelineWriteOwner';

export function audioSaveFailure(error: unknown): Error {
  const failure = new Error(`Could not save imported audio: ${String(error)}`);
  Object.assign(failure, { cause: error });
  return failure;
}

export function useAudioCompositionPersistence({ currentProjectDataRef, currentProjectId,
  currentProjectIdRef, loadProjects, setComposition }: {
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectId: string | null;
  currentProjectIdRef: MutableRefObject<string | null>;
  loadProjects: () => Promise<unknown>;
  setComposition: (composition: ProjectComposition) => void;
}) {
  const chainRef = useRef<Promise<void>>(Promise.resolve());
  return useCallback((nextComposition: ProjectComposition, reason: string, owner?: TimelineWriteOwner) => {
    if (owner && !owner.isCurrent()) throw new Error('The timeline changed while audio was importing');
    if (!owner) setComposition(nextComposition);
    const projectId = currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? currentProjectId ?? null;
    if (!projectId) {
      void logToHost(`[AudioImport][Frontend] skip composition persist reason="${reason}" no-project`);
      return;
    }
    const writeIntent = projectManager.createEditorWriteIntent(projectId);
    const persistTask = chainRef.current.catch(() => undefined).then(async () => {
      if (owner && !owner.isCurrent()) throw new Error('The timeline changed while audio was saving');
      const applied = await projectManager.updateProject(projectId, { composition: nextComposition },
        writeIntent, owner?.isCurrent, owner?.onInvalidated, owner?.audioFence);
      if (!applied && owner) throw new Error(owner.isCurrent()
        ? 'The timeline changed while audio was saving'
        : 'The open project changed while audio was saving');
    }).catch((error: unknown) => {
      if (owner) throw audioSaveFailure(error);
      throw error;
    }).then(() => {
      if (owner && !owner.isCurrent()) throw new AudioSavedNotPublishedError();
      if (owner) setComposition(nextComposition);
      return true;
    });
    chainRef.current = persistTask.then(() => undefined, () => undefined);
    void persistTask.then(() => loadProjects()).catch((error) => {
      console.warn('[AudioImport] Failed to persist composition', error);
      void logToHost(`[AudioImport][Frontend] composition persist failed reason="${reason}" project="${projectId}" error="${String(error)}"`);
    });
    return persistTask;
  }, [currentProjectDataRef, currentProjectId, currentProjectIdRef, loadProjects, setComposition]);
}
