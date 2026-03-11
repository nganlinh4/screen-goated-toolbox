import { useState, useRef, useEffect } from "react";
import { Video, Trash2, Play, X } from "lucide-react";
import { Project } from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import { useSettings } from "@/hooks/useSettings";
import { invoke } from "@/lib/ipc";
import { ConfirmDialog } from "./dialogs";

const PROJECTS_FLIP_DEBUG = false;

export interface ProjectsPreviewRectSnapshot {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ProjectsPreviewTargetSnapshot {
  stageRect: ProjectsPreviewRectSnapshot | null;
  canvasRect: ProjectsPreviewRectSnapshot | null;
}

interface ProjectsViewProps {
  projects: Omit<Project, "videoBlob">[];
  onBeginProjectOpen?: () => void;
  onLoadProject: (projectId: string) => void | Promise<void>;
  onProjectsChange: () => void;
  onClose: () => void;
  currentProjectId?: string | null;
  restoreImage?: string | null;
  previewTargetSnapshot?: ProjectsPreviewTargetSnapshot | null;
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

function getProjectPreviewTargetRect(
  cw: number,
  ch: number,
  project: Omit<Project, "videoBlob"> | undefined,
  fallbackWidth: number,
  fallbackHeight: number,
): { left: number; top: number; width: number; height: number } {
  const canvasWidth = project?.backgroundConfig?.canvasWidth;
  const canvasHeight = project?.backgroundConfig?.canvasHeight;
  if (
    typeof canvasWidth === "number" &&
    canvasWidth > 0 &&
    typeof canvasHeight === "number" &&
    canvasHeight > 0
  ) {
    return containRect(cw, ch, canvasWidth, canvasHeight);
  }
  return containRect(cw, ch, fallbackWidth, fallbackHeight);
}

function rectSnapshotToDomRect(
  rect: ProjectsPreviewRectSnapshot | null | undefined,
): DOMRect | null {
  if (!rect || rect.width <= 0 || rect.height <= 0) return null;
  return new DOMRect(rect.left, rect.top, rect.width, rect.height);
}

function getPreviewStageRect(
  snapshot?: ProjectsPreviewTargetSnapshot | null,
): DOMRect | null {
  const snapshotRect = rectSnapshotToDomRect(snapshot?.stageRect);
  if (snapshotRect) return snapshotRect;
  const previewStage = document.querySelector(
    ".preview-canvas",
  ) as HTMLElement | null;
  if (!previewStage) return null;
  const rect = previewStage.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

function getLiveCanvasRect(): DOMRect | null {
  const canvas = document.querySelector(
    ".preview-canvas-element",
  ) as HTMLCanvasElement | null;
  if (!canvas) return null;
  const rect = canvas.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

function getOptionalElementRect(selector: string): DOMRect | null {
  const element = document.querySelector(selector) as HTMLElement | null;
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return rect;
}

function toRectSnapshot(rect: DOMRect | null) {
  if (!rect) return null;
  return {
    left: Number(rect.left.toFixed(2)),
    top: Number(rect.top.toFixed(2)),
    width: Number(rect.width.toFixed(2)),
    height: Number(rect.height.toFixed(2)),
  };
}

function logProjectsFlip(
  event: string,
  details: Record<string, unknown> = {},
) {
  if (!PROJECTS_FLIP_DEBUG) return;
  console.log("[ProjectsFlip]", { event, ...details });
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
  onBeginProjectOpen,
  onLoadProject,
  onProjectsChange,
  onClose,
  currentProjectId,
  restoreImage,
  previewTargetSnapshot,
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
    const portalRect = getPreviewStageRect(previewTargetSnapshot);

    if (!thumbnailImg || !container || !portalRect) {
      onLoadProject(projectId);
      return;
    }

    const thumbRect = thumbnailImg.getBoundingClientRect();
    const canvasRect =
      rectSnapshotToDomRect(previewTargetSnapshot?.canvasRect) ??
      getLiveCanvasRect();
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;
    const targetProject = projects.find((project) => project.id === projectId);
    const target =
      projectId === currentProjectId && canvasRect && canvasRect.width > 0
        ? {
            left: canvasRect.left,
            top: canvasRect.top,
            width: canvasRect.width,
            height: canvasRect.height,
          }
        : getProjectPreviewTargetRect(
            portalRect.width,
            portalRect.height,
            targetProject,
            natW,
            natH,
          );
    const targetGlobal =
      projectId === currentProjectId && canvasRect && canvasRect.width > 0
        ? target
        : {
            left: portalRect.left + target.left,
            top: portalRect.top + target.top,
            width: target.width,
            height: target.height,
          };

    logProjectsFlip("load-estimate", {
      projectId,
      projectName: targetProject?.name,
      thumbnailNaturalSize: { width: natW, height: natH },
      previewStageRect: toRectSnapshot(portalRect),
      stablePreviewStageRect: previewTargetSnapshot?.stageRect ?? null,
      preLoadCanvasRect: toRectSnapshot(canvasRect),
      stableCanvasRect: previewTargetSnapshot?.canvasRect ?? null,
      estimatedTargetRect: {
        left: Number(targetGlobal.left.toFixed(2)),
        top: Number(targetGlobal.top.toFixed(2)),
        width: Number(targetGlobal.width.toFixed(2)),
        height: Number(targetGlobal.height.toFixed(2)),
      },
      canvasConfig: {
        mode: targetProject?.backgroundConfig?.canvasMode ?? null,
        width: targetProject?.backgroundConfig?.canvasWidth ?? null,
        height: targetProject?.backgroundConfig?.canvasHeight ?? null,
        scale: targetProject?.backgroundConfig?.scale ?? null,
      },
    });

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

    const finishProjectOpen = () => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          clone.remove();
          onClose();
        });
      });
    };

    const settleCloneToLiveCanvas = () => {
      const liveCanvasRect = getLiveCanvasRect();
      const livePreviewStageRect = getPreviewStageRect();
      if (!liveCanvasRect) {
        logProjectsFlip("load-settle-missing-live-canvas", { projectId });
        finishProjectOpen();
        return;
      }
      const cloneRect = clone.getBoundingClientRect();
      const deltaLeft = liveCanvasRect.left - cloneRect.left;
      const deltaTop = liveCanvasRect.top - cloneRect.top;
      const widthDelta = Math.abs(liveCanvasRect.width - cloneRect.width);
      const heightDelta = Math.abs(liveCanvasRect.height - cloneRect.height);
      const liveCanvasEl = document.querySelector(
        ".preview-canvas-element",
      ) as HTMLCanvasElement | null;
      logProjectsFlip("load-settle", {
        projectId,
        estimatedTargetRect: toRectSnapshot(
          new DOMRect(
            targetGlobal.left,
            targetGlobal.top,
            targetGlobal.width,
            targetGlobal.height,
          ),
        ),
        cloneRect: toRectSnapshot(cloneRect),
        liveCanvasRect: toRectSnapshot(liveCanvasRect),
        livePreviewStageRect: toRectSnapshot(livePreviewStageRect),
        livePreviewSurfaceRect: toRectSnapshot(
          getOptionalElementRect(".video-preview-container"),
        ),
        livePlaybackControlsRect: toRectSnapshot(
          getOptionalElementRect(".playback-controls-row"),
        ),
        liveSequenceBreadcrumbRect: toRectSnapshot(
          getOptionalElementRect(".sequence-focus-breadcrumb"),
        ),
        liveTimelineRect: toRectSnapshot(
          getOptionalElementRect(".timeline-container"),
        ),
        delta: {
          left: Number(deltaLeft.toFixed(2)),
          top: Number(deltaTop.toFixed(2)),
          width: Number(widthDelta.toFixed(2)),
          height: Number(heightDelta.toFixed(2)),
        },
        previewStageDelta: livePreviewStageRect
          ? {
              width: Number(
                (livePreviewStageRect.width - portalRect.width).toFixed(2),
              ),
              height: Number(
                (livePreviewStageRect.height - portalRect.height).toFixed(2),
              ),
              left: Number(
                (livePreviewStageRect.left - portalRect.left).toFixed(2),
              ),
              top: Number(
                (livePreviewStageRect.top - portalRect.top).toFixed(2),
              ),
            }
          : null,
        liveCanvasIntrinsicSize: liveCanvasEl
          ? {
              width: liveCanvasEl.width,
              height: liveCanvasEl.height,
              aspectRatio: liveCanvasEl.style.aspectRatio || null,
            }
          : null,
        canvasConfig: {
          mode: targetProject?.backgroundConfig?.canvasMode ?? null,
          width: targetProject?.backgroundConfig?.canvasWidth ?? null,
          height: targetProject?.backgroundConfig?.canvasHeight ?? null,
          scale: targetProject?.backgroundConfig?.scale ?? null,
        },
      });
      if (
        Math.abs(deltaLeft) < 0.5 &&
        Math.abs(deltaTop) < 0.5 &&
        widthDelta < 0.5 &&
        heightDelta < 0.5
      ) {
        finishProjectOpen();
        return;
      }
      clone.animate(
        [
          {
            left: `${cloneRect.left}px`,
            top: `${cloneRect.top}px`,
            width: `${cloneRect.width}px`,
            height: `${cloneRect.height}px`,
          },
          {
            left: `${liveCanvasRect.left}px`,
            top: `${liveCanvasRect.top}px`,
            width: `${liveCanvasRect.width}px`,
            height: `${liveCanvasRect.height}px`,
          },
        ],
        {
          duration: 140,
          easing: "cubic-bezier(0.22, 1, 0.36, 1)",
          fill: "forwards",
        },
      ).onfinish = () => {
        clone.style.left = `${liveCanvasRect.left}px`;
        clone.style.top = `${liveCanvasRect.top}px`;
        clone.style.width = `${liveCanvasRect.width}px`;
        clone.style.height = `${liveCanvasRect.height}px`;
        finishProjectOpen();
      };
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

      Promise.resolve(onLoadProject(projectId)).then(() => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            settleCloneToLiveCanvas();
          });
        });
      });
    };
  };

  // Animated close: expand current project's card → canvas, then unmount
  const handleAnimatedClose = () => {
    if (animatingRef.current) return;

    const container = containerRef.current;
    const portalRect = getPreviewStageRect();

    if (!currentProjectId || !container || !portalRect) {
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

    const thumbRect = thumbnailImg.getBoundingClientRect();
    const canvasRect = getLiveCanvasRect();
    const natW = thumbnailImg.naturalWidth || 16;
    const natH = thumbnailImg.naturalHeight || 9;
    const currentProject = projects.find((project) => project.id === currentProjectId);
    const fallbackTarget = getProjectPreviewTargetRect(
      portalRect.width,
      portalRect.height,
      currentProject,
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
                className="projects-clear-btn ui-chip-button rounded-lg px-2.5 py-1 text-xs font-medium text-red-500 hover:text-red-400"
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
                .map((project) => (
                  <div
                    key={project.id}
                    data-project-id={project.id}
                    className="project-card ui-surface group relative rounded-xl overflow-hidden"
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
