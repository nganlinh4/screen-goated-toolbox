import { Project } from "@/types/video";
import { invoke } from "@/lib/ipc";
import { isManagedCompositionSnapshotPath } from "@/lib/mediaServer";

function buildCompositionAssetKey(projectId: string, clipId: string): string {
  return `${projectId}:${clipId}`;
}

class ProjectManager {
  private readonly STORAGE_KEY = "screen-demo-projects";
  private limit = 50;
  private compactionPromise: Promise<void> | null = null;

  private getProjectsMetaSync(): any[] {
    const projectsJson = localStorage.getItem(this.STORAGE_KEY);
    if (!projectsJson) return [];
    try {
      const parsed = JSON.parse(projectsJson);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  setLimit(newLimit: number) {
    this.limit = newLimit;
    this.pruneProjects();
  }

  getLimit(): number {
    return this.limit;
  }

  private async pruneProjects() {
    const projects = this.getProjectsMetaSync();
    if (projects.length > this.limit) {
      const projectsToDelete = projects.splice(this.limit);
      for (const p of projectsToDelete) {
        await this.deleteVideoBlob(p.id);
        await this.deleteAudioBlob(p.id);
        await this.deleteMouseData(p.id);
        await this.deleteSegmentData(p.id);
        await this.deleteThumbnailData(p.id);
        await this.deleteCustomBackgroundData(p.id);
        await this.deleteCompositionData(p.id, p.composition?.clips);
        await this.deleteCompositionSnapshotFiles(
          p.composition?.clips,
          p.rawVideoPath,
        );
      }
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    }
  }

  async saveProject(
    project: Omit<Project, "id" | "createdAt" | "lastModified">,
  ): Promise<Project> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();

    const newProject: Project = {
      ...project,
      id: crypto.randomUUID(),
      createdAt: Date.now(),
      lastModified: Date.now(),
    };

    // Store heavy data in IndexedDB
    if (newProject.videoBlob) {
      await this.saveVideoBlob(newProject.id, newProject.videoBlob);
    }
    if (newProject.audioBlob) {
      await this.saveAudioBlob(newProject.id, newProject.audioBlob);
    }
    await this.saveMouseData(newProject.id, newProject.mousePositions);
    await this.saveSegmentData(newProject.id, newProject.segment);
    if (newProject.thumbnail) {
      await this.saveThumbnailData(newProject.id, newProject.thumbnail);
    }
    const customBackground = newProject.backgroundConfig?.customBackground;
    if (customBackground) {
      await this.saveCustomBackgroundData(newProject.id, customBackground);
    }

    // Store project metadata in localStorage (exclude heavy blobs and arrays)
    const projectMeta = { ...newProject };
    delete (projectMeta as any).videoBlob;
    delete (projectMeta as any).audioBlob;
    (projectMeta as any).mousePositions = [];
    delete (projectMeta as any).segment;
    delete (projectMeta as any).thumbnail;
    if ((projectMeta as any).backgroundConfig) {
      (projectMeta as any).backgroundConfig = {
        ...(projectMeta as any).backgroundConfig,
        customBackground: undefined,
      };
    }

    projects.unshift(projectMeta);

    // Limit projects
    if (projects.length > this.limit) {
      const projectsToDelete = projects.splice(this.limit);
      for (const p of projectsToDelete) {
        await this.deleteVideoBlob(p.id);
        await this.deleteAudioBlob(p.id);
        await this.deleteMouseData(p.id);
        await this.deleteSegmentData(p.id);
        await this.deleteThumbnailData(p.id);
        await this.deleteCustomBackgroundData(p.id);
        await this.deleteCompositionData(p.id, p.composition?.clips);
      }
    }

    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    return newProject;
  }

  async getProjects(): Promise<Omit<Project, "videoBlob" | "audioBlob">[]> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const hydrated = await Promise.all(
      projects.map(async (p: any) => ({
        ...p,
        thumbnail: (await this.loadThumbnailData(p.id)) || undefined,
      })),
    );
    return hydrated;
  }

  async loadProject(id: string): Promise<Project | null> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const project = projects.find((p) => p.id === id);

    if (!project) return null;

    // Load heavy data from IndexedDB
    const videoBlob = await this.loadVideoBlob(id);
    if (!videoBlob && !project.rawVideoPath) return null;

    const audioBlob = await this.loadAudioBlob(id);
    const mousePositions = (await this.loadMouseData(id)) || [];
    const segment = await this.loadSegmentData(id);
    const thumbnail = (await this.loadThumbnailData(id)) || undefined;
    const customBackground =
      (await this.loadCustomBackgroundData(id)) || undefined;

    return {
      ...project,
      videoBlob: videoBlob || undefined,
      audioBlob: audioBlob || undefined,
      mousePositions,
      segment: segment || project.segment,
      thumbnail,
      backgroundConfig: {
        ...project.backgroundConfig,
        customBackground,
      },
    };
  }

  async deleteProject(id: string): Promise<void> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const project = projects.find((p) => p.id === id);
    const filteredProjects = projects.filter((p) => p.id !== id);
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(filteredProjects));

    await this.deleteVideoBlob(id);
    await this.deleteAudioBlob(id);
    await this.deleteMouseData(id);
    await this.deleteSegmentData(id);
    await this.deleteThumbnailData(id);
    await this.deleteCustomBackgroundData(id);
    await this.deleteCompositionData(id, project?.composition?.clips);
    await this.deleteCompositionSnapshotFiles(
      project?.composition?.clips,
      project?.rawVideoPath,
    );
  }

  async updateProject(
    id: string,
    updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
  ): Promise<void> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const projectIndex = projects.findIndex((p) => p.id === id);

    if (projectIndex === -1) return;

    if (updates.videoBlob) {
      await this.saveVideoBlob(id, updates.videoBlob);
    }
    if ("audioBlob" in updates) {
      if (updates.audioBlob) await this.saveAudioBlob(id, updates.audioBlob);
      else await this.deleteAudioBlob(id);
    }
    if (Array.isArray(updates.mousePositions)) {
      await this.saveMouseData(id, updates.mousePositions);
    }
    if (updates.segment) {
      await this.saveSegmentData(id, updates.segment);
    }
    if (updates.thumbnail !== undefined) {
      if (updates.thumbnail)
        await this.saveThumbnailData(id, updates.thumbnail);
      else await this.deleteThumbnailData(id);
    }
    if ("backgroundConfig" in updates) {
      const customBackground = updates.backgroundConfig?.customBackground;
      if (typeof customBackground === "string" && customBackground.length > 0) {
        await this.saveCustomBackgroundData(id, customBackground);
      } else {
        await this.deleteCustomBackgroundData(id);
      }
    }

    // Update metadata
    const updatedProject = {
      ...projects[projectIndex],
      ...updates,
      lastModified: Date.now(),
    };

    delete (updatedProject as any).videoBlob;
    delete (updatedProject as any).audioBlob;
    (updatedProject as any).mousePositions = [];
    delete (updatedProject as any).segment;
    delete (updatedProject as any).thumbnail;
    if ((updatedProject as any).backgroundConfig) {
      (updatedProject as any).backgroundConfig = {
        ...(updatedProject as any).backgroundConfig,
        customBackground: undefined,
      };
    }

    projects[projectIndex] = updatedProject;
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
  }

  private async openDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("ScreenDemoDB", 6);

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains("videos"))
          db.createObjectStore("videos");
        if (!db.objectStoreNames.contains("audio"))
          db.createObjectStore("audio");
        if (!db.objectStoreNames.contains("mouse"))
          db.createObjectStore("mouse");
        if (!db.objectStoreNames.contains("thumbnails"))
          db.createObjectStore("thumbnails");
        if (!db.objectStoreNames.contains("custom_backgrounds"))
          db.createObjectStore("custom_backgrounds");
        if (!db.objectStoreNames.contains("segments"))
          db.createObjectStore("segments");
        if (!db.objectStoreNames.contains("composition_videos"))
          db.createObjectStore("composition_videos");
        if (!db.objectStoreNames.contains("composition_audio"))
          db.createObjectStore("composition_audio");
        if (!db.objectStoreNames.contains("composition_custom_backgrounds"))
          db.createObjectStore("composition_custom_backgrounds");
      };
    });
  }

  private async ensureStorageCompacted(): Promise<void> {
    if (!this.compactionPromise) {
      this.compactionPromise = this.compactLegacyLocalStorage().finally(() => {
        this.compactionPromise = null;
      });
    }
    await this.compactionPromise;
  }

  private async compactLegacyLocalStorage(): Promise<void> {
    const projectsJson = localStorage.getItem(this.STORAGE_KEY);
    if (!projectsJson) return;

    let projects: any[] = [];
    try {
      projects = JSON.parse(projectsJson);
      if (!Array.isArray(projects)) return;
    } catch {
      return;
    }

    let changed = false;
    for (const p of projects) {
      if (!p || !p.id) continue;
      if (typeof p.thumbnail === "string" && p.thumbnail.length > 0) {
        await this.saveThumbnailData(p.id, p.thumbnail);
        delete p.thumbnail;
        changed = true;
      }
      const customBackground = p.backgroundConfig?.customBackground;
      if (typeof customBackground === "string" && customBackground.length > 0) {
        await this.saveCustomBackgroundData(p.id, customBackground);
        p.backgroundConfig = {
          ...p.backgroundConfig,
          customBackground: undefined,
        };
        changed = true;
      }
      if (p.segment) {
        await this.saveSegmentData(p.id, p.segment);
        delete p.segment;
        changed = true;
      }
      // Migrate mousePositions: older versions stored them inline in localStorage metadata.
      // Only migrate if there's actual data (not the [] placeholder written by saveProject).
      if (Array.isArray(p.mousePositions) && p.mousePositions.length > 0) {
        const existing = await this.loadMouseData(p.id);
        if (!existing || existing.length === 0) {
          await this.saveMouseData(p.id, p.mousePositions);
        }
        p.mousePositions = [];
        changed = true;
      }
    }

    if (changed) {
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    }
  }

  // --- LOW-LEVEL IDB HELPERS ---
  // IDBRequest is NOT a Promise/thenable — `await store.put(...)` resolves immediately
  // without waiting for the write to commit. These helpers wrap writes/deletes in real
  // Promises that resolve only after onsuccess fires, making them properly awaitable.
  private idbPut<T>(storeName: string, value: T, key: string): Promise<void> {
    return this.openDB().then(
      (db) =>
        new Promise<void>((resolve, reject) => {
          const tx = db.transaction(storeName, "readwrite");
          const request = tx.objectStore(storeName).put(value, key);
          request.onsuccess = () => resolve();
          request.onerror = () => reject(request.error);
        }),
    );
  }

  private idbDelete(storeName: string, key: string): Promise<void> {
    return this.openDB().then(
      (db) =>
        new Promise<void>((resolve, reject) => {
          const tx = db.transaction(storeName, "readwrite");
          const request = tx.objectStore(storeName).delete(key);
          request.onsuccess = () => resolve();
          request.onerror = () => reject(request.error);
        }),
    );
  }

  // --- SEGMENT DATA HELPERS ---
  private saveSegmentData(id: string, data: any): Promise<void> {
    return this.idbPut("segments", data, id);
  }

  private async loadSegmentData(id: string): Promise<any | null> {
    const db = await this.openDB();
    const tx = db.transaction("segments", "readonly");
    const store = tx.objectStore("segments");
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result ?? null);
    });
  }

  private deleteSegmentData(id: string): Promise<void> {
    return this.idbDelete("segments", id);
  }

  // --- MOUSE DATA HELPERS ---
  private saveMouseData(id: string, data: any[]): Promise<void> {
    return this.idbPut("mouse", data, id);
  }

  private async loadMouseData(id: string): Promise<any[] | null> {
    const db = await this.openDB();
    const tx = db.transaction("mouse", "readonly");
    const store = tx.objectStore("mouse");
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as any[]);
    });
  }

  private deleteMouseData(id: string): Promise<void> {
    return this.idbDelete("mouse", id);
  }

  // --- THUMBNAIL DATA HELPERS ---
  private saveThumbnailData(id: string, data: string): Promise<void> {
    return this.idbPut("thumbnails", data, id);
  }

  private async loadThumbnailData(id: string): Promise<string | null> {
    const db = await this.openDB();
    const tx = db.transaction("thumbnails", "readonly");
    const store = tx.objectStore("thumbnails");
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as string);
    });
  }

  private deleteThumbnailData(id: string): Promise<void> {
    return this.idbDelete("thumbnails", id);
  }

  // --- CUSTOM BACKGROUND DATA HELPERS ---
  private saveCustomBackgroundData(id: string, data: string): Promise<void> {
    return this.idbPut("custom_backgrounds", data, id);
  }

  private async loadCustomBackgroundData(id: string): Promise<string | null> {
    const db = await this.openDB();
    const tx = db.transaction("custom_backgrounds", "readonly");
    const store = tx.objectStore("custom_backgrounds");
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as string);
    });
  }

  private deleteCustomBackgroundData(id: string): Promise<void> {
    return this.idbDelete("custom_backgrounds", id);
  }

  async saveCompositionClipAssets(
    projectId: string,
    clipId: string,
    data: { videoBlob?: Blob; audioBlob?: Blob; customBackground?: string },
  ): Promise<void> {
    const key = buildCompositionAssetKey(projectId, clipId);
    if (data.videoBlob) {
      await this.idbPut("composition_videos", data.videoBlob, key);
    } else {
      await this.idbDelete("composition_videos", key);
    }
    if (data.audioBlob)
      await this.idbPut("composition_audio", data.audioBlob, key);
    else await this.idbDelete("composition_audio", key);
    if (data.customBackground)
      await this.idbPut(
        "composition_custom_backgrounds",
        data.customBackground,
        key,
      );
    else await this.idbDelete("composition_custom_backgrounds", key);
  }

  async loadCompositionClipAssets(
    projectId: string,
    clipId: string,
  ): Promise<{
    videoBlob: Blob | null;
    audioBlob: Blob | null;
    customBackground: string | null;
  }> {
    const key = buildCompositionAssetKey(projectId, clipId);
    return {
      videoBlob: await this.loadBlobData("composition_videos", key),
      audioBlob: await this.loadBlobData("composition_audio", key),
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
    await this.idbDelete("composition_custom_backgrounds", key);
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

  private async loadBlobData(
    storeName: string,
    key: string,
  ): Promise<Blob | null> {
    const db = await this.openDB();
    const tx = db.transaction(storeName, "readonly");
    const store = tx.objectStore(storeName);
    return new Promise((resolve) => {
      const request = store.get(key);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve((request.result as Blob) ?? null);
    });
  }

  private async loadStringData(
    storeName: string,
    key: string,
  ): Promise<string | null> {
    const db = await this.openDB();
    const tx = db.transaction(storeName, "readonly");
    const store = tx.objectStore(storeName);
    return new Promise((resolve) => {
      const request = store.get(key);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve((request.result as string) ?? null);
    });
  }

  // --- EXISTING HELPERS ---
  private saveVideoBlob(id: string, blob: Blob): Promise<void> {
    return this.idbPut("videos", blob, id);
  }

  private async loadVideoBlob(id: string): Promise<Blob | null> {
    const db = await this.openDB();
    const tx = db.transaction("videos", "readonly");
    const store = tx.objectStore("videos");
    return new Promise((resolve, reject) => {
      const request = store.get(id);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result as Blob);
    });
  }

  private deleteVideoBlob(id: string): Promise<void> {
    return this.idbDelete("videos", id);
  }

  private saveAudioBlob(id: string, blob: Blob): Promise<void> {
    return this.idbPut("audio", blob, id);
  }

  private async loadAudioBlob(id: string): Promise<Blob | null> {
    const db = await this.openDB();
    const tx = db.transaction("audio", "readonly");
    const store = tx.objectStore("audio");
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as Blob);
    });
  }

  private deleteAudioBlob(id: string): Promise<void> {
    return this.idbDelete("audio", id);
  }
}

export const projectManager = new ProjectManager();
