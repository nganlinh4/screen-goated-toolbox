import { useCallback, useRef, useState } from "react";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { projectManager } from "@/lib/projectManager";
import { createAudioPlaceholderVideo } from "@/lib/mediaServer";
import {
  importSubtitleSrtIntoSegment,
  parseSubtitleSrt,
} from "@/lib/subtitleSrt";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import type {
  BackgroundConfig,
  Project,
  SubtitleSegment,
  VideoSegment,
} from "@/types/video";

const SUBTITLE_PLACEHOLDER_BACKGROUND_CONFIG: BackgroundConfig = {
  ...DEFAULT_BACKGROUND_CONFIG,
  canvasMode: "custom",
  canvasWidth: 1920,
  canvasHeight: 1080,
};

export interface SubtitleSrtImportPayload {
  fileName: string;
  content: string;
}

interface UseSubtitleSrtImportOpts {
  segment: VideoSegment | null;
  duration: number;
  getCurrentProjectId: () => string | null;
  setSegment: (segment: VideoSegment | null) => void;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text" | "subtitles") => void;
  setEditingSubtitleId: (id: string | null) => void;
  onCreateSubtitleProject: (project: Project) => void | Promise<void>;
  onImportedIntoCurrentProject?: (subtitles: SubtitleSegment[]) => void;
}

function buildSubtitleImportTraceId() {
  return `subtitle-srt-import-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

function subtitleFileName(raw: string): string {
  const base = raw.split(/[\\/]/).pop() || "Imported Subtitles";
  return base.replace(/\.[^.]+$/, "") || "Imported Subtitles";
}

function subtitleDuration(subtitles: readonly SubtitleSegment[]): number {
  return Math.max(
    ...subtitles.map((subtitle) => subtitle.endTime).filter(Number.isFinite),
    1,
  );
}

function buildSubtitlePlaceholderSegment(
  subtitles: SubtitleSegment[],
  duration: number,
): VideoSegment {
  const safeDuration = Math.max(duration, 1);
  return {
    trimStart: 0,
    trimEnd: safeDuration,
    trimSegments: [
      { id: crypto.randomUUID(), startTime: 0, endTime: safeDuration },
    ],
    zoomKeyframes: [],
    textSegments: [],
    ...createSubtitleTrackStateFromSegments(subtitles),
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

export function useSubtitleSrtImport(opts: UseSubtitleSrtImportOpts) {
  const [isImporting, setIsImporting] = useState(false);
  const isImportingRef = useRef(false);

  const createSubtitleProject = useCallback(
    async (
      payload: SubtitleSrtImportPayload,
      subtitles: SubtitleSegment[],
    ) => {
      const duration = subtitleDuration(subtitles);
      const segment = buildSubtitlePlaceholderSegment(subtitles, duration);
      const rootClipId = "root";
      const projectName = subtitleFileName(payload.fileName);
      const placeholder = await createAudioPlaceholderVideo(
        duration,
        buildSubtitleImportTraceId(),
      );
      const project = await projectManager.saveProject({
        name: projectName,
        segment,
        backgroundConfig: { ...SUBTITLE_PLACEHOLDER_BACKGROUND_CONFIG },
        mousePositions: [],
        recordingMode: "imported",
        duration,
        rawVideoPath: placeholder.path,
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
              duration,
              segment,
              backgroundConfig: { ...SUBTITLE_PLACEHOLDER_BACKGROUND_CONFIG },
              mousePositions: [],
              recordingMode: "imported",
              rawVideoPath: placeholder.path,
            },
          ],
          audioSegments: [],
          timelineOnly: false,
          placeholderVideoForSubtitles: true,
        },
      });
      await opts.onCreateSubtitleProject(project);
    },
    [opts],
  );

  const importSubtitleSrtPayload = useCallback(
    async (payload: SubtitleSrtImportPayload) => {
      if (isImportingRef.current) return;
      isImportingRef.current = true;
      setIsImporting(true);
      const traceId = buildSubtitleImportTraceId();
      try {
        const projectId = opts.getCurrentProjectId();
        if (projectId && opts.segment) {
          const { segment, subtitles } = importSubtitleSrtIntoSegment(
            opts.segment,
            payload.content,
            opts.duration,
          );
          if (subtitles.length === 0) {
            console.error(`[SubtitleSrt:${traceId}] import failed: no valid subtitles found`);
            return;
          }
          opts.setSegment(segment);
          opts.setEditingSubtitleId(subtitles[0]?.id ?? null);
          opts.setActivePanel("subtitles");
          opts.onImportedIntoCurrentProject?.(subtitles);
          return;
        }

        const subtitles = parseSubtitleSrt(payload.content, 0);
        if (subtitles.length === 0) {
          console.error(`[SubtitleSrt:${traceId}] import failed: no valid subtitles found`);
          return;
        }
        await createSubtitleProject(payload, subtitles);
        opts.setEditingSubtitleId(subtitles[0]?.id ?? null);
        opts.setActivePanel("subtitles");
      } catch (error) {
        console.error(`[SubtitleSrt:${traceId}] import failed`, error);
      } finally {
        isImportingRef.current = false;
        setIsImporting(false);
      }
    },
    [createSubtitleProject, opts],
  );

  const importSubtitleSrtFile = useCallback(
    async (file: File) => {
      await importSubtitleSrtPayload({
        fileName: file.name,
        content: await file.text(),
      });
    },
    [importSubtitleSrtPayload],
  );

  return {
    isImporting,
    importSubtitleSrtFile,
    importSubtitleSrtPayload,
  };
}
