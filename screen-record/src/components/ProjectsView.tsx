import { useState, useRef, useEffect } from "react";
import { Video, Trash2, Play, X } from "lucide-react";
import { Project } from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import { useSettings } from "@/hooks/useSettings";
import { invoke } from "@/lib/ipc";
import { ConfirmDialog } from "./dialogs";

interface ProjectsViewProps {
  projects: Omit<Project, "videoBlob">[];
  onLoadProject: (projectId: string) => void | Promise<void>;
  onProjectsChange: () => void;
  onClose: () => void;
  currentProjectId?: string | null;
  restoreImage?: string | null;
  pickerMode?: "load" | "insertBefore" | "insertAfter";
  onPickProject?: (projectId: string) => void | Promise<void>;
}

function formatDuration(s: number): string {
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

/** Compute the object-fit:contain rect for an aspect ratio inside a container. */
function containRect(
  cw: number,
  ch: number,
  nw: number,
  nh: number,
): { left: number; top: number; width: number; height: number } {
  const ca = cw / ch,
    na = nw / nh;
  let w: number, h: number;
  if (ca > na) {
    h = ch;
    w = h * na;
  } else {
    w = cw;
    h = w / na;
  }
  return { left: (cw - w) / 2, top: (ch - h) / 2, width: w, height: h };
}

/** Resolve the rendered preview canvas rect relative to a parent, if present. */
function getPreviewCanvasRect(
  parent: HTMLElement,
): { left: number; top: number; width: number; height: number } | null {
  const canvas = parent.querySelector(
    ".preview-canvas-element",
  ) as HTMLCanvasElement | null;
  if (!canvas) return null;
  const parentRect = parent.getBoundingClientRect();
  const canvasRect = canvas.getBoundingClientRect();
  if (canvasRect.width <= 0 || canvasRect.height <= 0) return null;
  return {
    left: canvasRect.left - parentRect.left,
    top: canvasRect.top - parentRect.top,
    width: canvasRect.width,
    height: canvasRect.height,
  };
}
void getPreviewCanvasRect;

export function ProjectsView({
  projects,
  onLoadProject,
  onProjectsChange,
  onClose,
  currentProjectId,
  restoreImage,
  pickerMode = "load",
  onPickProject,
}: ProjectsViewProps) {
  const [editingNameId, setEditingNameId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [animatingId, setAnimatingId] = useState<string | null>(null);
  const [isRestoring, setIsRestoring] = useState(
    () => !!restoreImage && !!currentProjectId,
  );
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const animatingRef = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const animatedCloseRef = useRef<() => void>(() => {});
  const { t } = useSettings();
  const isPickerMode = pickerMode !== "load";

  // Restore animation: shrink current video frame into its card position
  useEffect(() => {
    if (!restoreImage || !currentProjectId || !containerRef.current) {
      setIsRestoring(false);
      return;
    }

    const container = containerRef.current;

    requestAnimationFrame(() => {
      const img = new Image();
      img.src = restoreImage;

      const runAnimation = () => {
        const canvasEl = document.querySelector(
          ".preview-canvas-element",
        ) as HTMLCanvasElement | null;
        const canvasRect = canvasEl?.getBoundingClientRect();
        const natW = img.naturalWidth || 16;
        const natH = img.naturalHeight || 9;
        let source: {
          left: number;
          top: number;
          width: number;
          height: number;
        };
        if (canvasRect && canvasRect.width > 0) {
          source = {
            left: canvasRect.left,
            top: canvasRect.top,
            width: canvasRect.width,
            height: canvasRect.height,
          };
        } else {
          source = containRect(
            window.innerWidth,
            Math.max(1, window.innerHeight - 44),
            natW,
            natH,
          );
          source.top += 44;
        }

        const card = container.querySelector(
          `[data-project-id="${currentProjectId}"]`,
        ) as HTMLElement | null;
        if (!card) {
          setIsRestoring(false);
          return;
        }

        // Ensure card is visible in scroll container
        card.scrollIntoView({
          block: "nearest",
          behavior: "instant" as ScrollBehavior,
        });

        const thumbArea = card.querySelector(
          ".project-thumbnail",
        ) as HTMLElement | null;
        if (!thumbArea) {
          setIsRestoring(false);
          return;
        }

        const thumbRect = thumbArea.getBoundingClientRect();

        // Clone at canvas (source) position
        const clone = document.createElement("div");
        clone.style.cssText = `
          position: absolute; z-index: 9999; pointer-events: none;
          left: ${source.left}px; top: ${source.top}px;
          width: ${source.width}px; height: ${source.height}px;
          overflow: hidden; transform-origin: 0 0;
          will-change: transform;
        `;
        const imgEl = document.createElement("img");
        imgEl.src = restoreImage;
        imgEl.style.cssText = "width: 100%; height: 100%; object-fit: cover;";
        clone.appendChild(imgEl);
        document.body.appendChild(clone);

        // Animate source → target (reverse of expand), body-relative coordinates
        const dx = thumbRect.left - source.left;
        const dy = thumbRect.top - source.top;
        const sx = thumbRect.width / source.width;
        const sy = thumbRect.height / source.height;

        // Compensate border-radius for scale so it visually appears as 8px after transform
        const thumbRadius = `${12 / sx}px / ${12 / sy}px`;

        clone.animate(
          [
            { transform: "none", borderRadius: "0px" },
            {
              transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
              borderRadius: thumbRadius,
            },
          ],
          {
            duration: 400,
            easing: "cubic-bezier(0.22, 1, 0.36, 1)",
            fill: "forwards",
          },
        ).onfinish = () => {
          clone.animate([{ opacity: 1 }, { opacity: 0 }], {
            duration: 100,
            fill: "forwards",
          }).onfinish = () => clone.remove();
        };

        // Fade in grid content
        setTimeout(() => setIsRestoring(false), 50);
      };

      if (img.complete) runAnimation();
      else {
        img.onload = runAnimation;
        img.onerror = () => setIsRestoring(false);
      }
    });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !animatingRef.current)
        animatedCloseRef.current();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // External close requests (toggle button dispatches this event)
  useEffect(() => {
    const handler = () => animatedCloseRef.current();
    window.addEventListener("sr-close-projects", handler);
    return () => window.removeEventListener("sr-close-projects", handler);
  }, []);

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

    const thumbnailImg = e.currentTarget.querySelector(
      "img",
    ) as HTMLImageElement | null;
    const container = containerRef.current;
    const portalTarget = document.querySelector(
      ".video-preview-container",
    ) as HTMLElement | null;

    if (!thumbnailImg || !container || !portalTarget) {
      onLoadProject(projectId);
      return;
    }

    const portalRect = portalTarget.getBoundingClientRect();
    const thumbRect = thumbnailImg.getBoundingClientRect();
    const canvasEl = document.querySelector(
      ".preview-canvas-element",
    ) as HTMLCanvasElement | null;
    const canvasRect = canvasEl?.getBoundingClientRect();
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;
    const target =
      projectId === currentProjectId && canvasRect && canvasRect.width > 0
        ? {
            left: canvasRect.left,
            top: canvasRect.top,
            width: canvasRect.width,
            height: canvasRect.height,
          }
        : containRect(portalRect.width, portalRect.height, natW, natH);
    const targetGlobal =
      projectId === currentProjectId && canvasRect && canvasRect.width > 0
        ? target
        : {
            left: portalRect.left + target.left,
            top: portalRect.top + target.top,
            width: target.width,
            height: target.height,
          };

    const clone = document.createElement("div");
    clone.style.cssText = `
      position: absolute; z-index: 9999; pointer-events: none;
      left: ${targetGlobal.left}px; top: ${targetGlobal.top}px;
      width: ${targetGlobal.width}px; height: ${targetGlobal.height}px;
      overflow: hidden; transform-origin: 0 0;
      will-change: transform;
    `;
    const img = document.createElement("img");
    img.src = thumbnailImg.src;
    img.style.cssText = "width: 100%; height: 100%; object-fit: cover;";
    clone.appendChild(img);
    document.body.appendChild(clone);

    animatingRef.current = true;
    setAnimatingId(projectId);

    const fadeOut = () => {
      clone.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: 200,
        fill: "forwards",
      }).onfinish = () => clone.remove();
    };

    const dx = thumbRect.left - targetGlobal.left;
    const dy = thumbRect.top - targetGlobal.top;
    const sx = thumbRect.width / targetGlobal.width;
    const sy = thumbRect.height / targetGlobal.height;
    const thumbRadius = `${12 / sx}px / ${12 / sy}px`;

    clone.animate(
      [
        {
          transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
          borderRadius: thumbRadius,
        },
        { transform: "none", borderRadius: "0px" },
      ],
      {
        duration: 500,
        easing: "cubic-bezier(0.22, 1, 0.36, 1)",
        fill: "forwards",
      },
    ).onfinish = () => {
      animatingRef.current = false;

      // Wait for project load and allow React lifecycle to completely stabilize canvas dimensions
      Promise.resolve(onLoadProject(projectId)).then(() => {
        setTimeout(() => requestAnimationFrame(fadeOut), 250);
      });
    };
  };

  // Animated close: expand current project's card → canvas, then unmount
  const handleAnimatedClose = () => {
    if (animatingRef.current) return;

    const container = containerRef.current;
    const portalTarget = document.querySelector(
      ".video-preview-container",
    ) as HTMLElement | null;

    if (!currentProjectId || !container || !portalTarget) {
      onClose();
      return;
    }

    const card = container.querySelector(
      `[data-project-id="${currentProjectId}"]`,
    ) as HTMLElement | null;
    const thumbnailImg = card?.querySelector(
      ".project-thumbnail img",
    ) as HTMLImageElement | null;

    if (!card || !thumbnailImg) {
      onClose();
      return;
    }

    card.scrollIntoView({
      block: "nearest",
      behavior: "instant" as ScrollBehavior,
    });

    const portalRect = portalTarget.getBoundingClientRect();
    const thumbRect = thumbnailImg.getBoundingClientRect();
    const canvasEl = document.querySelector(
      ".preview-canvas-element",
    ) as HTMLCanvasElement | null;
    const canvasRect = canvasEl?.getBoundingClientRect();
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;
    const fallbackTarget = containRect(
      portalRect.width,
      portalRect.height,
      natW,
      natH,
    );
    const targetGlobal =
      canvasRect && canvasRect.width > 0
        ? {
            left: canvasRect.left,
            top: canvasRect.top,
            width: canvasRect.width,
            height: canvasRect.height,
          }
        : {
            left: portalRect.left + fallbackTarget.left,
            top: portalRect.top + fallbackTarget.top,
            width: fallbackTarget.width,
            height: fallbackTarget.height,
          };
    const dx = thumbRect.left - targetGlobal.left;
    const dy = thumbRect.top - targetGlobal.top;
    const sx = thumbRect.width / targetGlobal.width;
    const sy = thumbRect.height / targetGlobal.height;

    const clone = document.createElement("div");
    clone.style.cssText = `
      position: absolute; z-index: 9999; pointer-events: none;
      left: ${targetGlobal.left}px; top: ${targetGlobal.top}px;
      width: ${targetGlobal.width}px; height: ${targetGlobal.height}px;
      overflow: hidden; transform-origin: 0 0;
      will-change: transform;
    `;
    const img = document.createElement("img");
    img.src = thumbnailImg.src;
    img.style.cssText = "width: 100%; height: 100%; object-fit: cover;";
    clone.appendChild(img);
    document.body.appendChild(clone);

    animatingRef.current = true;
    setAnimatingId(currentProjectId);

    const fadeOut = () => {
      clone.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: 200,
        fill: "forwards",
      }).onfinish = () => clone.remove();
    };

    // Compensate border-radius for scale so it visually appears as 8px after transform
    const thumbRadius = `${12 / sx}px / ${12 / sy}px`;

    clone.animate(
      [
        {
          transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
          borderRadius: thumbRadius,
        },
        { transform: "none", borderRadius: "0px" },
      ],
      {
        duration: 500,
        easing: "cubic-bezier(0.22, 1, 0.36, 1)",
        fill: "forwards",
      },
    ).onfinish = () => {
      animatingRef.current = false;
      onClose();
      setTimeout(() => requestAnimationFrame(fadeOut), 150);
    };
  };

  // Keep ref current so event listeners always call the latest version
  animatedCloseRef.current = handleAnimatedClose;

  return (
    <div
      ref={containerRef}
      className="projects-view absolute inset-0 z-20 overflow-hidden rounded-xl"
    >
      {/* Solid background — stays opaque while this component lives */}
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
        <div className="projects-header flex justify-between items-center px-6 py-4 flex-shrink-0 border-b border-[var(--glass-border)]">
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
                className="text-xs font-medium text-red-500 hover:text-red-400 bg-red-500/10 hover:bg-red-500/20 px-2.5 py-1 rounded transition-colors"
              >
                {t.clearAll}
              </button>
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
              className="p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Grid */}
        <div className="projects-grid-scroll flex-1 min-h-0 overflow-y-auto thin-scrollbar px-6 py-5">
          {projects.length === 0 ? (
            <div className="projects-empty-state flex items-center justify-center h-full text-xs text-[var(--outline)]">
              {t.noProjectsYet}
            </div>
          ) : (
            <div className="projects-grid grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3 items-start">
              {projects
                .filter(
                  (project) => !isPickerMode || project.id !== currentProjectId,
                )
                .map((project) => (
                  <div
                    key={project.id}
                    data-project-id={project.id}
                    className="project-card group relative bg-[var(--surface-container)] border border-[var(--glass-border)] rounded-lg overflow-hidden hover:border-[var(--outline)] transition-colors"
                  >
                    <div
                      className="project-thumbnail bg-[var(--surface-container-high)] relative cursor-pointer overflow-hidden"
                      onClick={(e) => handleProjectClick(project.id, e)}
                    >
                      {(project.id === currentProjectId && restoreImage) ||
                      project.thumbnail ? (
                        <img
                          src={
                            project.id === currentProjectId && restoreImage
                              ? restoreImage
                              : project.thumbnail
                          }
                          className="w-full block"
                          alt=""
                        />
                      ) : (
                        <div className="thumbnail-placeholder w-full aspect-video flex items-center justify-center">
                          <Video className="w-6 h-6 text-[var(--outline-variant)]" />
                        </div>
                      )}
                      <div className="thumbnail-hover-overlay absolute inset-0 bg-black/0 group-hover:bg-black/30 transition-colors flex items-center justify-center">
                        <Play className="w-7 h-7 text-white opacity-0 group-hover:opacity-90 transition-opacity" />
                      </div>
                      {project.duration != null && project.duration > 0 && (
                        <span
                          className="project-duration absolute bottom-2 right-2.5 text-white tabular-nums pointer-events-none"
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
                            className="bg-transparent border-b border-[var(--primary-color)] text-[var(--on-surface)] text-xs w-full outline-none py-0.5"
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
                          className="project-delete-btn text-[var(--outline)] hover:text-red-400 transition-colors opacity-0 group-hover:opacity-100 p-0.5 flex-shrink-0"
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      )}
                    </div>
                  </div>
                ))}
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
