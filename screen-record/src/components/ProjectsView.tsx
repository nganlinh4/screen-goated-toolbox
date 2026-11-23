import { useEffect, useState, useRef } from "react";
import { Video, Trash2, Play, X, Upload, AudioLines } from '@/components/ui/MaterialIcon';
import { Project } from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import { MAX_PROJECT_NAME_LENGTH } from "@/lib/projectMetadata";
import { useSettings } from "@/hooks/useSettings";
import { ConfirmDialog } from "./dialogs";
import { formatDuration } from "./projectsViewUtils";
import { useProjectsAnimations } from "./projectsViewAnimations";
import { useProjectLimitControl } from "./projectsViewLimit";

// Re-export types for backwards compatibility
export type {
  ProjectsPreviewRectSnapshot,
  ProjectsPreviewTargetSnapshot,
} from "./projectsViewUtils";

interface ProjectsViewProps {
  projects: Omit<Project, "videoBlob">[];
  onBeginProjectOpen?: () => void;
  onLoadProject: (projectId: string) => void | Promise<void>;
  onProjectsChange: () => void;
  onClearProjects: () => Promise<void>;
  onDeleteProject: (id: string) => Promise<void>;
  onRenameProject: (id: string, name: string) => Promise<void>;
  onClose: () => void;
  currentProjectId?: string | null;
  restoreImage?: string | null;
  previewTargetSnapshot?: import("./projectsViewUtils").ProjectsPreviewTargetSnapshot | null;
  pickerMode?: "load" | "insertBefore" | "insertAfter";
  onPickProject?: (projectId: string) => void | Promise<void>;
  onImportVideo?: (file: File) => void;
  onImportAudio?: (file: File) => void;
}

export function ProjectsView({
  projects,
  onBeginProjectOpen,
  onLoadProject,
  onProjectsChange,
  onClearProjects,
  onDeleteProject,
  onRenameProject,
  onClose,
  currentProjectId,
  restoreImage,
  previewTargetSnapshot,
  pickerMode = "load",
  onPickProject,
  onImportVideo,
  onImportAudio,
}: ProjectsViewProps) {
  const [editingNameId, setEditingNameId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<{ id: string; name: string } | null>(null);
  // Enter commits and then the unmounting input blurs; only the first commit writes.
  const renameSettledRef = useRef(true);
  const {
    draftLimit,
    sliderHandlers: limitSliderHandlers,
    pendingReduction,
    confirmReduction,
    cancelReduction,
  } = useProjectLimitControl({ projects, currentProjectId, onProjectsChange, onDeleteProject });
  const containerRef = useRef<HTMLDivElement>(null);
  const { t } = useSettings();
  const isPickerMode = pickerMode !== "load";

  useEffect(() => {
    const refreshProjects = () => onProjectsChange();
    window.addEventListener("sr-project-retention-changed", refreshProjects);
    return () => window.removeEventListener("sr-project-retention-changed", refreshProjects);
  }, [onProjectsChange]);

  const {
    animatingId,
    isRestoring,
    hiddenProjectCardId,
    animatingRef,
    handleProjectClick: handleProjectClickAnim,
    handleAnimatedClose,
  } = useProjectsAnimations({
    projects,
    currentProjectId,
    restoreImage,
    previewTargetSnapshot,
    onLoadProject,
    onClose,
    containerRef,
  });

  const startRename = (id: string, name: string) => {
    renameSettledRef.current = false;
    setEditingNameId(id);
    setRenameValue(name);
  };

  const cancelRename = () => {
    renameSettledRef.current = true;
    setEditingNameId(null);
  };

  const handleRename = async (id: string) => {
    if (renameSettledRef.current) return;
    renameSettledRef.current = true;
    setEditingNameId(null);
    await onRenameProject(id, renameValue);
  };

  const confirmDelete = async () => {
    const target = pendingDelete;
    setPendingDelete(null);
    if (target) await onDeleteProject(target.id);
  };

  const handleClearAll = async () => {
    setShowClearConfirm(false);
    await onClearProjects();
  };

  const handleProjectClick = (
    projectId: string,
    e: React.MouseEvent<HTMLButtonElement>,
  ) => {
    if (isPickerMode) {
      void onPickProject?.(projectId);
      return;
    }
    if (animatingRef.current) return;
    handleProjectClickAnim(projectId, e);
  };

  return (
    <div
      ref={containerRef}
      className="projects-view absolute inset-0 z-20 overflow-hidden rounded-xl"
    >
      {/* Solid background -- stays opaque while this component lives */}
      <div className="projects-background absolute inset-0 bg-[var(--surface)]" />

      {/* Content fades out during thumbnail expansion */}
      <div
        className={`projects-content relative flex flex-col h-full transition-opacity ${
          animatingId
            ? "opacity-0 duration-200"
            : isRestoring
              ? "opacity-0"
              : "opacity-100 duration-300"
        }`}
      >
        {/* Header */}
        <div className="projects-header flex justify-between items-center px-6 py-4 shrink-0 border-[var(--ui-border)] ">
          <div className="flex items-center gap-4">
            <h3 className="text-lg font-semibold text-[var(--on-surface)]">
              {isPickerMode
                ? pickerMode === "insertBefore"
                  ? t.insertProjectBefore
                  : t.insertProjectAfter
                : t.projects}
            </h3>
            {!isPickerMode && projects.length > 0 && (
              <button
                onClick={() => setShowClearConfirm(true)}
                className="projects-clear-btn rounded-lg px-2.5 py-1 text-xs font-medium text-red-500/80 hover:text-red-400 hover:bg-red-500/10 transition-colors"
              >
                {t.clearAll}
              </button>
            )}
            {!isPickerMode && onImportVideo && (
              <label className="projects-import-btn ui-chip-button rounded-lg px-2.5 py-1 text-xs font-medium cursor-pointer flex items-center gap-1.5 text-[var(--primary-color)] hover:text-[var(--primary-color)]">
                <Upload className="w-3 h-3" />
                {t.importVideo}
                <input type="file" accept="video/*" className="hidden" onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) onImportVideo(file);
                  e.target.value = "";
                }} />
              </label>
            )}
            {!isPickerMode && onImportAudio && (
              <label className="projects-import-audio-btn ui-chip-button rounded-lg px-2.5 py-1 text-xs font-medium cursor-pointer flex items-center gap-1.5 text-[var(--primary-color)] hover:text-[var(--primary-color)]">
                <AudioLines className="w-3 h-3" />
                {t.importAudio}
                <input type="file" accept="audio/*" className="hidden" onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) onImportAudio(file);
                  e.target.value = "";
                }} />
              </label>
            )}
          </div>
          <div className="projects-limit-control flex items-center gap-4 shrink-0 whitespace-nowrap">
            {!isPickerMode && (
              <>
                <label htmlFor="projects-limit" className="text-xs text-[var(--on-surface-variant)]">
                  {t.max}
                </label>
                <input
                  id="projects-limit"
                  aria-label={t.max}
                  type="range"
                  min="10"
                  max="100"
                  value={draftLimit}
                  {...limitSliderHandlers}
                  className="projects-limit-slider w-16"
                />
                <span className="text-xs text-[var(--on-surface)] tabular-nums w-5">
                  {draftLimit}
                </span>
              </>
            )}
            <button
              type="button"
              aria-label={t.close}
              title={t.close}
              onClick={handleAnimatedClose}
              className="projects-close-btn ui-icon-button p-1"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {!isPickerMode && projects.length > projectManager.getLimit() && (
          <p role="status" className="projects-retention-notice px-6 text-xs text-[var(--on-surface-variant)]">
            {t.projectLimitPreservedMedia}
          </p>
        )}

        {/* Grid */}
        <div className="projects-grid-scroll flex-1 min-h-0 overflow-y-auto thin-scrollbar px-6 py-5">
          {projects.length === 0 ? (
            <div className="projects-empty-state ui-empty-state flex items-center justify-center h-full rounded-2xl text-xs">
              {t.noProjectsYet}
            </div>
          ) : (
            <div className="projects-grid grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3 items-start">
              {projects
                .filter(
                  (project) => !isPickerMode || project.id !== currentProjectId,
                )
                .map((project) => {
                  const isThumbnailMasked = project.id === hiddenProjectCardId;
                  const thumbnailSrc =
                    project.id === currentProjectId && restoreImage
                      ? restoreImage
                      : project.thumbnail;

                  return (
                  <div
                    key={project.id}
                    data-project-id={project.id}
                    className={`project-card ui-surface group relative rounded-xl overflow-hidden ${
                      isThumbnailMasked ? "pointer-events-none" : ""
                    }`}
                  >
                    <button
                      type="button"
                      aria-label={project.name}
                      className="project-thumbnail block w-full border-0 p-0 text-left bg-[var(--surface-container-high)] relative cursor-pointer overflow-hidden"
                      onMouseDownCapture={() => {
                        if (!isPickerMode) {
                          onBeginProjectOpen?.();
                        }
                      }}
                      onClick={(e) => handleProjectClick(project.id, e)}
                    >
                      {thumbnailSrc ? (
                        <img
                          src={thumbnailSrc}
                          className={`w-full block ${
                            isThumbnailMasked ? "opacity-0" : "opacity-100"
                          }`}
                          alt=""
                        />
                      ) : (
                        <div className="thumbnail-placeholder w-full aspect-video flex items-center justify-center">
                          <Video className="w-6 h-6 text-[var(--outline-variant)]" />
                        </div>
                      )}
                      <div
                        className={`thumbnail-hover-overlay absolute inset-0 bg-black/0 transition-colors flex items-center justify-center ${
                          isThumbnailMasked
                            ? "opacity-0"
                            : "group-hover:bg-black/30"
                        }`}
                      >
                        <Play className="w-7 h-7 text-white opacity-0 group-hover:opacity-90 transition-opacity" />
                      </div>
                      {project.duration != null && project.duration > 0 && (
                        <span
                          className={`project-duration absolute bottom-2 right-2.5 text-white tabular-nums pointer-events-none ${
                            isThumbnailMasked ? "opacity-0" : "opacity-100"
                          }`}
                          style={{
                            fontSize: "1.35rem",
                            fontVariationSettings: "'wght' 700, 'ROND' 100",
                            textShadow:
                              "0 1px 4px rgba(0,0,0,0.7), 0 0 12px rgba(0,0,0,0.4)",
                            letterSpacing: "-0.02em",
                          }}
                        >
                          {formatDuration(project.duration)}
                        </span>
                      )}
                    </button>
                    <div className="project-card-footer p-2 flex items-start justify-between gap-1">
                      <div className="project-info min-w-0 flex-1">
                        {editingNameId === project.id ? (
                          <input
                            autoFocus
                            className="project-rename-input ui-input w-full rounded-md border-b border-[var(--primary-color)] text-[var(--on-surface)] text-xs outline-hidden py-1 px-1.5"
                            value={renameValue}
                            maxLength={MAX_PROJECT_NAME_LENGTH}
                            onChange={(e) => setRenameValue(e.target.value)}
                            onBlur={() => handleRename(project.id)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") void handleRename(project.id);
                              if (e.key === "Escape") {
                                e.preventDefault();
                                cancelRename();
                              }
                            }}
                          />
                        ) : (
                          <button
                            type="button"
                            aria-label={`${t.rename}: ${project.name}`}
                            className="project-rename-trigger block w-full border-0 bg-transparent p-0 text-left text-xs text-[var(--on-surface)] truncate cursor-pointer hover:text-[var(--primary-color)] transition-colors"
                            onClick={() => startRename(project.id, project.name)}
                          >
                            {project.name}
                          </button>
                        )}
                        <p className="text-[10px] text-[var(--outline)] mt-0.5">
                          {new Date(project.lastModified).toLocaleDateString()}
                        </p>
                      </div>
                      {!isPickerMode && (
                        <button
                          type="button"
                          aria-label={t.deleteProjectNamedLabel.replace('{name}', project.name)}
                          onClick={(e) => {
                            e.stopPropagation();
                            setPendingDelete({ id: project.id, name: project.name });
                          }}
                          className="project-delete-btn ui-icon-button text-[var(--outline)] hover:text-red-400 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 p-0.5 shrink-0"
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      )}
                    </div>
                  </div>
                )})}
            </div>
          )}
        </div>
      </div>
      <ConfirmDialog
        show={showClearConfirm}
        title={t.confirmClearAllTitle}
        message={t.confirmClearAllDesc}
        onConfirm={handleClearAll}
        onCancel={() => setShowClearConfirm(false)}
        confirmLabel={t.clearAll}
      />
      <ConfirmDialog
        show={pendingDelete !== null}
        title={t.confirmDeleteProjectTitle}
        message={pendingDelete ? `${pendingDelete.name}: ${t.confirmDeleteProjectDesc}` : ""}
        onConfirm={() => void confirmDelete()}
        onCancel={() => setPendingDelete(null)}
        confirmLabel={t.deleteAction}
      />
      <ConfirmDialog
        show={pendingReduction !== null}
        title={t.confirmProjectLimitTitle}
        message={pendingReduction
          ? t.confirmProjectLimitDesc
            .replace('{limit}', String(pendingReduction.limit))
            .replace('{count}', String(pendingReduction.projectIds.length))
          : ""}
        onConfirm={() => void confirmReduction()}
        onCancel={cancelReduction}
        confirmLabel={t.deleteAction}
      />
    </div>
  );
}
