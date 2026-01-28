import { Project } from '@/types/video';

class ProjectManager {
  private readonly STORAGE_KEY = 'screen-demo-projects';
  private limit = 50;

  setLimit(newLimit: number) {
    this.limit = newLimit;
    this.pruneProjects();
  }

  getLimit(): number {
    return this.limit;
  }

  private async pruneProjects() {
    const projects = await this.getProjects();
    if (projects.length > this.limit) {
      const projectsToDelete = projects.splice(this.limit);
      for (const p of projectsToDelete) {
        await this.deleteVideoBlob(p.id);
        await this.deleteAudioBlob(p.id);
        await this.deleteMouseData(p.id); // Prune mouse data
      }
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
    }
  }

  async saveProject(project: Omit<Project, 'id' | 'createdAt' | 'lastModified'>): Promise<Project> {
    const projects = await this.getProjects();

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

    // Store project metadata in localStorage (exclude heavy blobs and mouse data)
    const projectMeta = { ...newProject };
    delete (projectMeta as any).videoBlob;
    delete (projectMeta as any).audioBlob;
    // We keep mousePositions as an empty array in meta to satisfy type, or remove it and re-attach on load
    (projectMeta as any).mousePositions = [];

    projects.unshift(projectMeta);

    // Limit projects
    if (projects.length > this.limit) {
      const projectsToDelete = projects.splice(this.limit);
      for (const p of projectsToDelete) {
        await this.deleteVideoBlob(p.id);
        await this.deleteAudioBlob(p.id);
        await this.deleteMouseData(p.id);
      }
    }

    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));

    // Return full object to caller
    return newProject;
  }

  async getProjects(): Promise<Omit<Project, 'videoBlob' | 'audioBlob'>[]> {
    const projectsJson = localStorage.getItem(this.STORAGE_KEY);
    return projectsJson ? JSON.parse(projectsJson) : [];
  }

  async loadProject(id: string): Promise<Project | null> {
    const projects = await this.getProjects();
    const project = projects.find(p => p.id === id);

    if (!project) return null;

    // Load heavy data from IndexedDB
    const videoBlob = await this.loadVideoBlob(id);
    if (!videoBlob) return null;

    const audioBlob = await this.loadAudioBlob(id);
    const mousePositions = await this.loadMouseData(id) || [];

    return {
      ...project,
      videoBlob,
      audioBlob: audioBlob || undefined,
      mousePositions // Attach loaded positions
    };
  }

  async deleteProject(id: string): Promise<void> {
    const projects = await this.getProjects();
    const filteredProjects = projects.filter(p => p.id !== id);
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(filteredProjects));

    // Delete from IndexedDB
    await this.deleteVideoBlob(id);
    await this.deleteAudioBlob(id);
    await this.deleteMouseData(id);
  }

  async updateProject(id: string, updates: Partial<Omit<Project, 'id' | 'createdAt' | 'lastModified'>>): Promise<void> {
    const projects = await this.getProjects();
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

    projects[projectIndex] = updatedProject;
    localStorage.setItem(this.STORAGE_KEY, JSON.stringify(projects));
  }

  private async openDB(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open('ScreenDemoDB', 3); // Bump version to 3

      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(request.result);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains('videos')) db.createObjectStore('videos');
        if (!db.objectStoreNames.contains('audio')) db.createObjectStore('audio');
        if (!db.objectStoreNames.contains('mouse')) db.createObjectStore('mouse'); // New store
      };
    });
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