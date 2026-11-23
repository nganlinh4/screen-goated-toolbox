import {
  buildCompositionAssetKey,
  idbHasKey,
  sortProjectsByDisplayOrder,
  type StoredProjectRecord,
} from '@/lib/projectStorage';

/** The oldest projects a limit of `limit` leaves out; the open project is never chosen. */
export function selectProjectsBeyondLimit<T extends { id: string; createdAt: number; lastModified: number }>(
  projects: T[],
  limit: number,
  activeProjectId: string | null | undefined,
): T[] {
  const excess = projects.length - limit;
  if (excess <= 0) return [];
  return sortProjectsByDisplayOrder(projects)
    .reverse()
    .filter((project) => project.id !== activeProjectId)
    .slice(0, excess);
}

/** Automatic retention must keep every project that is the discovery owner of media. */
export async function hasStoredProjectMedia(project: StoredProjectRecord): Promise<boolean> {
  if (project.rawVideoPath || project.rawMicAudioPath || project.rawWebcamVideoPath) return true;
  if ([...(project.composition?.audioSegments ?? []), ...(project.composition?.narrationSegments ?? [])]
    .some((segment) => segment.rawAudioPath)) return true;
  for (const store of ['videos', 'audio', 'mic_audio', 'webcam_videos']) {
    if (await idbHasKey(store, project.id)) return true;
  }
  for (const clip of [
    ...(project.composition?.clips ?? []),
    ...(project.composition?.retainedRemovedClips ?? []),
  ]) {
    if (clip.rawVideoPath || clip.rawMicAudioPath || clip.rawWebcamVideoPath) return true;
    if (clip.role === 'root') continue;
    const key = buildCompositionAssetKey(project.id, clip.id);
    for (const store of ['composition_videos', 'composition_audio', 'composition_mic_audio', 'composition_webcam_videos']) {
      if (await idbHasKey(store, key)) return true;
    }
  }
  return false;
}
