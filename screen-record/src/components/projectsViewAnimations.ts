import { useEffect, useRef, useState } from "react";
import { Project } from "@/types/video";
import {
  PROJECTS_FLIP_EASING,
  PROJECTS_FLIP_SETTLE_EASING,
  type ProjectsPreviewTargetSnapshot,
  containRect,
  getProjectPreviewTargetRect,
  getProjectThumbnailRadius,
  rectSnapshotToDomRect,
  getPreviewStageRect,
  getLiveCanvasRect,
  getOptionalElementRect,
  toRectSnapshot,
  logProjectsFlip,
} from "./projectsViewUtils";

interface UseProjectsAnimationsOptions {
  projects: Omit<Project, "videoBlob">[];
  currentProjectId?: string | null;
  restoreImage?: string | null;
  previewTargetSnapshot?: ProjectsPreviewTargetSnapshot | null;
  onLoadProject: (projectId: string) => void | Promise<void>;
  onClose: () => void;
  containerRef: React.RefObject<HTMLDivElement | null>;
}

export function useProjectsAnimations({
  projects,
  currentProjectId,
  restoreImage,
  previewTargetSnapshot,
  onLoadProject,
  onClose,
  containerRef,
}: UseProjectsAnimationsOptions) {
  const [animatingId, setAnimatingId] = useState<string | null>(null);
  const [isRestoring, setIsRestoring] = useState(
    () => !!restoreImage && !!currentProjectId,
  );
  const [hiddenProjectCardId, setHiddenProjectCardId] = useState<string | null>(
    () => (restoreImage && currentProjectId ? currentProjectId : null),
  );
  const animatingRef = useRef(false);
  const animatedCloseRef = useRef<() => void>(() => {});

  // Restore animation: shrink current video frame into its card position
  useEffect(() => {
    if (!restoreImage || !currentProjectId || !containerRef.current) {
      setIsRestoring(false);
      setHiddenProjectCardId(null);
      return;
    }

    const container = containerRef.current;
    setHiddenProjectCardId(currentProjectId);

    // Place the clone IMMEDIATELY so it covers the preview area from the
    // very first frame — prevents any blink.  The snapshot rect was captured
    // before the dialog opened, so it's always available synchronously.
    const snapshotCanvas = previewTargetSnapshot?.canvasRect;
    const canvasEl = document.querySelector(
      ".preview-canvas-element",
    ) as HTMLCanvasElement | null;
    const canvasRect = canvasEl?.getBoundingClientRect();
    const imgObj = new Image();
    imgObj.src = restoreImage;
    const natW = imgObj.naturalWidth || 16;
    const natH = imgObj.naturalHeight || 9;

    let source: { left: number; top: number; width: number; height: number };
    if (snapshotCanvas && snapshotCanvas.width > 0) {
      source = { left: snapshotCanvas.left, top: snapshotCanvas.top, width: snapshotCanvas.width, height: snapshotCanvas.height };
    } else if (canvasRect && canvasRect.width > 0) {
      source = { left: canvasRect.left, top: canvasRect.top, width: canvasRect.width, height: canvasRect.height };
    } else {
      source = containRect(window.innerWidth, Math.max(1, window.innerHeight - 44), natW, natH);
      source.top += 44;
    }

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

    // Make content visible NOW while the clone covers it.  This triggers the
    // heavy React re-render (opacity-0 → opacity-1) BEFORE the animation starts,
    // so no re-render competes with the animation frames.
    setIsRestoring(false);

    const startAnimation = () => {
      const card = container.querySelector(
        `[data-project-id="${currentProjectId}"]`,
      ) as HTMLElement | null;
      if (!card) {
        clone.remove();
        setIsRestoring(false);
        setHiddenProjectCardId(null);
        return;
      }

      card.scrollIntoView({ block: "nearest", behavior: "instant" as ScrollBehavior });

      const thumbArea = card.querySelector(".project-thumbnail") as HTMLElement | null;
      if (!thumbArea) {
        clone.remove();
        setIsRestoring(false);
        setHiddenProjectCardId(null);
        return;
      }

      const thumbRect = thumbArea.getBoundingClientRect();
      const dx = thumbRect.left - source.left;
      const dy = thumbRect.top - source.top;
      const sx = thumbRect.width / source.width;
      const sy = thumbRect.height / source.height;
      const thumbRadius = getProjectThumbnailRadius(sx, sy);

      clone.animate(
        [
          { transform: "none", borderRadius: "0px" },
          {
            transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
            borderRadius: thumbRadius,
          },
        ],
        {
          duration: 520,
          easing: PROJECTS_FLIP_EASING,
          fill: "forwards",
        },
      ).onfinish = () => {
        setHiddenProjectCardId(null);
        requestAnimationFrame(() => {
          clone.animate([{ opacity: 1 }, { opacity: 0 }], {
            duration: 120,
            easing: PROJECTS_FLIP_SETTLE_EASING,
            fill: "forwards",
          }).onfinish = () => {
            clone.remove();
          };
        });
      };

    };

    // Wait for the content re-render + paint to fully finish,
    // THEN start the animation on a clean frame budget.
    const tryStart = () => requestAnimationFrame(() => setTimeout(() => {
      requestAnimationFrame(() => setTimeout(startAnimation, 0));
    }, 0));
    if (imgObj.complete) tryStart();
    else {
      imgObj.onload = tryStart;
      imgObj.onerror = () => {
        clone.remove();
        setIsRestoring(false);
        setHiddenProjectCardId(null);
      };
    }
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !animatingRef.current)
        animatedCloseRef.current();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    const handler = () => animatedCloseRef.current();
    window.addEventListener("sr-close-projects", handler);
    return () => window.removeEventListener("sr-close-projects", handler);
  }, []);

  const handleProjectClick = (
    projectId: string,
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
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
          onClose();
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              clone.remove();
            });
          });
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
          duration: 210,
          easing: PROJECTS_FLIP_SETTLE_EASING,
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
    const thumbRadius = getProjectThumbnailRadius(sx, sy);

    clone.animate(
      [
        {
          transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
          borderRadius: thumbRadius,
        },
        { transform: "none", borderRadius: "0px" },
      ],
      {
        duration: 640,
        easing: PROJECTS_FLIP_EASING,
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
    const imgClone = document.createElement("img");
    imgClone.src = thumbnailImg.src;
    imgClone.style.cssText = "width: 100%; height: 100%; object-fit: cover;";
    clone.appendChild(imgClone);
    document.body.appendChild(clone);

    animatingRef.current = true;
    setAnimatingId(currentProjectId);

    const fadeOut = () => {
      clone.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: 200,
        easing: PROJECTS_FLIP_SETTLE_EASING,
        fill: "forwards",
      }).onfinish = () => clone.remove();
    };

    const thumbRadius = getProjectThumbnailRadius(sx, sy);
    clone.animate(
      [
        {
          transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
          borderRadius: thumbRadius,
        },
        { transform: "none", borderRadius: "0px" },
      ],
      {
        duration: 620,
        easing: PROJECTS_FLIP_EASING,
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

  return {
    animatingId,
    isRestoring,
    hiddenProjectCardId,
    animatingRef,
    handleProjectClick,
    handleAnimatedClose,
  };
}
