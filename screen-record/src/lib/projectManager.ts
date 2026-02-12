import { Project } from '@/types/video';

class ProjectManager {
  private readonly STORAGE_KEY = 'screen-demo-projects';
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
        await this.deleteMouseData(p.id); // Prune mouse data
        await this.deleteThumbnailData(p.id);
        await this.deleteCustomBackgroundData(p.id);
      }
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    }
  }

  async saveProject(project: Omit<Project, 'id' | 'createdAt' | 'lastModified'>): Promise<Project> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();

    const newProject: Project = {
      ...project,
      id: crypto.randomUUID(),
      createdAt: Date.now(),
      lastModified: Date.now(),
    };

    // Store heavy data in IndexedDB
    await this.saveVideoBlob(newProject.id, newProject.videoBlob);
    if (newProject.audioBlob) {
      await this.saveAudioBlob(newProject.id, newProject.audioBlob);
    }
    // FIX: Store mouse positions in IDB
    await this.saveMouseData(newProject.id, newProject.mousePositions);
    if (newProject.thumbnail) {
      await this.saveThumbnailData(newProject.id, newProject.thumbnail);
    }
    const customBackground = newProject.backgroundConfig?.customBackground;
    if (customBackground) {
      await this.saveCustomBackgroundData(newProject.id, customBackground);
    }

    // Store project metadata in localStorage (exclude heavy blobs and mouse data)
    const projectMeta = { ...newProject };
    delete (projectMeta as any).videoBlob;
    delete (projectMeta as any).audioBlob;
    // We keep mousePositions as an empty array in meta to satisfy type, or remove it and re-attach on load
    (projectMeta as any).mousePositions = [];
    // Keep thumbnails/custom background in IndexedDB to avoid localStorage quota exhaustion
    delete (projectMeta as any).thumbnail;
    if ((projectMeta as any).backgroundConfig) {
      (projectMeta as any).backgroundConfig = {
        ...(projectMeta as any).backgroundConfig,
        customBackground: undefined
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
        await this.deleteThumbnailData(p.id);
        await this.deleteCustomBackgroundData(p.id);
      }
    }

    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));

    // Return full object to caller
    return newProject;
  }

  async getProjects(): Promise<Omit<Project, 'videoBlob' | 'audioBlob'>[]> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const hydrated = await Promise.all(projects.map(async (p: any) => ({
      ...p,
      thumbnail: (await this.loadThumbnailData(p.id)) || undefined
    })));
    return hydrated;
  }

  async loadProject(id: string): Promise<Project | null> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const project = projects.find(p => p.id === id);

    if (!project) return null;

    // Load heavy data from IndexedDB
    const videoBlob = await this.loadVideoBlob(id);
    if (!videoBlob) return null;

    const audioBlob = await this.loadAudioBlob(id);
    const mousePositions = await this.loadMouseData(id) || [];
    const thumbnail = await this.loadThumbnailData(id) || undefined;
    const customBackground = await this.loadCustomBackgroundData(id) || undefined;

    return {
      ...project,
      videoBlob,
      audioBlob: audioBlob || undefined,
      mousePositions, // Attach loaded positions
      thumbnail,
      backgroundConfig: {
        ...project.backgroundConfig,
        customBackground
      }
    };
  }

  async deleteProject(id: string): Promise<void> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const filteredProjects = projects.filter(p => p.id !== id);
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(filteredProjects));

    // Delete from IndexedDB
    await this.deleteVideoBlob(id);
    await this.deleteAudioBlob(id);
    await this.deleteMouseData(id);
    await this.deleteThumbnailData(id);
    await this.deleteCustomBackgroundData(id);
  }

  async updateProject(id: string, updates: Partial<Omit<Project, 'id' | 'createdAt' | 'lastModified'>>): Promise<void> {
    await this.ensureStorageCompacted();
    const projects = this.getProjectsMetaSync();
    const projectIndex = projects.findIndex(p => p.id === id);

    if (projectIndex === -1) return;

    if (updates.videoBlob) {
      await this.saveVideoBlob(id, updates.videoBlob);
    }
    if (updates.audioBlob) {
      await this.saveAudioBlob(id, updates.audioBlob);
    }
    if (updates.mousePositions) {
      await this.saveMouseData(id, updates.mousePositions);
    }
    if (updates.thumbnail !== undefined) {
      if (updates.thumbnail) await this.saveThumbnailData(id, updates.thumbnail);
      else await this.deleteThumbnailData(id);
    }
    if (updates.backgroundConfig?.customBackground !== undefined) {
      if (updates.backgroundConfig.customBackground) {
        await this.saveCustomBackgroundData(id, updates.backgroundConfig.customBackground);
      } else {
        await this.deleteCustomBackgroundData(id);
      }
    }

    // Update metadata
    const updatedProject = {
      ...projects[projectIndex],
      ...updates,
      lastModified: Date.now()
    };

    // Clean heavy props
    delete (updatedProject as any).videoBlob;
    delete (updatedProject as any).audioBlob;
    (updatedProject as any).mousePositions = [];
    delete (updatedProject as any).thumbnail;
    if ((updatedProject as any).backgroundConfig) {
      (updatedProject as any).backgroundConfig = {
        ...(updatedProject as any).backgroundConfig,
        customBackground: undefined
      };
    }

    projects[projectIndex] = updatedProject;
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
  }

  private async openDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open('ScreenDemoDB', 4); // Bump version to 4

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains('videos')) db.createObjectStore('videos');
        if (!db.objectStoreNames.contains('audio')) db.createObjectStore('audio');
        if (!db.objectStoreNames.contains('mouse')) db.createObjectStore('mouse'); // New store
        if (!db.objectStoreNames.contains('thumbnails')) db.createObjectStore('thumbnails');
        if (!db.objectStoreNames.contains('custom_backgrounds')) db.createObjectStore('custom_backgrounds');
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
      if (typeof p.thumbnail === 'string' && p.thumbnail.length > 0) {
        await this.saveThumbnailData(p.id, p.thumbnail);
        delete p.thumbnail;
        changed = true;
      }
      const customBackground = p.backgroundConfig?.customBackground;
      if (typeof customBackground === 'string' && customBackground.length > 0) {
        await this.saveCustomBackgroundData(p.id, customBackground);
        p.backgroundConfig = { ...p.backgroundConfig, customBackground: undefined };
        changed = true;
      }
    }

    if (changed) {
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    }
  }

  // --- MOUSE DATA HELPERS ---
  private async saveMouseData(id: string, data: any[]): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('mouse', 'readwrite');
    const store = tx.objectStore('mouse');
    await store.put(data, id);
  }

  private async loadMouseData(id: string): Promise<any[] | null> {
    const db = await this.openDB();
    const tx = db.transaction('mouse', 'readonly');
    const store = tx.objectStore('mouse');
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as any[]);
    });
  }

  private async deleteMouseData(id: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('mouse', 'readwrite');
    const store = tx.objectStore('mouse');
    await store.delete(id);
  }

  // --- THUMBNAIL DATA HELPERS ---
  private async saveThumbnailData(id: string, data: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('thumbnails', 'readwrite');
    const store = tx.objectStore('thumbnails');
    await store.put(data, id);
  }

  private async loadThumbnailData(id: string): Promise<string | null> {
    const db = await this.openDB();
    const tx = db.transaction('thumbnails', 'readonly');
    const store = tx.objectStore('thumbnails');
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as string);
    });
  }

  private async deleteThumbnailData(id: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('thumbnails', 'readwrite');
    const store = tx.objectStore('thumbnails');
    await store.delete(id);
  }

  // --- CUSTOM BACKGROUND DATA HELPERS ---
  private async saveCustomBackgroundData(id: string, data: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('custom_backgrounds', 'readwrite');
    const store = tx.objectStore('custom_backgrounds');
    await store.put(data, id);
  }

  private async loadCustomBackgroundData(id: string): Promise<string | null> {
    const db = await this.openDB();
    const tx = db.transaction('custom_backgrounds', 'readonly');
    const store = tx.objectStore('custom_backgrounds');
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as string);
    });
  }

  private async deleteCustomBackgroundData(id: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('custom_backgrounds', 'readwrite');
    const store = tx.objectStore('custom_backgrounds');
    await store.delete(id);
  }

  // --- EXISTING HELPERS ---
  private async saveVideoBlob(id: string, blob: Blob): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('videos', 'readwrite');
    const store = tx.objectStore('videos');
    await store.put(blob, id);
  }

  private async loadVideoBlob(id: string): Promise<Blob | null> {
    const db = await this.openDB();
    const tx = db.transaction('videos', 'readonly');
    const store = tx.objectStore('videos');
    return new Promise((resolve, reject) => {
      const request = store.get(id);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result as Blob);
    });
  }

  private async deleteVideoBlob(id: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('videos', 'readwrite');
    const store = tx.objectStore('videos');
    await store.delete(id);
  }

  private async saveAudioBlob(id: string, blob: Blob): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('audio', 'readwrite');
    const store = tx.objectStore('audio');
    await store.put(blob, id);
  }

  private async loadAudioBlob(id: string): Promise<Blob | null> {
    const db = await this.openDB();
    const tx = db.transaction('audio', 'readonly');
    const store = tx.objectStore('audio');
    return new Promise((resolve) => {
      const request = store.get(id);
      request.onerror = () => resolve(null);
      request.onsuccess = () => resolve(request.result as Blob);
    });
  }

  private async deleteAudioBlob(id: string): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('audio', 'readwrite');
    const store = tx.objectStore('audio');
    await store.delete(id);
  }
}

export const projectManager = new ProjectManager();
