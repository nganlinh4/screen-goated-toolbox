import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check, Crop, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/Slider";
import { useSettings } from "@/hooks/useSettings";
import { formatTime } from "@/utils/helpers";
import { CropRect } from "@/types/video";
import { getContainedRect } from "@/lib/dynamicCapture";

const DEFAULT_CROP: CropRect = { x: 0, y: 0, width: 1, height: 1 };

interface CropWorkspaceProps {
  show: boolean;
  videoSrc: string | null;
  initialCrop?: CropRect;
  initialTime: number;
  onCancel: () => void;
  onApply: (crop: CropRect) => void;
}

export function CropWorkspace({
  show,
  videoSrc,
  initialCrop,
  initialTime,
  onCancel,
  onApply,
}: CropWorkspaceProps) {
  const { t } = useSettings();
  const stageRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const [draftCrop, setDraftCrop] = useState<CropRect>(initialCrop ?? DEFAULT_CROP);
  const [previewTime, setPreviewTime] = useState(initialTime);
  const [duration, setDuration] = useState(0);
  const [activeResizeHandle, setActiveResizeHandle] = useState<string | null>(null);
  const [videoBounds, setVideoBounds] = useState<{
    left: number;
    top: number;
    width: number;
    height: number;
  } | null>(null);

  useEffect(() => {
    if (!show) return;
    setDraftCrop(initialCrop ?? DEFAULT_CROP);
    setPreviewTime(initialTime);
  }, [show, initialCrop, initialTime]);

  useEffect(() => {
    if (!show) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onCancel();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [show, onCancel]);

  useLayoutEffect(() => {
    if (!show) return;
    const stage = stageRef.current;
    const video = videoRef.current;
    if (!stage || !video) {
      setVideoBounds(null);
      return;
    }

    let rafId: number | null = null;
    const updateBounds = () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        const nextStage = stageRef.current;
        const nextVideo = videoRef.current;
        if (!nextStage || !nextVideo) {
          setVideoBounds(null);
          return;
        }
        const stageRect = nextStage.getBoundingClientRect();
        const intrinsicWidth = nextVideo.videoWidth;
        const intrinsicHeight = nextVideo.videoHeight;
        if (
          stageRect.width <= 0 ||
          stageRect.height <= 0 ||
          intrinsicWidth <= 0 ||
          intrinsicHeight <= 0
        ) {
          setVideoBounds(null);
          return;
        }
        const contained = getContainedRect(
          stageRect.width,
          stageRect.height,
          intrinsicWidth,
          intrinsicHeight,
        );
        const nextBounds = {
          left: contained.left,
          top: contained.top,
          width: contained.width,
          height: contained.height,
        };
        setVideoBounds((prev) => {
          if (
            prev &&
            Math.abs(prev.left - nextBounds.left) < 0.5 &&
            Math.abs(prev.top - nextBounds.top) < 0.5 &&
            Math.abs(prev.width - nextBounds.width) < 0.5 &&
            Math.abs(prev.height - nextBounds.height) < 0.5
          ) {
            return prev;
          }
          return nextBounds;
        });
      });
    };

    updateBounds();

    const resizeObserver = new ResizeObserver(updateBounds);
    resizeObserver.observe(stage);
    resizeObserver.observe(video);
    window.addEventListener("resize", updateBounds);
    window.addEventListener("scroll", updateBounds, true);

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      window.removeEventListener("resize", updateBounds);
      window.removeEventListener("scroll", updateBounds, true);
    };
  }, [show, videoSrc]);

  useEffect(() => {
    if (!show) return;
    const video = videoRef.current;
    if (!video) return;

    const syncToInitialFrame = () => {
      const nextDuration =
        Number.isFinite(video.duration) && video.duration > 0 ? video.duration : 0;
      setDuration(nextDuration);
      const nextTime = Math.max(0, Math.min(nextDuration || initialTime, initialTime));
      setPreviewTime(nextTime);
      if (Math.abs(video.currentTime - nextTime) > 0.01) {
        video.currentTime = nextTime;
      }
    };

    const handleLoadedMetadata = () => syncToInitialFrame();
    const handleLoadedData = () => syncToInitialFrame();

    video.addEventListener("loadedmetadata", handleLoadedMetadata);
    video.addEventListener("loadeddata", handleLoadedData);

    if (video.readyState >= 1) {
      syncToInitialFrame();
    }

    return () => {
      video.removeEventListener("loadedmetadata", handleLoadedMetadata);
      video.removeEventListener("loadeddata", handleLoadedData);
    };
  }, [show, videoSrc, initialTime]);

  useEffect(() => {
    if (!show) return;
    const video = videoRef.current;
    if (!video) return;
    const maxTime =
      Number.isFinite(video.duration) && video.duration > 0 ? video.duration : duration;
    const clamped = Math.max(0, Math.min(maxTime || previewTime, previewTime));
    if (Math.abs(video.currentTime - clamped) > 0.01) {
      video.currentTime = clamped;
    }
  }, [show, previewTime, duration]);

  if (!show || !videoSrc) return null;

  const bounds = videoBounds;

  const handleResizeStart = (event: React.MouseEvent, type: string) => {
    if (!bounds) return;
    event.preventDefault();
    event.stopPropagation();
    setActiveResizeHandle(type);
    const startX = event.clientX;
    const startY = event.clientY;
    const startCrop = { ...draftCrop };

    const handleMove = (moveEvent: MouseEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      const dXPct = dx / bounds.width;
      const dYPct = dy / bounds.height;

      let newX = startCrop.x;
      let newY = startCrop.y;
      let newW = startCrop.width;
      let newH = startCrop.height;

      if (type.includes("n")) {
        let desiredY = startCrop.y + dYPct;
        const maxY = startCrop.y + startCrop.height - 0.05;
        desiredY = Math.max(0, Math.min(maxY, desiredY));
        const deltaY = desiredY - startCrop.y;
        newY = desiredY;
        newH = startCrop.height - deltaY;
      } else if (type.includes("s")) {
        let desiredH = startCrop.height + dYPct;
        newH = Math.max(0.05, Math.min(1 - startCrop.y, desiredH));
      }

      if (type.includes("w")) {
        let desiredX = startCrop.x + dXPct;
        const maxX = startCrop.x + startCrop.width - 0.05;
        desiredX = Math.max(0, Math.min(maxX, desiredX));
        const deltaX = desiredX - startCrop.x;
        newX = desiredX;
        newW = startCrop.width - deltaX;
      } else if (type.includes("e")) {
        let desiredW = startCrop.width + dXPct;
        newW = Math.max(0.05, Math.min(1 - startCrop.x, desiredW));
      }

      setDraftCrop({ x: newX, y: newY, width: newW, height: newH });
    };

    const handleUp = () => {
      setActiveResizeHandle(null);
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
  };

  const handleBoxMove = (event: React.MouseEvent) => {
    if (!bounds) return;
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startY = event.clientY;
    const startCrop = { ...draftCrop };

    const handleMove = (moveEvent: MouseEvent) => {
      const dx = (moveEvent.clientX - startX) / bounds.width;
      const dy = (moveEvent.clientY - startY) / bounds.height;
      const newX = Math.max(0, Math.min(1 - startCrop.width, startCrop.x + dx));
      const newY = Math.max(0, Math.min(1 - startCrop.height, startCrop.y + dy));
      setDraftCrop({ x: newX, y: newY, width: startCrop.width, height: startCrop.height });
    };

    const handleUp = () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
  };

  const handles = [
    { key: "nw", cursor: "cursor-nw-resize", position: "-top-1.5 -left-1.5" },
    { key: "n", cursor: "cursor-n-resize", position: "-top-1.5 left-1/2 -translate-x-1/2" },
    { key: "ne", cursor: "cursor-ne-resize", position: "-top-1.5 -right-1.5" },
    { key: "w", cursor: "cursor-w-resize", position: "top-1/2 -translate-y-1/2 -left-1.5" },
    { key: "e", cursor: "cursor-e-resize", position: "top-1/2 -translate-y-1/2 -right-1.5" },
    { key: "sw", cursor: "cursor-sw-resize", position: "-bottom-1.5 -left-1.5" },
    { key: "s", cursor: "cursor-s-resize", position: "-bottom-1.5 left-1/2 -translate-x-1/2" },
    { key: "se", cursor: "cursor-se-resize", position: "-bottom-1.5 -right-1.5" },
  ];

  const clampedDuration = duration > 0 ? duration : 0;

  return (
    <div className="crop-workspace absolute inset-0 z-[140] bg-[color-mix(in_srgb,var(--surface-dim)_90%,black)]">
      <div className="crop-workspace-shell flex h-full flex-col gap-4 px-5 py-4">
        <div className="crop-workspace-topbar ui-surface-elevated flex items-center justify-between rounded-[1.35rem] px-4 py-3">
          <div className="crop-workspace-title-row flex items-center gap-3">
            <div className="crop-workspace-title-icon flex h-10 w-10 items-center justify-center rounded-2xl bg-[color-mix(in_srgb,var(--primary-color)_16%,var(--ui-surface-3))] text-[var(--primary-color)]">
              <Crop className="h-4 w-4" />
            </div>
            <div className="crop-workspace-title-group">
              <div className="crop-workspace-title text-sm font-semibold text-[var(--on-surface)]">
                {t.cropVideo}
              </div>
              <div className="crop-workspace-time text-xs text-[var(--on-surface-variant)]">
                {formatTime(previewTime)} / {formatTime(clampedDuration)}
              </div>
            </div>
          </div>
          <div className="crop-workspace-actions flex items-center gap-2">
            <Button
              variant="outline"
              onClick={onCancel}
              className="crop-workspace-cancel-btn h-9 rounded-xl text-xs"
            >
              <X className="h-3.5 w-3.5" />
              {t.cancel}
            </Button>
            <Button
              onClick={() => onApply(draftCrop)}
              className="crop-workspace-apply-btn ui-action-button h-9 rounded-xl text-xs"
              data-tone="success"
              data-active="true"
              data-emphasis="strong"
            >
              <Check className="h-3.5 w-3.5" />
              {t.applyCrop}
            </Button>
          </div>
        </div>

        <div className="crop-workspace-stage-shell ui-surface flex min-h-0 flex-1 rounded-[1.75rem] p-4">
          <div
            ref={stageRef}
            className="crop-workspace-stage relative flex h-full w-full items-center justify-center rounded-[1.35rem]"
          >
            <div className="crop-workspace-stage-backdrop absolute inset-0 rounded-[1.35rem]" />

            {bounds && (
              <div
                className="crop-workspace-video-frame absolute z-[5] rounded-[1.2rem]"
                style={{
                  left: bounds.left,
                  top: bounds.top,
                  width: bounds.width,
                  height: bounds.height,
                }}
              />
            )}

            <video
              ref={videoRef}
              src={videoSrc}
              className="crop-workspace-video pointer-events-none absolute z-10 rounded-[1.15rem] object-contain shadow-[var(--shadow-elevation-3)]"
              style={
                bounds
                  ? {
                      left: bounds.left,
                      top: bounds.top,
                      width: bounds.width,
                      height: bounds.height,
                    }
                  : undefined
              }
              crossOrigin="anonymous"
              playsInline
              preload="auto"
              muted
            />

            {bounds && (
              <div className="crop-workspace-overlay absolute inset-0 z-20 pointer-events-none">
                <div className="crop-workspace-mask-layer absolute inset-0 overflow-hidden rounded-[1.35rem]">
                  <div
                    className="crop-workspace-video-bounds absolute"
                    style={{
                      left: bounds.left,
                      top: bounds.top,
                      width: bounds.width,
                      height: bounds.height,
                    }}
                  >
                    <div
                      className="crop-workspace-selection-mask absolute"
                      style={{
                        left: `${draftCrop.x * 100}%`,
                        top: `${draftCrop.y * 100}%`,
                        width: `${draftCrop.width * 100}%`,
                        height: `${draftCrop.height * 100}%`,
                      }}
                    />
                  </div>
                </div>

                <div
                  className="crop-workspace-video-bounds absolute"
                  style={{
                    left: bounds.left,
                    top: bounds.top,
                    width: bounds.width,
                    height: bounds.height,
                  }}
                >
                  <div
                    className="crop-workspace-selection absolute border-2 border-[var(--primary-color)] bg-[var(--primary-color)]/10 pointer-events-auto"
                    data-resizing={activeResizeHandle ? "true" : "false"}
                    style={{
                      left: `${draftCrop.x * 100}%`,
                      top: `${draftCrop.y * 100}%`,
                      width: `${draftCrop.width * 100}%`,
                      height: `${draftCrop.height * 100}%`,
                    }}
                    onMouseDown={handleBoxMove}
                  >
                    <div className="crop-workspace-grid-rows pointer-events-none absolute inset-0 flex flex-col opacity-30">
                      <div className="flex-1 border-b border-white/50" />
                      <div className="flex-1 border-b border-white/50" />
                      <div className="flex-1" />
                    </div>
                    <div className="crop-workspace-grid-cols pointer-events-none absolute inset-0 flex opacity-30">
                      <div className="flex-1 border-r border-white/50" />
                      <div className="flex-1 border-r border-white/50" />
                      <div className="flex-1" />
                    </div>

                    {handles.map((handle) => (
                      <div
                        key={handle.key}
                        className={`crop-workspace-handle absolute z-30 h-3 w-3 rounded-full border border-[var(--primary-color)] bg-white transition-transform hover:scale-125 ${handle.cursor} ${handle.position}`}
                        data-active={activeResizeHandle === handle.key ? "true" : "false"}
                        onMouseDown={(event) => handleResizeStart(event, handle.key)}
                      />
                    ))}

                    <div className="crop-workspace-crosshair pointer-events-none absolute left-1/2 top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 opacity-50">
                      <div className="absolute top-1/2 h-px w-full -translate-y-1/2 bg-white shadow-sm" />
                      <div className="absolute left-1/2 h-full w-px -translate-x-1/2 bg-white shadow-sm" />
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="crop-workspace-footer ui-surface-elevated flex items-center gap-4 rounded-[1.35rem] px-4 py-3">
          <div className="crop-workspace-time-start w-14 text-xs tabular-nums text-[var(--on-surface-variant)]">
            {formatTime(previewTime)}
          </div>
          <Slider
            min={0}
            max={Math.max(clampedDuration, 0.01)}
            step={0.01}
            value={Math.min(previewTime, Math.max(clampedDuration, 0.01))}
            onChange={setPreviewTime}
            className="crop-workspace-timeline-slider"
          />
          <div className="crop-workspace-time-end w-14 text-right text-xs tabular-nums text-[var(--on-surface-variant)]">
            {formatTime(clampedDuration)}
          </div>
        </div>
      </div>
    </div>
  );
}
