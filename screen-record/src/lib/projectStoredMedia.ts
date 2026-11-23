export const PROJECT_MEDIA_BLOB_KEYS = [
  "videoBlob",
  "audioBlob",
  "micAudioBlob",
  "webcamBlob",
] as const;

type ProjectMediaBlobKey = (typeof PROJECT_MEDIA_BLOB_KEYS)[number];
type ProjectMediaBlobs = Partial<Record<ProjectMediaBlobKey, Blob | null | undefined>>;

/**
 * Identity of the root media blobs last read from or written to IndexedDB, kept
 * for one project only so in-memory recordings of other projects are not retained.
 * Re-putting the same Blob makes Chromium copy the whole file again, so writes
 * that would store an identical Blob are dropped instead.
 */
let tracked: { projectId: string; blobs: Map<ProjectMediaBlobKey, Blob | null> } | null = null;

export function rememberStoredProjectMedia(projectId: string, media: ProjectMediaBlobs): void {
  // Metadata-only writes (rename, other projects) must not reset the tracked project.
  if (!PROJECT_MEDIA_BLOB_KEYS.some((key) => key in media)) return;
  if (tracked?.projectId !== projectId) tracked = { projectId, blobs: new Map() };
  for (const key of PROJECT_MEDIA_BLOB_KEYS) {
    if (key in media) tracked.blobs.set(key, media[key] ?? null);
  }
}

export function forgetStoredProjectMedia(projectId: string): void {
  if (tracked?.projectId === projectId) tracked = null;
}

/** Drops blob updates that would rewrite exactly the Blob already stored. */
export function omitUnchangedProjectMedia<T extends ProjectMediaBlobs>(projectId: string, updates: T): T {
  if (tracked?.projectId !== projectId) return updates;
  const stored = tracked.blobs;
  let next: T | null = null;
  for (const key of PROJECT_MEDIA_BLOB_KEYS) {
    const value = updates[key];
    if (!(key in updates) || !value || stored.get(key) !== value) continue;
    next ??= { ...updates };
    delete next[key];
  }
  return next ?? updates;
}
