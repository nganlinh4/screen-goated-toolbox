import { useState, useRef } from "react";
import { Video, Trash2, Play, X, Upload } from "lucide-react";
import { Project } from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import { useSettings } from "@/hooks/useSettings";
import { invoke } from "@/lib/ipc";
import { ConfirmDialog } from "./dialogs";
import { formatDuration } from "./projectsViewUtils";
import { useProjectsAnimations } from "./projectsViewAnimations";

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
  onClose: () => void;
  currentProjectId?: string | null;
  restoreImage?: string | null;
  previewTargetSnapshot?: import("./projectsViewUtils").ProjectsPreviewTargetSnapshot | null;
  pickerMode?: "load" | "insertBefore" | "insertAfter";
  onPickProject?: (projectId: string) => void | Promise<void>;
  onImportVideo?: (file: File) => void;
}

export function ProjectsView({
  projects,
  onBeginProjectOpen,
  onLoadProject,
  onProjectsChange,
  onClose,
  currentProjectId,
  restoreImage,
  previewTargetSnapshot,
  pickerMode = "load",
  onPickProject,
  onImportVideo,
}: ProjectsViewProps) {
  const [editingNameId, setEditingNameId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const { t } = useSettings();
  const isPickerMode = pickerMode !== "load";

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

  const handleRename = async (id: string) => {
    if (!renameValue.trim()) return;
    const project = await projectManager.loadProject(id);
    if (project) {
      await projectManager.updateProject(id, {
        ...project,
        name: renameValue.trim(),
      });
      onProjectsChange();
    }
    setEditingNameId(null);
  };

  const handleClearAll = async () => {
    setShowClearConfirm(false);
    for (const p of projects) {
      await projectManager.deleteProject(p.id);
      if (p.rawVideoPath) {
        try {
          await invoke("delete_file", { path: p.rawVideoPath });
        } catch {}
      }
    }
    onProjectsChange();
  };

  const handleProjectClick = (
    projectId: string,
    e: React.MouseEvent<HTMLDivElement>,
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
        <div className="projects-header flex justify-between items-center px-6 py-4 flex-shrink-0 border-[var(--ui-border)] ">
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
          </div>
          <div className="projects-limit-control flex items-center gap-4 flex-shrink-0 whitespace-nowrap">
            {!isPickerMode && (
              <>
                <span className="text-[10px] text-[var(--outline)]">
                  {t.max}
                </span>
                <input
                  type="range"
                  min="10"
                  max="100"
                  value={projectManager.getLimit()}
                  onChange={(e) => {
                    projectManager.setLimit(parseInt(e.target.value));
                    onProjectsChange();
                  }}
                  className="w-16"
                />
                <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-5">
                  {projectManager.getLimit()}
                </span>
              </>
            )}
            <button
              onClick={handleAnimatedClose}
              className="projects-close-btn ui-icon-button p-1"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

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
                    <div
                      className="project-thumbnail bg-[var(--surface-container-high)] relative cursor-pointer overflow-hidden"
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
                    </div>
                    <div className="project-card-footer p-2 flex items-start justify-between gap-1">
                      <div className="project-info min-w-0 flex-1">
                        {editingNameId === project.id ? (
                          <input
                            autoFocus
                            className="project-rename-input ui-input w-full rounded-md border-b border-[var(--primary-color)] text-[var(--on-surface)] text-xs outline-none py-1 px-1.5"
                            value={renameValue}
                            onChange={(e) => setRenameValue(e.target.value)}
                            onBlur={() => handleRename(project.id)}
                            onKeyDown={(e) =>
                              e.key === "Enter" && handleRename(project.id)
                            }
                          />
                        ) : (
                          <p
                            className="text-xs text-[var(--on-surface)] truncate cursor-pointer hover:text-[var(--primary-color)] transition-colors"
                            onClick={() => {
                              setEditingNameId(project.id);
                              setRenameValue(project.name);
                            }}
                          >
                            {project.name}
                          </p>
                        )}
                        <p className="text-[10px] text-[var(--outline)] mt-0.5">
                          {new Date(project.lastModified).toLocaleDateString()}
                        </p>
                      </div>
                      {!isPickerMode && (
                        <button
                          onClick={async (e) => {
                            e.stopPropagation();
                            await projectManager.deleteProject(project.id);
                            if (project.rawVideoPath) {
                              try {
                                await invoke("delete_file", {
                                  path: project.rawVideoPath,
                                });
                              } catch {}
                            }
                            onProjectsChange();
                          }}
                          className="project-delete-btn ui-icon-button text-[var(--outline)] hover:text-red-400 opacity-0 group-hover:opacity-100 p-0.5 flex-shrink-0"
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
      />
    </div>
  );
}
