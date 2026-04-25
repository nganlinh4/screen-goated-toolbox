import { useCallback, useRef, useState } from "react";
import { projectManager } from "@/lib/projectManager";
import {
  importAudioPathToManagedMediaFile,
  importAudioToManagedMediaFile,
} from "@/lib/mediaServer";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import type { MusicAudioSegment, Project, VideoSegment } from "@/types/video";

export interface UseMusicAudioImportResult {
  isImporting: boolean;
  importAudio: (file: File) => Promise<void>;
  importAudioPath: (filePath: string) => Promise<void>;
}

interface UseMusicAudioImportOpts {
  /**
   * Returns the id of the currently loaded project, or null if no project
   * is open. The hook reads this fresh on every call.
   */
  getCurrentProjectId: () => string | null;
  /**
   * Called when the user dropped audio while a project was open.
   * The parent must merge the new segment into composition.musicSegments
   * (and persist).
   */
  onAttachToCurrentProject: (segment: MusicAudioSegment) => void | Promise<void>;
  /**
   * Called when the user dropped audio with no project open. The new
   * audio-only project has been created and saved; the parent must load it.
   */
  onCreateAudioOnlyProject: (project: Project) => void | Promise<void>;
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

function makeMusicSegment(
  rawAudioPath: string,
  duration: number,
  fileName: string,
  startTime: number,
): MusicAudioSegment {
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

function buildAudioOnlySegment(duration: number): VideoSegment {
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

export function useMusicAudioImport(
  opts: UseMusicAudioImportOpts,
): UseMusicAudioImportResult {
  const [isImporting, setIsImporting] = useState(false);
  const isImportingRef = useRef(false);

  const createAudioOnlyProjectFromSegment = useCallback(
    async (segment: MusicAudioSegment): Promise<Project> => {
      const safeDuration = segment.duration > 0 ? segment.duration : 1;
      const videoSegment = buildAudioOnlySegment(safeDuration);
      return projectManager.saveProject({
        name: segment.name,
        segment: videoSegment,
        backgroundConfig: { ...DEFAULT_BACKGROUND_CONFIG },
        mousePositions: [],
        recordingMode: "imported",
        duration: safeDuration,
        composition: {
          mode: "separate",
          selectedClipId: null,
          focusedClipId: null,
          clips: [],
          musicSegments: [segment],
          audioOnly: true,
        },
      });
    },
    [],
  );

  const handleImported = useCallback(
    async (rawAudioPath: string, duration: number, fileName: string) => {
      const projectId = opts.getCurrentProjectId();
      const segment = makeMusicSegment(rawAudioPath, duration, fileName, 0);
      if (projectId) {
        await opts.onAttachToCurrentProject(segment);
        return;
      }
      const project = await createAudioOnlyProjectFromSegment(segment);
      await opts.onCreateAudioOnlyProject(project);
    },
    [opts, createAudioOnlyProjectFromSegment],
  );

  const importAudio = useCallback(
    async (file: File) => {
      if (isImportingRef.current) return;
      isImportingRef.current = true;
      setIsImporting(true);
      const traceId = buildAudioImportTraceId();
      try {
        const { path, duration } = await importAudioToManagedMediaFile(
          file,
          file.name,
          traceId,
        );
        await handleImported(path, duration, file.name);
      } catch (err) {
        console.error(`[AudioImport:${traceId}] failed`, err);
      } finally {
        isImportingRef.current = false;
        setIsImporting(false);
      }
    },
    [handleImported],
  );

  const importAudioPath = useCallback(
    async (filePath: string) => {
      if (!filePath.trim()) return;
      if (isImportingRef.current) return;
      isImportingRef.current = true;
      setIsImporting(true);
      const traceId = buildAudioImportTraceId();
      try {
        const { path, duration } = await importAudioPathToManagedMediaFile(
          filePath,
          traceId,
        );
        await handleImported(path, duration, filePath);
      } catch (err) {
        console.error(`[AudioImport:${traceId}] failed`, err);
      } finally {
        isImportingRef.current = false;
        setIsImporting(false);
      }
    },
    [handleImported],
  );

  return { isImporting, importAudio, importAudioPath };
}
