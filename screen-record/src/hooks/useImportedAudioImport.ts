import { useCallback, useRef, useState } from "react";
import { projectManager } from "@/lib/projectManager";
import {
  createAudioPlaceholderVideo,
  importAudioPathToManagedMediaFile,
  importAudioToManagedMediaFile,
} from "@/lib/mediaServer";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { invoke, logToHost } from "@/lib/ipc";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import type {
  BackgroundConfig,
  ImportedAudioSegment,
  Project,
  VideoSegment,
} from "@/types/video";

const AUDIO_PLACEHOLDER_BACKGROUND_CONFIG: BackgroundConfig = {
  ...DEFAULT_BACKGROUND_CONFIG,
  canvasMode: "custom",
  canvasWidth: 1920,
  canvasHeight: 1080,
};

export interface UseImportedAudioImportResult {
  isImporting: boolean;
  importAudio: (file: File, expectedProjectId?: string | null) => Promise<void>;
  importAudios: (files: File[], expectedProjectId?: string | null) => Promise<void>;
  importAudioPath: (filePath: string, expectedProjectId?: string | null) => Promise<void>;
  importAudioPaths: (filePaths: string[], expectedProjectId?: string | null) => Promise<void>;
}

interface UseImportedAudioImportOpts {
  /**
   * Returns the id of the currently loaded project, or null if no project
   * is open. The hook reads this fresh on every call.
   */
  getCurrentProjectId: () => string | null;
  /**
   * Called when the user dropped audio while a project was open.
   * The parent must merge the new segment into composition.audioSegments
   * (and persist).
   */
  onAttachToCurrentProject: (
    segments: ImportedAudioSegment[],
    expectedProjectId: string,
  ) => void | Promise<void>;
  /**
   * Called when the user dropped audio with no project open. The new
   * placeholder-video project has been created and saved; the parent must load it.
   */
  onCreateAudioProject: (project: Project) => void | Promise<void>;
  onError?: (message: string) => void;
}

function buildAudioImportTraceId() {
  return `audio-import-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

function audioFileName(raw: string): string {
  const base = raw.split(/[\\/]/).pop() || "Imported Audio";
  return base.replace(/\.[^.]+$/, "") || "Imported Audio";
}

function makeImportedAudioSegment(
  rawAudioPath: string,
  duration: number,
  fileName: string,
  startTime: number,
): ImportedAudioSegment {
  return {
    id: crypto.randomUUID(),
    rawAudioPath,
    name: audioFileName(fileName),
    duration,
    startTime,
    inPoint: 0,
    outPoint: duration,
    addedAt: Date.now(),
  };
}

async function deleteAbandonedImports(
  imported: Array<{ rawAudioPath: string }>,
): Promise<void> {
  await Promise.allSettled(
    imported.map((item) =>
      invoke("delete_file", { path: item.rawAudioPath }),
    ),
  );
}

function buildAudioPlaceholderSegment(duration: number): VideoSegment {
  const safeDuration = duration > 0 ? duration : 1;
  return {
    trimStart: 0,
    trimEnd: safeDuration,
    trimSegments: [
      { id: crypto.randomUUID(), startTime: 0, endTime: safeDuration },
    ],
    zoomKeyframes: [],
    textSegments: [],
    ...createSubtitleTrackStateFromSegments([]),
    speedPoints: [
      { time: 0, speed: 1 },
      { time: safeDuration, speed: 1 },
    ],
    deviceAudioAvailable: false,
    micAudioAvailable: false,
    webcamAvailable: false,
    useCustomCursor: false,
    keystrokeMode: "off",
    keystrokeEvents: [],
    keyboardVisibilitySegments: [],
    keyboardMouseVisibilitySegments: [],
  };
}

export function useImportedAudioImport(
  opts: UseImportedAudioImportOpts,
): UseImportedAudioImportResult {
  const [isImporting, setIsImporting] = useState(false);
  const isImportingRef = useRef(false);

  const createAudioProjectFromSegment = useCallback(
    async (segments: ImportedAudioSegment[], traceId: string): Promise<Project> => {
      const safeDuration = Math.max(
        ...segments.map((segment) => segment.startTime + Math.max(segment.outPoint - segment.inPoint, 0)),
        1,
      );
      const videoSegment = buildAudioPlaceholderSegment(safeDuration);
      const rootClipId = "root";
      const { path: placeholderVideoPath } = await createAudioPlaceholderVideo(
        safeDuration,
        traceId,
      );
      const projectName = segments.length === 1
        ? segments[0].name
        : `${segments[0].name} + ${segments.length - 1}`;
      logToHost(
        `[AudioImport:${traceId}][Frontend] placeholder ready path="${placeholderVideoPath}"`,
      );
      const project = await projectManager.saveProject({
        name: projectName,
        segment: videoSegment,
        backgroundConfig: { ...AUDIO_PLACEHOLDER_BACKGROUND_CONFIG },
        mousePositions: [],
        recordingMode: "imported",
        duration: safeDuration,
        rawVideoPath: placeholderVideoPath,
        composition: {
          mode: "separate",
          selectedClipId: rootClipId,
          focusedClipId: rootClipId,
          globalCanvasConfig: {
            canvasMode: "custom",
            canvasWidth: 1920,
            canvasHeight: 1080,
            autoSourceClipId: null,
          },
          clips: [
            {
              id: rootClipId,
              role: "root",
              name: projectName,
              duration: safeDuration,
              segment: videoSegment,
              backgroundConfig: { ...AUDIO_PLACEHOLDER_BACKGROUND_CONFIG },
              mousePositions: [],
              recordingMode: "imported",
              rawVideoPath: placeholderVideoPath,
            },
          ],
          audioSegments: segments,
          placeholderVideoForAudio: true,
        },
      });
      logToHost(
        `[AudioImport:${traceId}][Frontend] project saved id="${project.id}" rawVideoPath="${project.rawVideoPath ?? ""}"`,
      );
      return project;
    },
    [],
  );

  const handleImported = useCallback(
    async (
      imported: Array<{ rawAudioPath: string; duration: number; fileName: string }>,
      traceId: string,
      expectedProjectId: string | null,
    ) => {
      if (imported.length === 0) return;
      if (opts.getCurrentProjectId() !== expectedProjectId) {
        throw new Error("The open project changed while audio was importing");
      }
      if (expectedProjectId) {
        await opts.onAttachToCurrentProject(
          imported.map((item) =>
            makeImportedAudioSegment(item.rawAudioPath, item.duration, item.fileName, 0),
          ),
          expectedProjectId,
        );
        return;
      }
      let cursor = 0;
      const segments = imported.map((item) => {
        const segment = makeImportedAudioSegment(item.rawAudioPath, item.duration, item.fileName, cursor);
        cursor += Math.max(segment.outPoint - segment.inPoint, 0);
        return segment;
      });
      const project = await createAudioProjectFromSegment(segments, traceId);
      if (opts.getCurrentProjectId() !== expectedProjectId) {
        await projectManager.deleteProject(project.id);
        throw new Error("The open project changed while audio was importing");
      }
      logToHost(
        `[AudioImport:${traceId}][Frontend] project callback start id="${project.id}"`,
      );
      await opts.onCreateAudioProject(project);
      logToHost(
        `[AudioImport:${traceId}][Frontend] project callback complete id="${project.id}"`,
      );
    },
    [opts, createAudioProjectFromSegment],
  );

  const importAudios = useCallback(
    async (
      files: File[],
      expectedProjectId = opts.getCurrentProjectId(),
    ) => {
      const audioFiles = files.filter(Boolean);
      if (audioFiles.length === 0) return;
      if (isImportingRef.current) return;
      isImportingRef.current = true;
      setIsImporting(true);
      const traceId = buildAudioImportTraceId();
      const imported: Array<{
        rawAudioPath: string;
        duration: number;
        fileName: string;
      }> = [];
      try {
        for (const file of audioFiles) {
          const { path, duration } = await importAudioToManagedMediaFile(
            file,
            file.name,
            traceId,
          );
          imported.push({ rawAudioPath: path, duration, fileName: file.name });
          if (opts.getCurrentProjectId() !== expectedProjectId) {
            await deleteAbandonedImports(imported);
            imported.length = 0;
            throw new Error("The open project changed while audio was importing");
          }
        }
        await handleImported(imported, traceId, expectedProjectId);
      } catch (err) {
        console.error(`[AudioImport:${traceId}] failed`, err);
        opts.onError?.(`Audio import failed: ${String(err)}`);
      } finally {
        isImportingRef.current = false;
        setIsImporting(false);
      }
    },
    [handleImported, opts],
  );

  const importAudio = useCallback(
    async (file: File, expectedProjectId = opts.getCurrentProjectId()) =>
      importAudios([file], expectedProjectId),
    [importAudios, opts],
  );

  const importAudioPaths = useCallback(
    async (
      filePaths: string[],
      expectedProjectId = opts.getCurrentProjectId(),
    ) => {
      const paths = filePaths.map((filePath) => filePath.trim()).filter(Boolean);
      if (paths.length === 0) return;
      if (isImportingRef.current) return;
      isImportingRef.current = true;
      setIsImporting(true);
      const traceId = buildAudioImportTraceId();
      const imported: Array<{
        rawAudioPath: string;
        duration: number;
        fileName: string;
      }> = [];
      try {
        for (const filePath of paths) {
          const { path, duration } = await importAudioPathToManagedMediaFile(
            filePath,
            traceId,
          );
          imported.push({ rawAudioPath: path, duration, fileName: filePath });
          if (opts.getCurrentProjectId() !== expectedProjectId) {
            await deleteAbandonedImports(imported);
            imported.length = 0;
            throw new Error("The open project changed while audio was importing");
          }
        }
        await handleImported(imported, traceId, expectedProjectId);
      } catch (err) {
        console.error(`[AudioImport:${traceId}] failed`, err);
        opts.onError?.(`Audio import failed: ${String(err)}`);
      } finally {
        isImportingRef.current = false;
        setIsImporting(false);
      }
    },
    [handleImported, opts],
  );

  const importAudioPath = useCallback(
    async (filePath: string, expectedProjectId = opts.getCurrentProjectId()) =>
      importAudioPaths([filePath], expectedProjectId),
    [importAudioPaths, opts],
  );

  return { isImporting, importAudio, importAudios, importAudioPath, importAudioPaths };
}
