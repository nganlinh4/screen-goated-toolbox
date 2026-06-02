import type { Project } from "@/types/video";
import { invoke } from "@/lib/ipc";
import { isManagedCompositionSnapshotPath } from "@/lib/mediaServer";
import {
  APP_META_STORE,
  buildCompositionAssetKey,
  idbDelete,
  idbGet,
  idbGetAll,
  idbPut,
  isTimelineOnlyProject,
  LEGACY_PROJECTS_KEY,
  PROJECT_MIGRATION_KEY,
  PROJECT_SWITCH_DEBUG,
  PROJECTS_STORE,
  sortProjectsByDisplayOrder,
  stripHeavyProjectFields,
  summarizeProjectUpdate,
  summarizeStoredProject,
  type StoredProjectRecord,
} from "@/lib/projectStorage";

class ProjectManager {
  private limit = 50;
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
    if (newProject.webcamBlob) {
      await this.saveWebcamBlob(newProject.id, newProject.webcamBlob);
    }

    await this.saveProjectRecord(stripHeavyProjectFields(newProject));
    await this.pruneProjects();
    return newProject;
  }

  async getProjects(): Promise<
    Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob">[]
  > {
    await this.ensureProjectStoreReady();
    return sortProjectsByDisplayOrder(await this.getProjectRecords());
  }

  async loadProject(id: string): Promise<Project | null> {
    await this.ensureProjectStoreReady();
    const project = await this.loadProjectRecord(id);
    if (!project) return null;

    const videoBlob = await this.loadVideoBlob(id);
    if (!videoBlob && !project.rawVideoPath && !isTimelineOnlyProject(project)) {
      return null;
    }

    const audioBlob = await this.loadAudioBlob(id);
    const micAudioBlob = await this.loadMicAudioBlob(id);
    const webcamBlob = await this.loadWebcamBlob(id);
    return {
      ...project,
      videoBlob: videoBlob || undefined,
      audioBlob: audioBlob || undefined,
      micAudioBlob: micAudioBlob || undefined,
      webcamBlob: webcamBlob || undefined,
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
    if ("webcamBlob" in updates) {
      if (updates.webcamBlob) {
        await this.saveWebcamBlob(id, updates.webcamBlob);
      } else {
        await this.deleteWebcamBlob(id);
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
      webcamBlob: undefined,
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
      webcamBlob?: Blob;
      customBackground?: string;
    },
  ): Promise<void> {
    const key = buildCompositionAssetKey(projectId, clipId);
    if (data.videoBlob) {
      await idbPut("composition_videos", data.videoBlob, key);
    } else {
      await idbDelete("composition_videos", key);
    }
    if (data.audioBlob) {
      await idbPut("composition_audio", data.audioBlob, key);
    } else {
      await idbDelete("composition_audio", key);
    }
    if (data.micAudioBlob) {
      await idbPut("composition_mic_audio", data.micAudioBlob, key);
    } else {
      await idbDelete("composition_mic_audio", key);
    }
    if (data.webcamBlob) {
      await idbPut("composition_webcam_videos", data.webcamBlob, key);
    } else {
      await idbDelete("composition_webcam_videos", key);
    }
    if (data.customBackground) {
      await idbPut(
        "composition_custom_backgrounds",
        data.customBackground,
        key,
      );
    } else {
      await idbDelete("composition_custom_backgrounds", key);
    }
  }

  async loadCompositionClipAssets(
    projectId: string,
    clipId: string,
  ): Promise<{
    videoBlob: Blob | null;
    audioBlob: Blob | null;
    micAudioBlob: Blob | null;
    webcamBlob: Blob | null;
    customBackground: string | null;
  }> {
    const key = buildCompositionAssetKey(projectId, clipId);
    return {
      videoBlob: await this.loadBlobData("composition_videos", key),
      audioBlob: await this.loadBlobData("composition_audio", key),
      micAudioBlob: await this.loadBlobData("composition_mic_audio", key),
      webcamBlob: await this.loadBlobData("composition_webcam_videos", key),
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
    await idbDelete("composition_videos", key);
    await idbDelete("composition_audio", key);
    await idbDelete("composition_mic_audio", key);
    await idbDelete("composition_webcam_videos", key);
    await idbDelete("composition_custom_backgrounds", key);
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
    project: Project | StoredProjectRecord | null | undefined,
  ): Promise<void> {
    await this.deleteVideoBlob(id);
    await this.deleteAudioBlob(id);
    await this.deleteMicAudioBlob(id);
    await this.deleteWebcamBlob(id);
    await this.deleteLegacyInlineProjectData(id);
    await this.deleteCompositionData(id, project?.composition?.clips);
    await this.deleteCompositionSnapshotFiles(
      project?.composition?.clips,
      project?.rawVideoPath,
      project?.rawWebcamVideoPath,
    );
    // Delete the root raw media files too (lived in `recordings/`).
    // Previously only ProjectsView.tsx did this, and it only covered
    // rawVideoPath — rawWebcamVideoPath and rawMicAudioPath were leaking.
    const rootRawPaths = [
      project?.rawVideoPath,
      project?.rawWebcamVideoPath,
      (project as Project | null | undefined)?.rawMicAudioPath,
    ];
    for (const path of rootRawPaths) {
      if (!path) continue;
      try {
        await invoke("delete_file", { path });
      } catch {
        // ignore cleanup failures
      }
    }
  }

  private async deleteLegacyInlineProjectData(id: string): Promise<void> {
    await Promise.all([
      idbDelete("mouse", id),
      idbDelete("segments", id),
      idbDelete("thumbnails", id),
      idbDelete("custom_backgrounds", id),
    ]);
  }

  private async getProjectRecords(): Promise<StoredProjectRecord[]> {
    return (await idbGetAll<StoredProjectRecord>(PROJECTS_STORE)).filter(
      Boolean,
    );
  }

  private async loadProjectRecord(
    id: string,
  ): Promise<StoredProjectRecord | null> {
    return (await idbGet<StoredProjectRecord>(PROJECTS_STORE, id)) ?? null;
  }

  private async saveProjectRecord(project: StoredProjectRecord): Promise<void> {
    await idbPut(PROJECTS_STORE, project);
  }

  private async deleteProjectRecord(id: string): Promise<void> {
    await idbDelete(PROJECTS_STORE, id);
  }

  private async getMetaValue<T>(key: string): Promise<T | null> {
    return (await idbGet<T>(APP_META_STORE, key)) ?? null;
  }

  private async setMetaValue<T>(key: string, value: T): Promise<void> {
    await idbPut(APP_META_STORE, value, key);
  }

  private async loadLegacySegmentData(id: string): Promise<any | null> {
    return idbGet("segments", id);
  }

  private async loadLegacyMouseData(id: string): Promise<any[] | null> {
    return idbGet("mouse", id);
  }

  private async loadLegacyThumbnailData(id: string): Promise<string | null> {
    return idbGet("thumbnails", id);
  }

  private async loadLegacyCustomBackgroundData(
    id: string,
  ): Promise<string | null> {
    return idbGet("custom_backgrounds", id);
  }

  private async loadBlobData(storeName: string, key: string): Promise<Blob | null> {
    return idbGet(storeName, key);
  }

  private async loadStringData(
    storeName: string,
    key: string,
  ): Promise<string | null> {
    return idbGet(storeName, key);
  }

  private saveVideoBlob(id: string, blob: Blob): Promise<void> {
    return idbPut("videos", blob, id);
  }

  private async loadVideoBlob(id: string): Promise<Blob | null> {
    return idbGet("videos", id);
  }

  private deleteVideoBlob(id: string): Promise<void> {
    return idbDelete("videos", id);
  }

  private saveAudioBlob(id: string, blob: Blob): Promise<void> {
    return idbPut("audio", blob, id);
  }

  private async loadAudioBlob(id: string): Promise<Blob | null> {
    return idbGet("audio", id);
  }

  private deleteAudioBlob(id: string): Promise<void> {
    return idbDelete("audio", id);
  }

  private saveMicAudioBlob(id: string, blob: Blob): Promise<void> {
    return idbPut("mic_audio", blob, id);
  }

  private async loadMicAudioBlob(id: string): Promise<Blob | null> {
    return idbGet("mic_audio", id);
  }

  private deleteMicAudioBlob(id: string): Promise<void> {
    return idbDelete("mic_audio", id);
  }

  private saveWebcamBlob(id: string, blob: Blob): Promise<void> {
    return idbPut("webcam_videos", blob, id);
  }

  private async loadWebcamBlob(id: string): Promise<Blob | null> {
    return idbGet("webcam_videos", id);
  }

  private deleteWebcamBlob(id: string): Promise<void> {
    return idbDelete("webcam_videos", id);
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
      | Array<{
          id?: string;
          role?: string;
          rawVideoPath?: string;
          rawWebcamVideoPath?: string;
        }>
      | undefined,
    rootRawVideoPath?: string,
    rootRawWebcamVideoPath?: string,
  ): Promise<void> {
    if (!Array.isArray(clips)) return;
    for (const clip of clips) {
      if (!clip || clip.role !== "snapshot") {
        continue;
      }

      for (const path of [clip.rawVideoPath, clip.rawWebcamVideoPath]) {
        const rootPath =
          path === clip.rawWebcamVideoPath
            ? rootRawWebcamVideoPath
            : rootRawVideoPath;
        if (
          !path ||
          path === rootPath ||
          !isManagedCompositionSnapshotPath(path)
        ) {
          continue;
        }
        try {
          await invoke("delete_file", { path });
        } catch {
          // ignore cleanup failures for orphaned snapshot files
        }
      }
    }
  }
}

export const projectManager = new ProjectManager();
