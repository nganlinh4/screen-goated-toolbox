import type { Project } from "@/types/video";

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

export type StoredProjectRecord = Omit<
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
