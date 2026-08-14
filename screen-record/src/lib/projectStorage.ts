import type { Project, ProjectComposition } from "@/types/video";

export const PROJECT_SWITCH_DEBUG = false;
export const PROJECTS_STORE = "projects";
export const APP_META_STORE = "app_meta";
export const LEGACY_PROJECTS_KEY = "screen-demo-projects";
export const PROJECT_MIGRATION_KEY = "projects-storage-migrated-v1";

const DB_NAME = "ScreenDemoDB";
const DB_VERSION = 9;
const PROJECT_OBJECT_STORES = [
  PROJECTS_STORE,
  APP_META_STORE,
  "videos",
  "audio",
  "mic_audio",
  "webcam_videos",
  "mouse",
  "thumbnails",
  "custom_backgrounds",
  "segments",
  "composition_videos",
  "composition_audio",
  "composition_mic_audio",
  "composition_webcam_videos",
  "composition_custom_backgrounds",
] as const;

const PROJECT_BUNDLE_STORES = [
  PROJECTS_STORE,
  "videos",
  "audio",
  "mic_audio",
  "webcam_videos",
] as const;

const COMPOSITION_ASSET_STORES = [
  "composition_videos",
  "composition_audio",
  "composition_mic_audio",
  "composition_webcam_videos",
  "composition_custom_backgrounds",
] as const;

const PROJECT_DELETE_STORES = [
  ...PROJECT_BUNDLE_STORES,
  "mouse",
  "thumbnails",
  "custom_backgrounds",
  "segments",
  ...COMPOSITION_ASSET_STORES,
] as const;

export type StoredProjectRecord = Omit<
  Project,
  "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob"
>;

type ProjectMediaFields = Pick<
  Project,
  "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob"
>;

let dbPromise: Promise<IDBDatabase> | null = null;

export function summarizeProjectUpdate(
  updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
) {
  const rootBackground = updates.composition?.clips?.find(
    (clip) => clip.id === "root",
  )?.backgroundConfig;
  return {
    name: updates.name ?? null,
    backgroundConfig: updates.backgroundConfig
      ? {
          backgroundType: updates.backgroundConfig.backgroundType,
          canvasMode: updates.backgroundConfig.canvasMode ?? "auto",
          canvasWidth: updates.backgroundConfig.canvasWidth ?? null,
          canvasHeight: updates.backgroundConfig.canvasHeight ?? null,
          autoCanvasSourceId:
            updates.backgroundConfig.autoCanvasSourceId ?? null,
          scale: updates.backgroundConfig.scale,
        }
      : null,
    segment: updates.segment
      ? {
          trimStart: updates.segment.trimStart,
          trimEnd: updates.segment.trimEnd,
          crop: updates.segment.crop ?? null,
        }
      : null,
    compositionRootBackground: rootBackground
      ? {
          backgroundType: rootBackground.backgroundType ?? null,
          canvasMode: rootBackground.canvasMode ?? "auto",
          canvasWidth: rootBackground.canvasWidth ?? null,
          canvasHeight: rootBackground.canvasHeight ?? null,
        }
      : null,
  };
}

export function summarizeStoredProject(
  project: StoredProjectRecord | null | undefined,
) {
  if (!project) return null;
  return {
    id: project.id ?? null,
    name: project.name ?? null,
    backgroundConfig: project.backgroundConfig
      ? {
          backgroundType: project.backgroundConfig.backgroundType,
          canvasMode: project.backgroundConfig.canvasMode ?? "auto",
          canvasWidth: project.backgroundConfig.canvasWidth ?? null,
          canvasHeight: project.backgroundConfig.canvasHeight ?? null,
          autoCanvasSourceId: project.backgroundConfig.autoCanvasSourceId ?? null,
          scale: project.backgroundConfig.scale ?? null,
        }
      : null,
    segment: project.segment
      ? {
          trimStart: project.segment.trimStart,
          trimEnd: project.segment.trimEnd,
          crop: project.segment.crop ?? null,
        }
      : null,
  };
}

export function buildCompositionAssetKey(
  projectId: string,
  clipId: string,
): string {
  return `${projectId}:${clipId}`;
}

export function stripHeavyProjectFields(
  project: Project | StoredProjectRecord,
): StoredProjectRecord {
  const record = { ...project } as Project;
  delete (record as Partial<Project>).videoBlob;
  delete (record as Partial<Project>).audioBlob;
  delete (record as Partial<Project>).micAudioBlob;
  delete (record as Partial<Project>).webcamBlob;
  return record as StoredProjectRecord;
}

export function isTimelineOnlyProject(project: StoredProjectRecord): boolean {
  return Boolean(
    project.composition?.timelineOnly ||
      project.segment?.mediaMode === "timelineOnly",
  );
}

export function sortProjectsByDisplayOrder<
  T extends { lastModified: number; createdAt: number },
>(projects: T[]): T[] {
  return [...projects].sort((a, b) => {
    // Keep project cards stable in the grid. The legacy localStorage-backed list
    // preserved insertion order (newest created first) and did not reshuffle when
    // a project was merely edited/opened. Sorting by lastModified breaks FLIP
    // restore targeting because cards swap positions after normal saves.
    return b.createdAt - a.createdAt;
  });
}

export function openProjectDB(): Promise<IDBDatabase> {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, DB_VERSION);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);
      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains(PROJECTS_STORE)) {
          db.createObjectStore(PROJECTS_STORE, { keyPath: "id" });
        }
        for (const storeName of PROJECT_OBJECT_STORES) {
          if (!db.objectStoreNames.contains(storeName)) {
            db.createObjectStore(storeName);
          }
        }
      };
    });
  }
  return dbPromise;
}

export async function idbPut<T>(
  storeName: string,
  value: T,
  key?: IDBValidKey,
): Promise<void> {
  const db = await openProjectDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(storeName, "readwrite");
    const store = tx.objectStore(storeName);
    const request = key === undefined ? store.put(value) : store.put(value, key);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

export async function idbGet<T>(
  storeName: string,
  key: IDBValidKey,
): Promise<T | null> {
  const db = await openProjectDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(storeName, "readonly");
    const request = tx.objectStore(storeName).get(key);
    request.onsuccess = () => resolve((request.result as T) ?? null);
    request.onerror = () => reject(request.error);
  });
}

export async function idbGetAll<T>(storeName: string): Promise<T[]> {
  const db = await openProjectDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(storeName, "readonly");
    const request = tx.objectStore(storeName).getAll();
    request.onsuccess = () => resolve((request.result as T[]) ?? []);
    request.onerror = () => reject(request.error);
  });
}

export async function idbDelete(
  storeName: string,
  key: IDBValidKey,
): Promise<void> {
  const db = await openProjectDB();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(storeName, "readwrite");
    const request = tx.objectStore(storeName).delete(key);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error);
  });
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () =>
      reject(transaction.error ?? new Error("IndexedDB transaction aborted"));
  });
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function abortFailedTransaction(
  transaction: IDBTransaction,
  done: Promise<void>,
  error: unknown,
): Promise<never> {
  try {
    transaction.abort();
  } catch {
    // The browser may already have aborted after a failed request.
  }
  await done.catch(() => undefined);
  throw error;
}

function writeOptionalBlob(
  store: IDBObjectStore,
  key: string,
  value: Blob | null | undefined,
): void {
  if (value) {
    store.put(value, key);
  } else {
    store.delete(key);
  }
}

/** Writes a new project record and all root media in one durable transaction. */
export async function idbCreateProjectBundle(project: Project): Promise<void> {
  const db = await openProjectDB();
  const transaction = db.transaction([...PROJECT_BUNDLE_STORES], "readwrite");
  const done = transactionDone(transaction);
  try {
    transaction.objectStore(PROJECTS_STORE).put(stripHeavyProjectFields(project));
    writeOptionalBlob(transaction.objectStore("videos"), project.id, project.videoBlob);
    writeOptionalBlob(transaction.objectStore("audio"), project.id, project.audioBlob);
    writeOptionalBlob(
      transaction.objectStore("mic_audio"),
      project.id,
      project.micAudioBlob,
    );
    writeOptionalBlob(
      transaction.objectStore("webcam_videos"),
      project.id,
      project.webcamBlob,
    );
    await done;
  } catch (error) {
    await abortFailedTransaction(transaction, done, error);
  }
}

/**
 * Merges an update with the latest record and applies matching root-media
 * changes in the same transaction. Returns null when the project was deleted.
 */
export async function idbUpdateProjectBundle(
  id: string,
  updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
): Promise<StoredProjectRecord | null> {
  const db = await openProjectDB();
  const transaction = db.transaction([...PROJECT_BUNDLE_STORES], "readwrite");
  const done = transactionDone(transaction);
  const projectStore = transaction.objectStore(PROJECTS_STORE);
  const previousProject =
    (await requestResult(projectStore.get(id))) as StoredProjectRecord | undefined;
  if (!previousProject) {
    transaction.abort();
    try {
      await done;
    } catch {
      // The abort is intentional: no writes should be committed for a missing project.
    }
    return null;
  }

  try {
    const mediaUpdates = updates as Partial<ProjectMediaFields>;
    if ("videoBlob" in updates) {
      writeOptionalBlob(transaction.objectStore("videos"), id, mediaUpdates.videoBlob);
    }
    if ("audioBlob" in updates) {
      writeOptionalBlob(transaction.objectStore("audio"), id, mediaUpdates.audioBlob);
    }
    if ("micAudioBlob" in updates) {
      writeOptionalBlob(
        transaction.objectStore("mic_audio"),
        id,
        mediaUpdates.micAudioBlob,
      );
    }
    if ("webcamBlob" in updates) {
      writeOptionalBlob(
        transaction.objectStore("webcam_videos"),
        id,
        mediaUpdates.webcamBlob,
      );
    }

    const nextProject = stripHeavyProjectFields({
      ...previousProject,
      ...updates,
      id,
      createdAt: previousProject.createdAt,
      lastModified: Date.now(),
    } as Project);
    projectStore.put(nextProject);
    await done;
    return nextProject;
  } catch (error) {
    return abortFailedTransaction(transaction, done, error);
  }
}

export interface CompositionAssetBundle {
  videoBlob?: Blob;
  audioBlob?: Blob;
  micAudioBlob?: Blob;
  webcamBlob?: Blob;
  customBackground?: string;
}

function writeCompositionAssetBundle(
  transaction: IDBTransaction,
  key: string,
  data: CompositionAssetBundle,
): void {
  writeOptionalBlob(
    transaction.objectStore("composition_videos"),
    key,
    data.videoBlob,
  );
  writeOptionalBlob(
    transaction.objectStore("composition_audio"),
    key,
    data.audioBlob,
  );
  writeOptionalBlob(
    transaction.objectStore("composition_mic_audio"),
    key,
    data.micAudioBlob,
  );
  writeOptionalBlob(
    transaction.objectStore("composition_webcam_videos"),
    key,
    data.webcamBlob,
  );
  const backgroundStore = transaction.objectStore(
    "composition_custom_backgrounds",
  );
  if (data.customBackground) {
    backgroundStore.put(data.customBackground, key);
  } else {
    backgroundStore.delete(key);
  }
}

export async function idbWriteCompositionAssetBundle(
  key: string,
  data: CompositionAssetBundle,
): Promise<void> {
  const db = await openProjectDB();
  const transaction = db.transaction(
    [...COMPOSITION_ASSET_STORES],
    "readwrite",
  );
  const done = transactionDone(transaction);
  try {
    writeCompositionAssetBundle(transaction, key, data);
    await done;
  } catch (error) {
    await abortFailedTransaction(transaction, done, error);
  }
}

/** Commits a composition edit and its new clip assets as one durable unit. */
export async function idbUpdateProjectWithCompositionAssetBundle(
  id: string,
  clipId: string,
  composition: ProjectComposition,
  data: CompositionAssetBundle,
): Promise<StoredProjectRecord | null> {
  const db = await openProjectDB();
  const transaction = db.transaction(
    [...PROJECT_BUNDLE_STORES, ...COMPOSITION_ASSET_STORES],
    "readwrite",
  );
  const done = transactionDone(transaction);
  const projectStore = transaction.objectStore(PROJECTS_STORE);
  const previousProject =
    (await requestResult(projectStore.get(id))) as StoredProjectRecord | undefined;
  if (!previousProject) {
    transaction.abort();
    await done.catch(() => undefined);
    return null;
  }
  try {
    writeCompositionAssetBundle(
      transaction,
      buildCompositionAssetKey(id, clipId),
      data,
    );
    const nextProject: StoredProjectRecord = {
      ...previousProject,
      composition,
      lastModified: Date.now(),
    };
    projectStore.put(nextProject);
    await done;
    return nextProject;
  } catch (error) {
    return abortFailedTransaction(transaction, done, error);
  }
}

/** Removes all database-owned project records and blobs in one transaction. */
export async function idbDeleteProjectBundle(
  id: string,
  project: Pick<StoredProjectRecord, "composition"> | null | undefined,
): Promise<void> {
  const db = await openProjectDB();
  const transaction = db.transaction([...PROJECT_DELETE_STORES], "readwrite");
  const done = transactionDone(transaction);
  try {
    for (const storeName of PROJECT_BUNDLE_STORES) {
      transaction.objectStore(storeName).delete(id);
    }
    for (const storeName of ["mouse", "thumbnails", "custom_backgrounds", "segments"] as const) {
      transaction.objectStore(storeName).delete(id);
    }
    const clipIds = new Set(
      [
        ...(project?.composition?.clips ?? []),
        ...(project?.composition?.retainedRemovedClips ?? []),
      ]
        .filter((clip) => clip.role !== "root")
        .map((clip) => clip.id),
    );
    for (const clipId of clipIds) {
      const key = buildCompositionAssetKey(id, clipId);
      for (const storeName of COMPOSITION_ASSET_STORES) {
        transaction.objectStore(storeName).delete(key);
      }
    }
    await done;
  } catch (error) {
    await abortFailedTransaction(transaction, done, error);
  }
}

export async function idbDeleteCompositionAssetBundle(
  key: string,
): Promise<void> {
  const db = await openProjectDB();
  const transaction = db.transaction(
    [...COMPOSITION_ASSET_STORES],
    "readwrite",
  );
  const done = transactionDone(transaction);
  try {
    for (const storeName of COMPOSITION_ASSET_STORES) {
      transaction.objectStore(storeName).delete(key);
    }
    await done;
  } catch (error) {
    await abortFailedTransaction(transaction, done, error);
  }
}
