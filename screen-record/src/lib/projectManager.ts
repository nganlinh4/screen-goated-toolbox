import type { Project, ProjectComposition } from "@/types/video";
import { invoke } from "@/lib/ipc";
import { hasStoredProjectMedia } from '@/lib/projectRetention';
import { forgetStoredProjectMedia, omitUnchangedProjectMedia, rememberStoredProjectMedia } from "@/lib/projectStoredMedia";
import { assertAudioImportWriteAllowed, audioProjectWriteGuard, isAudioImportWriteAllowed, markAudioImportCommitted, subscribeAudioImportWriteInvalidation, type AudioImportWriteFence } from './audioImportWriteFence';
import {
  ownedSnapshotClipMediaPaths,
  type SnapshotClipMediaPaths,
} from "@/lib/mediaServer";
import {
  APP_META_STORE,
  buildCompositionAssetKey,
  idbDelete,
  idbCreateProjectBundle,
  idbDeleteCompositionAssetBundle,
  idbDeleteProjectBundle,
  idbGet,
  idbCount,
  idbGetAll,
  idbPut,
  idbUpdateProjectBundle,
  idbUpdateProjectWithCompositionAssetBundle,
  idbWriteCompositionAssetBundle,
  isTimelineOnlyProject,
  LEGACY_PROJECTS_KEY,
  PROJECT_MIGRATION_KEY,
  PROJECT_SWITCH_DEBUG,
  PROJECTS_STORE,
  sortProjectsByDisplayOrder,
  summarizeProjectUpdate,
  summarizeStoredProject,
  type StoredProjectRecord,
} from "@/lib/projectStorage";
import {
  ProjectWriteCoordinator,
  type ProjectWriteIntent,
} from "@/lib/projectWriteCoordinator";
import {
  collectProjectCustomBackgroundUrls,
  normalizeProjectName,
} from "@/lib/projectMetadata";

class ProjectManager {
  private limit = normalizeProjectLimit(
    (window as any).__SR_INITIAL_PROJECT_LIMIT__,
  );
  private migrationPromise: Promise<void> | null = null;
  private readonly writes = new ProjectWriteCoordinator();
  private activeProjectId: string | null = null;
  private pruning: Promise<void> = Promise.resolve();

  setActiveProjectId(projectId: string | null): void {
    this.activeProjectId = projectId;
  }

  createEditorWriteIntent(projectId: string): ProjectWriteIntent {
    return this.writes.createEditorIntent(projectId);
  }

  invalidateEditorWrites(projectId: string): void {
    this.writes.invalidateEditorIntents(projectId);
  }

  isEditorWriteIntentLatest(intent: ProjectWriteIntent): boolean {
    return this.writes.isLatest(intent);
  }

  async setLimit(newLimit: number): Promise<void> {
    const limit = normalizeProjectLimit(newLimit);
    await invoke("set_project_limit", { limit });
    this.limit = limit;
    await this.pruneProjects();
  }

  applyHostLimit(newLimit: number) {
    this.limit = normalizeProjectLimit(newLimit);
    void this.pruneProjects().then(() => {
      window.dispatchEvent(new Event("sr-project-retention-changed"));
    }).catch((error) => console.error("Failed to apply project retention", error));
  }

  getLimit(): number {
    return this.limit;
  }

  async getManagedCustomBackgroundUrls(): Promise<string[]> {
    await this.ensureProjectStoreReady();
    const projects = await this.getProjectRecords();
    const urls = collectProjectCustomBackgroundUrls(projects);
    const compositionUrls = await idbGetAll<string>("composition_custom_backgrounds");
    return [...new Set([...urls, ...compositionUrls].filter(Boolean))];
  }

  async saveProject(
    project: Omit<Project, "id" | "createdAt" | "lastModified">,
    recordingId?: string,
  ): Promise<Project> {
    await this.ensureProjectStoreReady();

    if (recordingId && !/^[0-9a-f]{32}$/.test(recordingId)) {
      throw new Error("Invalid recording identity");
    }
    const id = recordingId ? `recording-${recordingId}` : crypto.randomUUID();
    const existing = recordingId ? await this.loadProjectRecord(id) : null;
    if (existing) {
      if (!project.rawVideoPath || existing.rawVideoPath !== project.rawVideoPath) {
        throw new Error("Recording identity belongs to another project");
      }
      return existing;
    }

    const newProject: Project = {
      ...project,
      id,
      createdAt: Date.now(),
      lastModified: Date.now(),
    };

    try {
      await idbCreateProjectBundle(newProject, Boolean(recordingId));
      rememberStoredProjectMedia(id, newProject);
    } catch (error) {
      const prior = recordingId ? await this.loadProjectRecord(id) : null;
      if (!prior || !project.rawVideoPath || prior.rawVideoPath !== project.rawVideoPath) throw error;
      return prior;
    }
    await this.pruneProjects();
    return newProject;
  }

  async getProjects(): Promise<
    Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob">[]
  > {
    await this.ensureProjectStoreReady();
    return sortProjectsByDisplayOrder(await this.getProjectRecords());
  }

  /** One stored project record plus the stored key count, so a caller can patch its list in place. */
  async getProjectListEntry(id: string): Promise<{
    project: Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob"> | null;
    count: number;
  }> {
    await this.ensureProjectStoreReady();
    const [project, count] = await Promise.all([this.loadProjectRecord(id), idbCount(PROJECTS_STORE)]);
    return { project, count };
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
    rememberStoredProjectMedia(id, { videoBlob, audioBlob, micAudioBlob, webcamBlob });
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
    await this.deleteProjectQueued(id);
  }

  private async deleteProjectQueued(id: string, retention = false): Promise<boolean> {
    assertAudioImportWriteAllowed(id, true);
    if (!retention) this.invalidateEditorWrites(id);
    const result = await this.writes.enqueue(id, async () => {
      assertAudioImportWriteAllowed(id, true);
      const project = await this.loadProjectRecord(id);
      if (!project || (retention && (id === this.activeProjectId || await hasStoredProjectMedia(project)
        || id === this.activeProjectId))) {
        return false;
      }
      if (retention) this.invalidateEditorWrites(id);
      await idbDeleteProjectBundle(id, project, () => isAudioImportWriteAllowed(id),
        (listener) => subscribeAudioImportWriteInvalidation(id, undefined, listener));
      forgetStoredProjectMedia(id);
      if (!retention) await this.deleteProjectFiles(project);
      return true;
    });
    if (!result.applied) {
      throw new Error(`Failed to delete project ${id}`);
    }
    return result.value === true;
  }

  async updateProject(
    id: string,
    updates: Partial<Omit<Project, "id" | "createdAt" | "lastModified">>,
    intent?: ProjectWriteIntent,
    canCommit?: () => boolean,
    onInvalidated?: (listener: () => void) => () => void,
    audioFence?: AudioImportWriteFence,
  ): Promise<boolean> {
    await this.ensureProjectStoreReady();
    const timelineWrite = Object.keys(updates).length > 0;
    const { isCurrent, subscribe } = audioProjectWriteGuard(id, timelineWrite, audioFence, canCommit, onInvalidated);
    const result = await this.writes.enqueue(
      id,
      async () => {
        assertAudioImportWriteAllowed(id, timelineWrite, audioFence);
        if (!isCurrent()) return false;
        const previousProject = await this.loadProjectRecord(id);
        if (!isCurrent()) return false;
        if (PROJECT_SWITCH_DEBUG) {
          console.warn(
            `[ProjectSwitch] ${JSON.stringify({
              event: "project-manager:update",
              targetProjectId: id,
              prev: summarizeStoredProject(previousProject),
              updates: summarizeProjectUpdate(updates),
            })}`,
          );
        }
        const changedUpdates = omitUnchangedProjectMedia(id, updates);
        const updated = await idbUpdateProjectBundle(id, changedUpdates, isCurrent, subscribe);
        if (updated === undefined) return false;
        if (!updated) {
          throw new Error(`Cannot update missing project ${id}`);
        }
        rememberStoredProjectMedia(id, changedUpdates);
        markAudioImportCommitted(id, audioFence);
        return true;
      },
      intent,
    );
    if (result.applied && result.value) await this.pruneProjects();
    return result.applied && result.value === true;
  }

  async renameProject(id: string, name: string): Promise<void> {
    await this.updateProject(id, { name: normalizeProjectName(name) });
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
    intent?: ProjectWriteIntent,
  ): Promise<boolean> {
    await this.ensureProjectStoreReady();
    const key = buildCompositionAssetKey(projectId, clipId);
    const result = await this.writes.enqueue(
      projectId,
      () => {
        assertAudioImportWriteAllowed(projectId, true);
        return idbWriteCompositionAssetBundle(key, data, () => isAudioImportWriteAllowed(projectId),
          (listener) => subscribeAudioImportWriteInvalidation(projectId, undefined, listener));
      },
      intent,
    );
    return result.applied;
  }

  async updateProjectWithCompositionClipAssets(
    projectId: string,
    clipId: string,
    composition: ProjectComposition,
    data: {
      videoBlob?: Blob;
      audioBlob?: Blob;
      micAudioBlob?: Blob;
      webcamBlob?: Blob;
      customBackground?: string;
    },
    intent?: ProjectWriteIntent,
  ): Promise<boolean> {
    await this.ensureProjectStoreReady();
    const result = await this.writes.enqueue(
      projectId,
      async () => {
        assertAudioImportWriteAllowed(projectId, true);
        const updated = await idbUpdateProjectWithCompositionAssetBundle(
          projectId,
          clipId,
          composition,
          data,
          () => isAudioImportWriteAllowed(projectId),
          (listener) => subscribeAudioImportWriteInvalidation(projectId, undefined, listener),
        );
        if (!updated) {
          throw new Error(`Cannot update missing project ${projectId}`);
        }
      },
      intent,
    );
    return result.applied;
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
    assertAudioImportWriteAllowed(projectId, true);
    const key = buildCompositionAssetKey(projectId, clipId);
    await idbDeleteCompositionAssetBundle(key, () => isAudioImportWriteAllowed(projectId),
      (listener) => subscribeAudioImportWriteInvalidation(projectId, undefined, listener));
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
    const pending = this.pruning.catch(() => {}).then(() => this.pruneProjectsNow());
    this.pruning = pending;
    await pending;
  }

  private async pruneProjectsNow(): Promise<void> {
    // Counting keys is cheap; only read every record when something can be pruned.
    if (await idbCount(PROJECTS_STORE) <= this.limit) return;
    const projects = sortProjectsByDisplayOrder(await this.getProjectRecords());
    let count = projects.length;
    for (const project of [...projects].reverse()) {
      if (count <= this.limit) break;
      if (project.id !== this.activeProjectId && await this.deleteProjectQueued(project.id, true)) {
        count -= 1;
      }
    }
  }

  private async deleteProjectFiles(
    project: Project | StoredProjectRecord | null | undefined,
  ): Promise<void> {
    await this.deleteCompositionSnapshotFiles(
      [
        ...(project?.composition?.clips ?? []),
        ...(project?.composition?.retainedRemovedClips ?? []),
      ],
      [
        project?.rawVideoPath,
        (project as Project | null | undefined)?.rawMicAudioPath,
        project?.rawWebcamVideoPath,
      ],
    );
    // Delete the root raw media files too (lived in `recordings/`).
    // Previously only ProjectsView.tsx did this, and it only covered
    // rawVideoPath — rawWebcamVideoPath and rawMicAudioPath were leaking.
    const rootRawPaths = [
      project?.rawVideoPath,
      project?.rawWebcamVideoPath,
      (project as Project | null | undefined)?.rawMicAudioPath,
      ...(project?.composition?.audioSegments ?? []).map(
        (segment) => segment.rawAudioPath,
      ),
      ...(project?.composition?.narrationSegments ?? []).map(
        (segment) => segment.rawAudioPath,
      ),
    ];
    for (const path of new Set(rootRawPaths)) {
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

  private async loadVideoBlob(id: string): Promise<Blob | null> {
    return idbGet("videos", id);
  }

  private async loadAudioBlob(id: string): Promise<Blob | null> {
    return idbGet("audio", id);
  }

  private async loadMicAudioBlob(id: string): Promise<Blob | null> {
    return idbGet("mic_audio", id);
  }

  private async loadWebcamBlob(id: string): Promise<Blob | null> {
    return idbGet("webcam_videos", id);
  }

  private async deleteCompositionSnapshotFiles(
    clips: Array<SnapshotClipMediaPaths & { role?: string }> | undefined,
    rootPaths: ReadonlyArray<string | undefined>,
  ): Promise<void> {
    if (!Array.isArray(clips)) return;
    for (const clip of clips) {
      if (!clip || clip.role !== "snapshot") continue;
      for (const path of ownedSnapshotClipMediaPaths(clip, rootPaths)) {
        try {
          await invoke("delete_file", { path });
        } catch {
          // ignore cleanup failures for orphaned snapshot files
        }
      }
    }
  }
}

export function normalizeProjectLimit(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(100, Math.max(10, Math.round(value)))
    : 50;
}

export const projectManager = new ProjectManager();
