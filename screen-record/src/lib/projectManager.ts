import { Project } from "@/types/video";
import { invoke } from "@/lib/ipc";
import { isManagedCompositionSnapshotPath } from "@/lib/mediaServer";

const PROJECT_SWITCH_DEBUG = false;
const DB_NAME = "ScreenDemoDB";
const DB_VERSION = 8;
const PROJECTS_STORE = "projects";
const APP_META_STORE = "app_meta";
const LEGACY_PROJECTS_KEY = "screen-demo-projects";
const PROJECT_MIGRATION_KEY = "projects-storage-migrated-v1";

type StoredProjectRecord = Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob">;

function summarizeProjectUpdate(
  updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
) {
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
    compositionRootBackground: updates.composition?.clips?.find(
      (clip) => clip.id === "root",
    )?.backgroundConfig
      ? {
          backgroundType:
            updates.composition.clips.find((clip) => clip.id === "root")
              ?.backgroundConfig.backgroundType ?? null,
          canvasMode:
            updates.composition.clips.find((clip) => clip.id === "root")
              ?.backgroundConfig.canvasMode ?? "auto",
          canvasWidth:
            updates.composition.clips.find((clip) => clip.id === "root")
              ?.backgroundConfig.canvasWidth ?? null,
          canvasHeight:
            updates.composition.clips.find((clip) => clip.id === "root")
              ?.backgroundConfig.canvasHeight ?? null,
        }
      : null,
  };
}

function summarizeStoredProject(project: StoredProjectRecord | null | undefined) {
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

function buildCompositionAssetKey(projectId: string, clipId: string): string {
  return `${projectId}:${clipId}`;
}

function stripHeavyProjectFields(
  project: Project | StoredProjectRecord,
): StoredProjectRecord {
  const record = { ...project } as Project;
  delete (record as Partial<Project>).videoBlob;
  delete (record as Partial<Project>).audioBlob;
  delete (record as Partial<Project>).micAudioBlob;
  return record as StoredProjectRecord;
}

function sortProjectsByDisplayOrder<
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

class ProjectManager {
  private limit = 50;
  private dbPromise: Promise<IDBDatabase> | null = null;
  private migrationPromise: Promise<void> | null = null;

  setLimit(newLimit: number) {
    this.limit = newLimit;
    void this.pruneProjects();
  }

  getLimit(): number {
    return this.limit;
  }

  async saveProject(
    project: Omit<Project, "id" | "createdAt" | "lastModified">,
  ): Promise<Project> {
    await this.ensureProjectStoreReady();

    const newProject: Project = {
      ...project,
      id: crypto.randomUUID(),
      createdAt: Date.now(),
      lastModified: Date.now(),
    };

    if (newProject.videoBlob) {
      await this.saveVideoBlob(newProject.id, newProject.videoBlob);
    }
    if (newProject.audioBlob) {
      await this.saveAudioBlob(newProject.id, newProject.audioBlob);
    }
    if (newProject.micAudioBlob) {
      await this.saveMicAudioBlob(newProject.id, newProject.micAudioBlob);
    }

    await this.saveProjectRecord(stripHeavyProjectFields(newProject));
    await this.pruneProjects();
    return newProject;
  }

  async getProjects(): Promise<Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob">[]> {
    await this.ensureProjectStoreReady();
    return sortProjectsByDisplayOrder(await this.getProjectRecords());
  }

  async loadProject(id: string): Promise<Project | null> {
    await this.ensureProjectStoreReady();
    const project = await this.loadProjectRecord(id);
    if (!project) return null;

    const videoBlob = await this.loadVideoBlob(id);
    if (!videoBlob && !project.rawVideoPath) return null;

    const audioBlob = await this.loadAudioBlob(id);
    const micAudioBlob = await this.loadMicAudioBlob(id);
    return {
      ...project,
      videoBlob: videoBlob || undefined,
      audioBlob: audioBlob || undefined,
      micAudioBlob: micAudioBlob || undefined,
    };
  }

  async deleteProject(id: string): Promise<void> {
    await this.ensureProjectStoreReady();
    const project = await this.loadProjectRecord(id);
    await this.deleteProjectRecord(id);
    await this.deleteProjectData(id, project);
  }

  async updateProject(
    id: string,
    updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
  ): Promise<void> {
    await this.ensureProjectStoreReady();
    const previousProject = await this.loadProjectRecord(id);
    if (!previousProject) return;

    if (PROJECT_SWITCH_DEBUG) {
      console.warn(
        `[ProjectSwitch] ${JSON.stringify({
          event: "project-manager:update",
          targetProjectId: id,
          prev: summarizeStoredProject(previousProject),
          updates: summarizeProjectUpdate(updates),
          stack: new Error()
            .stack?.split("\n")
            .slice(2, 5)
            .map((line) => line.trim()),
        })}`,
      );
    }

    if ("videoBlob" in updates) {
      if (updates.videoBlob) await this.saveVideoBlob(id, updates.videoBlob);
      else await this.deleteVideoBlob(id);
    }
    if ("audioBlob" in updates) {
      if (updates.audioBlob) await this.saveAudioBlob(id, updates.audioBlob);
      else await this.deleteAudioBlob(id);
    }
    if ("micAudioBlob" in updates) {
      if (updates.micAudioBlob) {
        await this.saveMicAudioBlob(id, updates.micAudioBlob);
      } else {
        await this.deleteMicAudioBlob(id);
      }
    }

    const nextProject: Project = {
      ...previousProject,
      ...updates,
      id,
      createdAt: previousProject.createdAt,
      lastModified: Date.now(),
      videoBlob: undefined,
      audioBlob: undefined,
      micAudioBlob: undefined,
    };

    await this.saveProjectRecord(stripHeavyProjectFields(nextProject));
    await this.pruneProjects();
  }

  async saveCompositionClipAssets(
    projectId: string,
    clipId: string,
    data: {
      videoBlob?: Blob;
      audioBlob?: Blob;
      micAudioBlob?: Blob;
      customBackground?: string;
    },
  ): Promise<void> {
    const key = buildCompositionAssetKey(projectId, clipId);
    if (data.videoBlob) {
      await this.idbPut("composition_videos", data.videoBlob, key);
    } else {
      await this.idbDelete("composition_videos", key);
    }
    if (data.audioBlob) {
      await this.idbPut("composition_audio", data.audioBlob, key);
    } else {
      await this.idbDelete("composition_audio", key);
    }
    if (data.micAudioBlob) {
      await this.idbPut("composition_mic_audio", data.micAudioBlob, key);
    } else {
      await this.idbDelete("composition_mic_audio", key);
    }
    if (data.customBackground) {
      await this.idbPut(
        "composition_custom_backgrounds",
        data.customBackground,
        key,
      );
    } else {
      await this.idbDelete("composition_custom_backgrounds", key);
    }
  }

  async loadCompositionClipAssets(
    projectId: string,
    clipId: string,
  ): Promise<{
    videoBlob: Blob | null;
    audioBlob: Blob | null;
    micAudioBlob: Blob | null;
    customBackground: string | null;
  }> {
    const key = buildCompositionAssetKey(projectId, clipId);
    return {
      videoBlob: await this.loadBlobData("composition_videos", key),
      audioBlob: await this.loadBlobData("composition_audio", key),
      micAudioBlob: await this.loadBlobData("composition_mic_audio", key),
      customBackground: await this.loadStringData(
        "composition_custom_backgrounds",
        key,
      ),
    };
  }

  async deleteCompositionClipAssets(
    projectId: string,
    clipId: string,
  ): Promise<void> {
    const key = buildCompositionAssetKey(projectId, clipId);
    await this.idbDelete("composition_videos", key);
    await this.idbDelete("composition_audio", key);
    await this.idbDelete("composition_mic_audio", key);
    await this.idbDelete("composition_custom_backgrounds", key);
  }

  private async ensureProjectStoreReady(): Promise<void> {
    if (!this.migrationPromise) {
      this.migrationPromise = this.migrateLegacyProjectStorage().finally(() => {
        this.migrationPromise = null;
      });
    }
    await this.migrationPromise;
  }

  private async migrateLegacyProjectStorage(): Promise<void> {
    const migrated = await this.getMetaValue<boolean>(PROJECT_MIGRATION_KEY);
    if (migrated) return;

    const existingRecords = await this.getProjectRecords();
    if (existingRecords.length > 0) {
      await this.setMetaValue(PROJECT_MIGRATION_KEY, true);
      localStorage.removeItem(LEGACY_PROJECTS_KEY);
      return;
    }

    const legacyProjects = this.getLegacyProjectsMeta();
    if (legacyProjects.length === 0) {
      await this.setMetaValue(PROJECT_MIGRATION_KEY, true);
      localStorage.removeItem(LEGACY_PROJECTS_KEY);
      return;
    }

    for (const legacyProject of legacyProjects) {
      if (!legacyProject?.id) continue;
      const record = await this.buildMigratedProjectRecord(legacyProject);
      await this.saveProjectRecord(record);
      await this.deleteLegacyInlineProjectData(legacyProject.id);
    }

    await this.pruneProjects();
    localStorage.removeItem(LEGACY_PROJECTS_KEY);
    await this.setMetaValue(PROJECT_MIGRATION_KEY, true);
  }

  private getLegacyProjectsMeta(): any[] {
    const projectsJson = localStorage.getItem(LEGACY_PROJECTS_KEY);
    if (!projectsJson) return [];
    try {
      const parsed = JSON.parse(projectsJson);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  private async buildMigratedProjectRecord(
    legacyProject: any,
  ): Promise<StoredProjectRecord> {
    const migratedSegment =
      (await this.loadLegacySegmentData(legacyProject.id)) ?? legacyProject.segment;
    const migratedMousePositions =
      (await this.loadLegacyMouseData(legacyProject.id)) ??
      (Array.isArray(legacyProject.mousePositions)
        ? legacyProject.mousePositions
        : []);
    const migratedThumbnail =
      (await this.loadLegacyThumbnailData(legacyProject.id)) ??
      legacyProject.thumbnail ??
      undefined;
    const migratedCustomBackground =
      (await this.loadLegacyCustomBackgroundData(legacyProject.id)) ??
      legacyProject.backgroundConfig?.customBackground ??
      undefined;

    return {
      ...legacyProject,
      mousePositions: migratedMousePositions,
      segment: migratedSegment,
      thumbnail: migratedThumbnail,
      backgroundConfig: legacyProject.backgroundConfig
        ? {
            ...legacyProject.backgroundConfig,
            customBackground: migratedCustomBackground,
          }
        : legacyProject.backgroundConfig,
    } as StoredProjectRecord;
  }

  private async pruneProjects(): Promise<void> {
    const projects = sortProjectsByDisplayOrder(await this.getProjectRecords());
    if (projects.length <= this.limit) return;

    const projectsToDelete = projects.slice(this.limit);
    for (const project of projectsToDelete) {
      await this.deleteProjectRecord(project.id);
      await this.deleteProjectData(project.id, project);
    }
  }

  private async deleteProjectData(
    id: string,
    project: Pick<Project, "composition" | "rawVideoPath"> | null | undefined,
  ): Promise<void> {
    await this.deleteVideoBlob(id);
    await this.deleteAudioBlob(id);
    await this.deleteMicAudioBlob(id);
    await this.deleteLegacyInlineProjectData(id);
    await this.deleteCompositionData(id, project?.composition?.clips);
    await this.deleteCompositionSnapshotFiles(
      project?.composition?.clips,
      project?.rawVideoPath,
    );
  }

  private async deleteLegacyInlineProjectData(id: string): Promise<void> {
    await Promise.all([
      this.idbDelete("mouse", id),
      this.idbDelete("segments", id),
      this.idbDelete("thumbnails", id),
      this.idbDelete("custom_backgrounds", id),
    ]);
  }

  private async getProjectRecords(): Promise<StoredProjectRecord[]> {
    return (await this.idbGetAll<StoredProjectRecord>(PROJECTS_STORE)).filter(
      Boolean,
    );
  }

  private async loadProjectRecord(
    id: string,
  ): Promise<StoredProjectRecord | null> {
    return (await this.idbGet<StoredProjectRecord>(PROJECTS_STORE, id)) ?? null;
  }

  private async saveProjectRecord(project: StoredProjectRecord): Promise<void> {
    await this.idbPut(PROJECTS_STORE, project);
  }

  private async deleteProjectRecord(id: string): Promise<void> {
    await this.idbDelete(PROJECTS_STORE, id);
  }

  private async getMetaValue<T>(key: string): Promise<T | null> {
    return (await this.idbGet<T>(APP_META_STORE, key)) ?? null;
  }

  private async setMetaValue<T>(key: string, value: T): Promise<void> {
    await this.idbPut(APP_META_STORE, value, key);
  }

  private async openDB(): Promise<IDBDatabase> {
    if (!this.dbPromise) {
      this.dbPromise = new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);

        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
        request.onupgradeneeded = (event) => {
          const db = (event.target as IDBOpenDBRequest).result;
          if (!db.objectStoreNames.contains(PROJECTS_STORE)) {
            db.createObjectStore(PROJECTS_STORE, { keyPath: "id" });
          }
          if (!db.objectStoreNames.contains(APP_META_STORE)) {
            db.createObjectStore(APP_META_STORE);
          }
          if (!db.objectStoreNames.contains("videos")) {
            db.createObjectStore("videos");
          }
          if (!db.objectStoreNames.contains("audio")) {
            db.createObjectStore("audio");
          }
          if (!db.objectStoreNames.contains("mic_audio")) {
            db.createObjectStore("mic_audio");
          }
          if (!db.objectStoreNames.contains("mouse")) {
            db.createObjectStore("mouse");
          }
          if (!db.objectStoreNames.contains("thumbnails")) {
            db.createObjectStore("thumbnails");
          }
          if (!db.objectStoreNames.contains("custom_backgrounds")) {
            db.createObjectStore("custom_backgrounds");
          }
          if (!db.objectStoreNames.contains("segments")) {
            db.createObjectStore("segments");
          }
          if (!db.objectStoreNames.contains("composition_videos")) {
            db.createObjectStore("composition_videos");
          }
          if (!db.objectStoreNames.contains("composition_audio")) {
            db.createObjectStore("composition_audio");
          }
          if (!db.objectStoreNames.contains("composition_mic_audio")) {
            db.createObjectStore("composition_mic_audio");
          }
          if (!db.objectStoreNames.contains("composition_custom_backgrounds")) {
            db.createObjectStore("composition_custom_backgrounds");
          }
        };
      });
    }
    return this.dbPromise;
  }

  private async idbPut<T>(
    storeName: string,
    value: T,
    key?: IDBValidKey,
  ): Promise<void> {
    const db = await this.openDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(storeName, "readwrite");
      const store = tx.objectStore(storeName);
      const request = key === undefined ? store.put(value) : store.put(value, key);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  private async idbGet<T>(
    storeName: string,
    key: IDBValidKey,
  ): Promise<T | null> {
    const db = await this.openDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(storeName, "readonly");
      const request = tx.objectStore(storeName).get(key);
      request.onsuccess = () => resolve((request.result as T) ?? null);
      request.onerror = () => reject(request.error);
    });
  }

  private async idbGetAll<T>(storeName: string): Promise<T[]> {
    const db = await this.openDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(storeName, "readonly");
      const request = tx.objectStore(storeName).getAll();
      request.onsuccess = () => resolve((request.result as T[]) ?? []);
      request.onerror = () => reject(request.error);
    });
  }

  private async idbDelete(storeName: string, key: IDBValidKey): Promise<void> {
    const db = await this.openDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(storeName, "readwrite");
      const request = tx.objectStore(storeName).delete(key);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  private async loadLegacySegmentData(id: string): Promise<any | null> {
    return this.idbGet("segments", id);
  }

  private async loadLegacyMouseData(id: string): Promise<any[] | null> {
    return this.idbGet("mouse", id);
  }

  private async loadLegacyThumbnailData(id: string): Promise<string | null> {
    return this.idbGet("thumbnails", id);
  }

  private async loadLegacyCustomBackgroundData(
    id: string,
  ): Promise<string | null> {
    return this.idbGet("custom_backgrounds", id);
  }

  private async loadBlobData(storeName: string, key: string): Promise<Blob | null> {
    return this.idbGet(storeName, key);
  }

  private async loadStringData(
    storeName: string,
    key: string,
  ): Promise<string | null> {
    return this.idbGet(storeName, key);
  }

  private saveVideoBlob(id: string, blob: Blob): Promise<void> {
    return this.idbPut("videos", blob, id);
  }

  private async loadVideoBlob(id: string): Promise<Blob | null> {
    return this.idbGet("videos", id);
  }

  private deleteVideoBlob(id: string): Promise<void> {
    return this.idbDelete("videos", id);
  }

  private saveAudioBlob(id: string, blob: Blob): Promise<void> {
    return this.idbPut("audio", blob, id);
  }

  private async loadAudioBlob(id: string): Promise<Blob | null> {
    return this.idbGet("audio", id);
  }

  private deleteAudioBlob(id: string): Promise<void> {
    return this.idbDelete("audio", id);
  }

  private saveMicAudioBlob(id: string, blob: Blob): Promise<void> {
    return this.idbPut("mic_audio", blob, id);
  }

  private async loadMicAudioBlob(id: string): Promise<Blob | null> {
    return this.idbGet("mic_audio", id);
  }

  private deleteMicAudioBlob(id: string): Promise<void> {
    return this.idbDelete("mic_audio", id);
  }

  private async deleteCompositionData(
    projectId: string,
    clips: Array<{ id: string; role?: string }> | undefined,
  ): Promise<void> {
    if (!Array.isArray(clips)) return;
    for (const clip of clips) {
      if (!clip || clip.role === "root") continue;
      await this.deleteCompositionClipAssets(projectId, clip.id);
    }
  }

  private async deleteCompositionSnapshotFiles(
    clips:
      | Array<{ id?: string; role?: string; rawVideoPath?: string }>
      | undefined,
    rootRawVideoPath?: string,
  ): Promise<void> {
    if (!Array.isArray(clips)) return;
    for (const clip of clips) {
      if (
        !clip ||
        clip.role !== "snapshot" ||
        !clip.rawVideoPath ||
        clip.rawVideoPath === rootRawVideoPath ||
        !isManagedCompositionSnapshotPath(clip.rawVideoPath)
      ) {
        continue;
      }
      try {
        await invoke("delete_file", { path: clip.rawVideoPath });
      } catch {
        // ignore cleanup failures for orphaned snapshot files
      }
    }
  }
}

export const projectManager = new ProjectManager();
