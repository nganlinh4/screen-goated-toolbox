import {
  PROJECTS_FLIP_EASING,
  PROJECTS_FLIP_SETTLE_EASING,
  type ProjectsPreviewTargetSnapshot,
  containRect,
  getProjectThumbnailRadius,
} from "./projectsViewUtils";

const HEADER_HEIGHT_PX = 44;

export interface ProjectRestoreAnimationOptions {
  container: HTMLElement;
  projectId: string;
  restoreImage: string;
  snapshotCanvas?: ProjectsPreviewTargetSnapshot["canvasRect"] | null;
  /** The clone covers the preview, so the grid can become visible underneath it. */
  onRevealContent: () => void;
  /** The clone has landed on (or cannot reach) the card, so the card can show. */
  onShowCard: () => void;
}

/**
 * Shrinks a snapshot of the current preview into its project card.
 * Returns a disposer that cancels every pending frame, timer and animation
 * and removes the clone, so closing the view mid-restore leaves nothing behind.
 */
export function startProjectRestoreAnimation({
  container,
  projectId,
  restoreImage,
  snapshotCanvas,
  onRevealContent,
  onShowCard,
}: ProjectRestoreAnimationOptions): () => void {
  let disposed = false;
  const frames = new Set<number>();
  const timers = new Set<number>();
  const animations: Animation[] = [];
  const nextFrame = (run: () => void) => {
    const id = requestAnimationFrame(() => {
      frames.delete(id);
      if (!disposed) run();
    });
    frames.add(id);
  };
  const nextTask = (run: () => void) => {
    const id = window.setTimeout(() => {
      timers.delete(id);
      if (!disposed) run();
    }, 0);
    timers.add(id);
  };

  // Place the clone immediately so it covers the preview from the first frame.
  // The snapshot rect was captured before the dialog opened.
  const canvasEl = document.querySelector(".preview-canvas-element") as HTMLCanvasElement | null;
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
    source = containRect(window.innerWidth, Math.max(1, window.innerHeight - HEADER_HEIGHT_PX), natW, natH);
    source.top += HEADER_HEIGHT_PX;
  }

  const clone = document.createElement("div");
  clone.className = "projects-restore-clone";
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

  // Make content visible now while the clone covers it, so the heavy
  // re-render happens before the animation instead of competing with it.
  onRevealContent();

  const abort = () => {
    clone.remove();
    onRevealContent();
    onShowCard();
  };

  const startAnimation = () => {
    const card = container.querySelector(`[data-project-id="${projectId}"]`) as HTMLElement | null;
    const thumbArea = card?.querySelector(".project-thumbnail") as HTMLElement | null;
    if (!card || !thumbArea) {
      abort();
      return;
    }
    card.scrollIntoView({ block: "nearest", behavior: "instant" as ScrollBehavior });

    const thumbRect = thumbArea.getBoundingClientRect();
    const dx = thumbRect.left - source.left;
    const dy = thumbRect.top - source.top;
    const sx = thumbRect.width / source.width;
    const sy = thumbRect.height / source.height;
    const flip = clone.animate(
      [
        { transform: "none", borderRadius: "0px" },
        { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, borderRadius: getProjectThumbnailRadius(sx, sy) },
      ],
      { duration: 520, easing: PROJECTS_FLIP_EASING, fill: "forwards" },
    );
    animations.push(flip);
    flip.onfinish = () => {
      onShowCard();
      nextFrame(() => {
        const settle = clone.animate([{ opacity: 1 }, { opacity: 0 }], {
          duration: 120,
          easing: PROJECTS_FLIP_SETTLE_EASING,
          fill: "forwards",
        });
        animations.push(settle);
        settle.onfinish = () => clone.remove();
      });
    };
  };

  // Wait for the content re-render and paint to finish, then start on a clean frame.
  const tryStart = () => nextFrame(() => nextTask(() => nextFrame(() => nextTask(startAnimation))));
  if (imgObj.complete) tryStart();
  else {
    imgObj.onload = tryStart;
    imgObj.onerror = abort;
  }

  return () => {
    disposed = true;
    frames.forEach((id) => cancelAnimationFrame(id));
    timers.forEach((id) => window.clearTimeout(id));
    imgObj.onload = null;
    imgObj.onerror = null;
    for (const animation of animations) {
      animation.onfinish = null;
      animation.cancel();
    }
    clone.remove();
  };
}
