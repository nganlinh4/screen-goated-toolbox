import { useEffect } from "react";
import { invoke } from "@/lib/ipc";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import type { SubtitleFileFormat } from "@/lib/subtitleSrt";
import { type ActivePanel } from "@/components/sidepanel/index";

type PendingVideoDropAction = {
  path?: string;
  action?: string;
};

type PendingSubtitleDropAction = {
  path?: string;
};

type ReadSubtitleFilePathResult = {
  fileName?: string;
  content?: string;
  format?: SubtitleFileFormat;
};

interface AppDropActionsOptions {
  importAudioPaths: (filePaths: string[]) => Promise<unknown>;
  importSubtitlePayload: (payload: {
    fileName: string;
    content: string;
    format?: SubtitleFileFormat;
  }) => Promise<unknown>;
  importVideoPath: (filePath: string) => Promise<{ id: string } | null | undefined>;
  setPendingAutoSubtitleProjectId: (projectId: string | null) => void;
}

export function useAppDropActions({
  importAudioPaths,
  importSubtitlePayload,
  importVideoPath,
  setPendingAutoSubtitleProjectId,
}: AppDropActionsOptions) {
  useEffect(() => {
    let isDraining = false;
    const drainPendingAudioDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<{ path: string }[]>(
            "take_pending_audio_drop_actions",
            {},
          );
          const filePaths = actions
            .map((action) => action.path?.trim() ?? "")
            .filter(Boolean);
          if (filePaths.length > 0) {
            await importAudioPaths(filePaths);
          }
        } catch (error) {
          console.warn("[AudioDrop] Failed to drain pending audio actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-audio-drop-pending", drainPendingAudioDropActions);
    drainPendingAudioDropActions();
    return () => {
      window.removeEventListener("sgt-audio-drop-pending", drainPendingAudioDropActions);
    };
  }, [importAudioPaths]);

  useEffect(() => {
    let isDraining = false;
    const drainPendingSubtitleDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<PendingSubtitleDropAction[]>(
            "take_pending_subtitle_drop_actions",
            {},
          );
          for (const action of actions) {
            const filePath = action.path?.trim();
            if (!filePath) continue;
            const result = await invoke<ReadSubtitleFilePathResult>(
              "read_subtitle_file_path",
              { path: filePath },
            );
            if (!result.content) continue;
            await importSubtitlePayload({
              content: result.content,
              fileName: result.fileName || filePath,
              format: result.format,
            });
            break;
          }
        } catch (error) {
          console.warn("[SubtitleDrop] Failed to drain pending subtitle actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-subtitle-drop-pending", drainPendingSubtitleDropActions);
    drainPendingSubtitleDropActions();
    return () => {
      window.removeEventListener(
        "sgt-subtitle-drop-pending",
        drainPendingSubtitleDropActions,
      );
    };
  }, [importSubtitlePayload]);

  useEffect(() => {
    let isDraining = false;
    const drainPendingVideoDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<PendingVideoDropAction[]>(
            "take_pending_video_drop_actions",
            {},
          );
          for (const action of actions) {
            const filePath = action.path?.trim();
            if (!filePath) continue;
            const project = await importVideoPath(filePath);
            if (project && action.action === "generate-subtitles") {
              setPendingAutoSubtitleProjectId(project.id);
            }
          }
        } catch (error) {
          console.warn("[VideoDrop] Failed to drain pending video actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-video-drop-pending", drainPendingVideoDropActions);
    drainPendingVideoDropActions();
    return () => {
      window.removeEventListener("sgt-video-drop-pending", drainPendingVideoDropActions);
    };
  }, [importVideoPath, setPendingAutoSubtitleProjectId]);
}

interface PendingAutoSubtitleOptions {
  currentProjectId: string | null;
  currentRawVideoPath: string;
  isGeneratingSubtitles: boolean;
  pendingAutoSubtitleArmed: boolean;
  pendingAutoSubtitleProjectId: string | null;
  setActivePanel: (panel: ActivePanel) => void;
  setPendingAutoSubtitleArmed: (isArmed: boolean) => void;
  setPendingAutoSubtitleProjectId: (projectId: string | null) => void;
  setSubtitleSource: (source: SubtitleSource) => void;
  subtitleSource: SubtitleSource;
  handleGenerateSubtitles: () => Promise<unknown> | unknown;
}

export function usePendingAutoSubtitleGeneration({
  currentProjectId,
  currentRawVideoPath,
  handleGenerateSubtitles,
  isGeneratingSubtitles,
  pendingAutoSubtitleArmed,
  pendingAutoSubtitleProjectId,
  setActivePanel,
  setPendingAutoSubtitleArmed,
  setPendingAutoSubtitleProjectId,
  setSubtitleSource,
  subtitleSource,
}: PendingAutoSubtitleOptions) {
  useEffect(() => {
    if (!pendingAutoSubtitleProjectId) return;
    if (currentProjectId !== pendingAutoSubtitleProjectId) return;
    if (!currentRawVideoPath || isGeneratingSubtitles) return;
    setPendingAutoSubtitleProjectId(null);
    setActivePanel("subtitles");
    setSubtitleSource("video");
    setPendingAutoSubtitleArmed(true);
  }, [
    currentProjectId,
    currentRawVideoPath,
    isGeneratingSubtitles,
    pendingAutoSubtitleProjectId,
    setActivePanel,
    setPendingAutoSubtitleArmed,
    setPendingAutoSubtitleProjectId,
    setSubtitleSource,
  ]);

  useEffect(() => {
    if (!pendingAutoSubtitleArmed || subtitleSource !== "video") return;
    setPendingAutoSubtitleArmed(false);
    window.setTimeout(() => {
      void handleGenerateSubtitles();
    }, 150);
  }, [
    handleGenerateSubtitles,
    pendingAutoSubtitleArmed,
    setPendingAutoSubtitleArmed,
    subtitleSource,
  ]);
}
