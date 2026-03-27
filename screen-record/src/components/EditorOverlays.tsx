import type { Dispatch, MutableRefObject, RefObject, SetStateAction } from "react";
import { VideoSegment, BackgroundConfig, ExportOptions, type Project, type ExportArtifact } from "@/types/video";
import {
  ProcessingOverlay,
  ExportDialog,
  WindowSelectDialog,
  RawVideoDialog,
  ExportSuccessDialog,
  HotkeyDialog,
} from "@/components/dialogs";
import {
  ProjectsView,
  type ProjectsPreviewTargetSnapshot,
} from "@/components/ProjectsView";
import { CropWorkspace } from "@/components/CropWorkspace";
import { useSettings } from "@/hooks/useSettings";
import type { WindowInfo } from "@/hooks/useAppHooks";

export interface EditorOverlaysExportHook {
  isProcessing: boolean;
  cancelExport: () => void;
  showExportDialog: boolean;
  setShowExportDialog: (show: boolean) => void;
  startExport: () => void;
  exportOptions: ExportOptions;
  setExportOptions: Dispatch<SetStateAction<ExportOptions>>;
  dialogSegment: VideoSegment | null;
  dialogBackgroundConfig: BackgroundConfig | null;
  hasAudio: boolean;
  sourceVideoFps: number | null;
  dialogTrimmedDurationSec: number;
  dialogClipCount: number;
  exportAutoCopyEnabled: boolean;
  setExportAutoCopyEnabled: (v: boolean) => void;
  showExportSuccessDialog: boolean;
  setShowExportSuccessDialog: (show: boolean) => void;
  lastExportedPath: string;
  lastExportArtifacts: ExportArtifact[];
}

export interface EditorOverlaysProps {
  // Projects overlay
  showProjectsDialog: boolean;
  projects: Project[];
  onBeginProjectOpen: () => void;
  onLoadProject: (id: string) => Promise<void>;
  onProjectsChange: () => Promise<void>;
  currentProjectId: string | null;
  restoreImageRef: MutableRefObject<string | null>;
  previewTargetSnapshotRef: MutableRefObject<ProjectsPreviewTargetSnapshot | null>;
  projectPickerMode: "insertBefore" | "insertAfter" | null;
  setProjectPickerMode: (mode: "insertBefore" | "insertAfter" | null) => void;
  setShowProjectsDialog: (show: boolean) => void;
  armProjectInteractionShieldRelease: () => void;
  onPickProject: (id: string) => void;
  onImportVideo?: (file: File) => void;
  // Interaction shield
  isProjectInteractionShieldVisible: boolean;
  // Crop workspace
  isCropping: boolean;
  currentVideo: string | null;
  segment: VideoSegment | null;
  currentTime: number;
  onCancelCrop: () => void;
  onApplyCrop: (crop: VideoSegment["crop"]) => void;
  // Dialogs
  exportHook: EditorOverlaysExportHook;
  videoRef: RefObject<HTMLVideoElement | null>;
  showWindowSelect: boolean;
  onCloseWindowSelect: () => void;
  windows: WindowInfo[];
  onSelectWindowForRecording: (windowId: string, captureMethod: "game" | "window") => void;
  isVideoReady: boolean;
  // Raw video dialog
  showRawVideoDialog: boolean;
  onCloseRawVideoDialog: () => void;
  lastRawSavedPath: string;
  rawAutoCopyEnabled: boolean;
  isRawActionBusy: boolean;
  onChangeRawSavedPath: (path: string) => void;
  onToggleRawAutoCopy: (enabled: boolean) => void;
  // Export success path change
  onExportSuccessPathChange: (path: string) => Promise<void>;
  // Hotkey dialog
  showHotkeyDialog: boolean;
  onCloseHotkeyDialog: () => void;
}

export function EditorOverlays({
  showProjectsDialog,
  projects,
  onBeginProjectOpen,
  onLoadProject,
  onProjectsChange,
  currentProjectId,
  restoreImageRef,
  previewTargetSnapshotRef,
  projectPickerMode,
  setProjectPickerMode,
  setShowProjectsDialog,
  armProjectInteractionShieldRelease,
  onPickProject,
  onImportVideo,
  isProjectInteractionShieldVisible,
  isCropping,
  currentVideo,
  segment,
  currentTime,
  onCancelCrop,
  onApplyCrop,
  exportHook,
  videoRef,
  showWindowSelect,
  onCloseWindowSelect,
  windows,
  onSelectWindowForRecording,
  isVideoReady,
  showRawVideoDialog,
  onCloseRawVideoDialog,
  lastRawSavedPath,
  rawAutoCopyEnabled,
  isRawActionBusy,
  onChangeRawSavedPath,
  onToggleRawAutoCopy,
  onExportSuccessPathChange,
  showHotkeyDialog,
  onCloseHotkeyDialog,
}: EditorOverlaysProps) {
  const { t } = useSettings();

  return (
    <>
      {showProjectsDialog && (
        <div className="absolute inset-0 top-[44px] z-[90]">
          <ProjectsView
            projects={projects}
            onBeginProjectOpen={onBeginProjectOpen}
            onLoadProject={onLoadProject}
            onProjectsChange={onProjectsChange}
            onClose={() => {
              setProjectPickerMode(null);
              setShowProjectsDialog(false);
              // Only release shield on normal "load" mode close (picker mode is null = "load")
              if (projectPickerMode === null) {
                armProjectInteractionShieldRelease();
              }
            }}
            currentProjectId={currentProjectId}
            restoreImage={restoreImageRef.current}
            previewTargetSnapshot={previewTargetSnapshotRef.current}
            pickerMode={projectPickerMode ?? "load"}
            onPickProject={onPickProject}
            onImportVideo={onImportVideo}
          />
        </div>
      )}

      {isProjectInteractionShieldVisible && !showProjectsDialog && (
        <div className="project-interaction-shield absolute inset-0 top-[44px] z-[89]" />
      )}

      {isCropping && currentVideo && (
        <div className="crop-workspace-overlay absolute inset-0 top-[44px] z-[120]">
          <CropWorkspace
            show={isCropping}
            videoSrc={currentVideo}
            initialCrop={segment?.crop}
            initialTime={currentTime}
            onCancel={onCancelCrop}
            onApply={onApplyCrop}
          />
        </div>
      )}

      <ProcessingOverlay
        show={exportHook.isProcessing}
        exportProgress={0}
        onCancel={exportHook.cancelExport}
      />
      <WindowSelectDialog
        show={showWindowSelect}
        onClose={onCloseWindowSelect}
        windows={windows}
        onSelectWindow={onSelectWindowForRecording}
      />
      {currentVideo && !isVideoReady && !showProjectsDialog && (
        <div className="video-loading-overlay absolute inset-0 flex items-center justify-center bg-black/62">
          <div className="loading-message text-[var(--on-surface)]">
            {t.preparingVideoOverlay}
          </div>
        </div>
      )}
      <ExportDialog
        show={exportHook.showExportDialog}
        onClose={() => exportHook.setShowExportDialog(false)}
        onExport={exportHook.startExport}
        exportOptions={exportHook.exportOptions}
        setExportOptions={exportHook.setExportOptions}
        segment={exportHook.dialogSegment}
        videoRef={videoRef}
        backgroundConfig={exportHook.dialogBackgroundConfig ?? {} as BackgroundConfig}
        hasAudio={exportHook.hasAudio}
        sourceVideoFps={exportHook.sourceVideoFps}
        trimmedDurationSec={exportHook.dialogTrimmedDurationSec}
        clipCount={exportHook.dialogClipCount}
        autoCopyEnabled={exportHook.exportAutoCopyEnabled}
        onToggleAutoCopy={exportHook.setExportAutoCopyEnabled}
      />
      <RawVideoDialog
        show={showRawVideoDialog}
        onClose={onCloseRawVideoDialog}
        savedPath={lastRawSavedPath}
        autoCopyEnabled={rawAutoCopyEnabled}
        isBusy={isRawActionBusy}
        onChangePath={onChangeRawSavedPath}
        onToggleAutoCopy={onToggleRawAutoCopy}
      />
      <ExportSuccessDialog
        show={exportHook.showExportSuccessDialog}
        onClose={() => exportHook.setShowExportSuccessDialog(false)}
        filePath={exportHook.lastExportedPath}
        artifacts={exportHook.lastExportArtifacts}
        onFilePathChange={onExportSuccessPathChange}
        autoCopyEnabled={exportHook.exportAutoCopyEnabled}
        onToggleAutoCopy={exportHook.setExportAutoCopyEnabled}
      />
      <HotkeyDialog show={showHotkeyDialog} onClose={onCloseHotkeyDialog} />
    </>
  );
}
