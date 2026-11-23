import { useEffect, useRef } from "react";
import { getImportedProjectOwner, importStartOwnerCurrent, type ImportStartOwner } from './importProjectOwnership';
import { invoke } from "@/lib/ipc";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import type { SubtitleFileFormat } from "@/lib/subtitleSrt";
import { type ActivePanel } from "@/components/sidepanel/index";

type PendingVideoDropAction = {
  path?: string;
  action?: string;
};

export interface PendingAutoSubtitleRequest {
  projectId: string;
  isCurrent: () => boolean;
}

type PendingSubtitleDropAction = {
  path?: string;
};

type ReadSubtitleFilePathResult = {
  fileName?: string;
  content?: string;
  format?: SubtitleFileFormat;
};

interface AppDropActionsOptions {
  getCurrentProjectId: () => string | null;
  captureStartOwner?: () => ImportStartOwner;
  importAudioPaths: (
    filePaths: string[],
    expectedProjectId?: string | null,
    owner?: ImportStartOwner,
  ) => Promise<unknown>;
  importSubtitlePayload: (payload: {
    fileName: string;
    content: string;
    format?: SubtitleFileFormat;
  }, expectedProjectId?: string | null, owner?: ImportStartOwner) => Promise<unknown>;
  importVideoPath: (filePath: string, owner?: ImportStartOwner) => Promise<{ id: string } | null | undefined>;
  setPendingAutoSubtitleProjectId: (request: PendingAutoSubtitleRequest | null) => void;
  /** True while an import of that kind runs; drops wait in the host queue until it ends. */
  isAudioImportBusy?: boolean;
  isSubtitleImportBusy?: boolean;
  isVideoImportBusy?: boolean;
}

/**
 * Drains one host drop queue on mount, on its host event, and whenever the
 * matching importer becomes idle. Actions stay queued on the host while that
 * importer is busy (it would reject them), and an event that arrives during a
 * drain schedules one more drain instead of being ignored.
 */
function usePendingDropDrain(eventName: string, busy: boolean, drain: () => Promise<void>) {
  const drainRef = useRef(drain);
  const busyRef = useRef(busy);
  useEffect(() => {
    drainRef.current = drain;
    busyRef.current = busy;
  });
  const stateRef = useRef({ draining: false, again: false });
  const runRef = useRef<() => void>(() => undefined);
  useEffect(() => {
    runRef.current = () => {
      const state = stateRef.current;
      if (busyRef.current) return;
      if (state.draining) {
        state.again = true;
        return;
      }
      state.draining = true;
      state.again = false;
      void drainRef.current().finally(() => {
        state.draining = false;
        if (state.again) runRef.current();
      });
    };
    const onPending = () => runRef.current();
    window.addEventListener(eventName, onPending);
    return () => window.removeEventListener(eventName, onPending);
  }, [eventName]);
  useEffect(() => {
    if (!busy) runRef.current();
  }, [busy]);
}

export function useAppDropActions(options: AppDropActionsOptions) {
  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  });

  usePendingDropDrain("sgt-audio-drop-pending", options.isAudioImportBusy === true, async () => {
    const { getCurrentProjectId, captureStartOwner, importAudioPaths } = optionsRef.current;
    const expectedProjectId = getCurrentProjectId();
    const owner = captureStartOwner?.();
    try {
      const actions = await invoke<{ path: string }[]>("take_pending_audio_drop_actions", {});
      const filePaths = actions
        .map((action) => action.path?.trim() ?? "")
        .filter(Boolean);
      if (filePaths.length > 0) {
        await importAudioPaths(filePaths, expectedProjectId, owner);
      }
    } catch (error) {
      console.warn("[AudioDrop] Failed to drain pending audio actions", error);
    }
  });

  usePendingDropDrain("sgt-subtitle-drop-pending", options.isSubtitleImportBusy === true, async () => {
    const { getCurrentProjectId, captureStartOwner, importSubtitlePayload } = optionsRef.current;
    const expectedProjectId = getCurrentProjectId();
    const owner = captureStartOwner?.();
    try {
      const actions = await invoke<PendingSubtitleDropAction[]>("take_pending_subtitle_drop_actions", {});
      for (const action of actions) {
        const filePath = action.path?.trim();
        if (!filePath) continue;
        const result = await invoke<ReadSubtitleFilePathResult>(
          "read_subtitle_file_path",
          { path: filePath },
        );
        if (!result.content) continue;
        await importSubtitlePayload(
          {
            content: result.content,
            fileName: result.fileName || filePath,
            format: result.format,
          },
          expectedProjectId,
          owner,
        );
        break;
      }
    } catch (error) {
      console.warn("[SubtitleDrop] Failed to drain pending subtitle actions", error);
    }
  });

  usePendingDropDrain("sgt-video-drop-pending", options.isVideoImportBusy === true, async () => {
    const { captureStartOwner, importVideoPath, setPendingAutoSubtitleProjectId } = optionsRef.current;
    let owner = captureStartOwner?.();
    try {
      const actions = await invoke<PendingVideoDropAction[]>("take_pending_video_drop_actions", {});
      for (const action of actions) {
        if (!importStartOwnerCurrent(owner)) return;
        const filePath = action.path?.trim();
        if (!filePath) continue;
        const project = await importVideoPath(filePath, owner);
        if (project && action.action === "generate-subtitles") {
          const isCurrent = getImportedProjectOwner(project);
          if (isCurrent?.()) setPendingAutoSubtitleProjectId({ projectId: project.id, isCurrent });
        }
        if (project && getImportedProjectOwner(project)?.()) owner = captureStartOwner?.();
      }
    } catch (error) {
      console.warn("[VideoDrop] Failed to drain pending video actions", error);
    }
  });
}

interface PendingAutoSubtitleOptions {
  currentProjectId: string | null;
  currentRawVideoPath: string;
  isGeneratingSubtitles: boolean;
  pendingAutoSubtitleArmed: boolean;
  pendingAutoSubtitleProjectId: PendingAutoSubtitleRequest | null;
  setActivePanel: (panel: ActivePanel) => void;
  setPendingAutoSubtitleArmed: (isArmed: boolean) => void;
  setPendingAutoSubtitleProjectId: (request: PendingAutoSubtitleRequest | null) => void;
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
  const armedOwner = useRef<PendingAutoSubtitleRequest | null>(null);
  const generationTimer = useRef<number | null>(null);
  useEffect(() => () => {
    if (generationTimer.current !== null) window.clearTimeout(generationTimer.current);
  }, []);
  useEffect(() => {
    if (!pendingAutoSubtitleProjectId) return;
    if (!pendingAutoSubtitleProjectId.isCurrent()) {
      setPendingAutoSubtitleProjectId(null);
      return;
    }
    if (currentProjectId !== pendingAutoSubtitleProjectId.projectId) return;
    if (!currentRawVideoPath || isGeneratingSubtitles) return;
    armedOwner.current = pendingAutoSubtitleProjectId;
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
    const owner = armedOwner.current;
    setPendingAutoSubtitleArmed(false);
    if (generationTimer.current !== null) window.clearTimeout(generationTimer.current);
    generationTimer.current = window.setTimeout(() => {
      generationTimer.current = null;
      if (owner?.isCurrent()) void handleGenerateSubtitles();
    }, 150);
  }, [
    handleGenerateSubtitles,
    pendingAutoSubtitleArmed,
    setPendingAutoSubtitleArmed,
    subtitleSource,
  ]);
}
