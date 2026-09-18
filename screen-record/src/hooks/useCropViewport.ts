import { useEffect, useState, type RefObject } from "react";

interface Bounds {
  left: number;
  top: number;
  width: number;
  height: number;
}

const FIT = { zoom: 1, x: 0, y: 0 };

export function useCropViewport(
  stageRef: RefObject<HTMLDivElement | null>,
  bounds: Bounds | null,
  show: boolean,
  videoSrc: string | null,
) {
  const [view, setView] = useState(FIT);

  useEffect(() => {
    setView(FIT);
  }, [show, videoSrc, bounds?.width, bounds?.height]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!show || !stage || !bounds) return;
    let dragging = false;
    const startDrag = () => { dragging = true; };
    const endDrag = () => { dragging = false; };
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      event.stopPropagation();
      // Keep the coordinate system fixed until an active crop drag finishes.
      if (dragging || !Number.isFinite(event.deltaY)) return;
      const stageRect = stage.getBoundingClientRect();
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? stageRect.height : 1;
      const delta = Math.max(-600, Math.min(600, event.deltaY * unit));
      const anchorX = event.clientX - stageRect.left - bounds.left;
      const anchorY = event.clientY - stageRect.top - bounds.top;
      setView((previous) => {
        const zoom = Math.max(1, Math.min(8, previous.zoom * Math.exp(-delta * 0.002)));
        if (zoom === 1) return FIT;
        const ratio = zoom / previous.zoom;
        return {
          zoom,
          x: Math.max(bounds.width * (1 - zoom), Math.min(0, anchorX - (anchorX - previous.x) * ratio)),
          y: Math.max(bounds.height * (1 - zoom), Math.min(0, anchorY - (anchorY - previous.y) * ratio)),
        };
      });
    };
    stage.addEventListener("wheel", wheel, { passive: false });
    stage.addEventListener("pointerdown", startDrag, true);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
    window.addEventListener("blur", endDrag);
    return () => {
      stage.removeEventListener("wheel", wheel);
      stage.removeEventListener("pointerdown", startDrag, true);
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", endDrag);
      window.removeEventListener("blur", endDrag);
    };
  }, [stageRef, bounds, show, videoSrc]);

  return bounds ? {
    left: bounds.left + view.x,
    top: bounds.top + view.y,
    width: bounds.width * view.zoom,
    height: bounds.height * view.zoom,
  } : null;
}
