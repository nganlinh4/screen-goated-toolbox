import { Link2, Plus, X, Check, Minus } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import type { ProjectComposition, ProjectCompositionClip, ProjectCompositionMode } from "@/types/video";
import { motion, AnimatePresence } from "framer-motion";
import { useRef, useCallback, useState, useEffect } from "react";

interface SequencePillChainProps {
  composition: ProjectComposition;
  activeClipId: string | null;
  spreadFromClipId: string | null;
  onSelectClip: (clipId: string) => void;
  onInsertClip: (clipId: string | null, placement: "before" | "after") => void;
  onRemoveClip: (clipId: string) => void;
  onModeChange: (mode: ProjectCompositionMode) => void;
}

export function SequencePillChain({
  composition,
  activeClipId,
  spreadFromClipId,
  onSelectClip,
  onInsertClip,
  onRemoveClip,
  onModeChange,
}: SequencePillChainProps) {
  const { t } = useSettings();
  const isMultiClip = composition.clips.length > 1;
  const isSingleClip = !isMultiClip;
  const spreadSourceIndex = composition.clips.findIndex(
    (clip) => clip.id === spreadFromClipId,
  );

  // --- Mode popover hover with leave delay ---
  const [modePopoverVisible, setModePopoverVisible] = useState(false);
  const [hoveredClipId, setHoveredClipId] = useState<string | null>(null);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showPopover = useCallback(() => {
    if (leaveTimerRef.current) { clearTimeout(leaveTimerRef.current); leaveTimerRef.current = null; }
    setModePopoverVisible(true);
  }, []);

  const scheduleHidePopover = useCallback(() => {
    if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current);
    leaveTimerRef.current = setTimeout(() => setModePopoverVisible(false), 250);
  }, []);

  // --- Measure inner content to report intrinsic width to parent slot ---
  const contentRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const content = contentRef.current;
    const root = rootRef.current;
    if (!content || !root) return;
    const ro = new ResizeObserver(() => {
      const w = content.scrollWidth;
      root.style.setProperty('--breadcrumb-content-w', `${w}px`);
    });
    ro.observe(content);
    return () => ro.disconnect();
  }, []);

  // --- Drag-to-scroll (window listeners, like cursor grid) ---
  const scrollRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const suppressClickRef = useRef(false);
  const DRAG_THRESHOLD = 4;

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    const el = scrollRef.current;
    if (!el) return;

    const startX = e.clientX;
    const startScrollLeft = el.scrollLeft;
    let dragging = false;

    const handlePointerMove = (me: PointerEvent) => {
      const dx = me.clientX - startX;
      if (!dragging && Math.abs(dx) > DRAG_THRESHOLD) {
        dragging = true;
        suppressClickRef.current = true;
        setIsDragging(true);
      }
      if (dragging) {
        el.scrollLeft = startScrollLeft - dx;
        me.preventDefault();
      }
    };

    const handlePointerEnd = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerEnd);
      window.removeEventListener("pointercancel", handlePointerEnd);
      if (dragging) {
        setIsDragging(false);
        requestAnimationFrame(() => { suppressClickRef.current = false; });
      }
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerEnd);
    window.addEventListener("pointercancel", handlePointerEnd);
  }, []);

  const handleClickCapture = useCallback((e: React.MouseEvent) => {
    if (suppressClickRef.current) {
      e.stopPropagation();
      e.preventDefault();
    }
  }, []);

  return (
    <div
      ref={rootRef}
      className={`sequence-focus-breadcrumb relative flex items-center gap-1 px-1 min-w-0 w-full text-[11px] text-[var(--on-surface-variant)] ${isSingleClip ? "justify-center" : ""}`}
      onMouseEnter={showPopover}
      onMouseLeave={scheduleHidePopover}
    >
      <div
        ref={scrollRef}
        className={`sequence-pill-chain flex min-w-0 flex-1 items-center overflow-x-scroll overflow-y-clip py-0.5 ${isDragging ? "!cursor-grabbing select-none" : "!cursor-grab"}`}
        style={{ scrollbarWidth: "none", msOverflowStyle: "none", touchAction: "none" }}
        onPointerDown={handlePointerDown}
        onClickCapture={handleClickCapture}
      >
        <div ref={contentRef} className="flex min-w-max items-center gap-1">
          <button
            type="button"
            onClick={() => onInsertClip(null, "before")}
            className="sequence-pill-add-btn ui-chip-button flex h-6 w-6 items-center justify-center rounded-full text-[var(--primary-color)]"
            title={t.sequenceInsertAtStart}
            aria-label={t.sequenceInsertAtStart}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>

          <AnimatePresence mode="popLayout">
          {composition.clips.map((clip, index) => {
            const isRoot = clip.role === "root";
            const isSelected = composition.selectedClipId === clip.id;
            const isPlaying = activeClipId === clip.id;
            const isUnifiedSource =
              isMultiClip &&
              composition.mode === "unified" &&
              composition.unifiedSourceClipId === clip.id;
            const shouldAnimateSpread =
              isMultiClip &&
              composition.mode === "unified" &&
              spreadSourceIndex >= 0 &&
              spreadFromClipId;
            const spreadDelayMs =
              spreadSourceIndex >= 0
                ? Math.abs(spreadSourceIndex - index) * 70
                : 0;
            const pillTitleParts = [clip.name];

            if (isRoot) {
              pillTitleParts.push(t.sequenceOriginalTitle);
            }

            if (isUnifiedSource) {
              pillTitleParts.push(t.sequenceSharedLookSourceTitle);
            }

            return (
              <motion.div
                key={clip.id}
                layout
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                transition={{ type: 'spring', stiffness: 400, damping: 25 }}
                className="flex items-center gap-1"
              >
                <div
                  className="relative"
                  onMouseEnter={() => { setHoveredClipId(clip.id); showPopover(); }}
                  onMouseLeave={() => setHoveredClipId(null)}
                >
                  <button
                    type="button"
                    onClick={() => onSelectClip(clip.id)}
                    className={`sequence-pill ui-chip-button group relative flex min-w-[7.5rem] max-w-[10rem] items-center gap-1.5 rounded-full px-2.5 py-1 text-left ${
                      isSelected
                        ? "ui-chip-button-active"
                        : "text-[var(--on-surface)]"
                    } ${isUnifiedSource ? "ring-2 ring-[var(--primary-color)]/35 ring-offset-1 ring-offset-transparent" : ""} ${isPlaying ? "shadow-[0_0_0_2px_rgba(255,255,255,0.14),0_0_0_4px_rgba(59,130,246,0.24)]" : ""} ${shouldAnimateSpread ? "animate-pulse" : ""}`}
                    style={
                      shouldAnimateSpread
                        ? {
                            animationDelay: `${spreadDelayMs}ms`,
                            animationDuration: "680ms",
                          }
                        : undefined
                    }
                    title={pillTitleParts.join(" • ")}
                  >
                    <span className="truncate font-medium">{clip.name}</span>
                    {isRoot && (
                      <span
                        className={`sequence-pill-root-badge shrink-0 whitespace-nowrap rounded-full px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em] ${
                          isSelected
                            ? "bg-white/18 text-[var(--on-surface)] dark:text-white"
                            : "bg-[var(--surface)] text-[var(--on-surface-variant)]"
                        }`}
                      >
                        {t.original}
                      </span>
                    )}
                    {isUnifiedSource && (
                      <span className="rounded-full bg-white/18 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em]">
                        {t.sequenceSource}
                      </span>
                    )}
                  </button>
                  {clip.role !== "root" && (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onRemoveClip(clip.id);
                      }}
                      className={`sequence-pill-remove ui-chip-button absolute -right-1 -top-1 flex h-[18px] w-[18px] items-center justify-center rounded-full ${
                        isSelected
                          ? "border-white/30 bg-black/35 text-white hover:bg-red-500"
                          : "text-[var(--on-surface-variant)] hover:bg-red-500 hover:text-white"
                      }`}
                      title={t.sequenceRemoveClip}
                      aria-label={t.sequenceRemoveClip}
                    >
                      <X className="h-2.5 w-2.5" />
                    </button>
                  )}
                </div>

                {isMultiClip && index < composition.clips.length - 1 && (
                  <button
                    type="button"
                    onClick={() => onInsertClip(clip.id, "after")}
                    className="sequence-pill-gap ui-icon-button relative flex h-6 w-6 items-center justify-center rounded-full border border-transparent text-[var(--on-surface-variant)] hover:border-[var(--ui-border)] hover:bg-[var(--ui-surface-2)] hover:text-[var(--primary-color)]"
                    title={t.sequenceInsertHere}
                    aria-label={t.sequenceInsertHere}
                  >
                    <Link2 className="h-3.5 w-3.5" />
                    <span className="absolute -right-0.5 -top-0.5 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-[var(--primary-color)] text-white">
                      <Plus className="h-2.5 w-2.5" />
                    </span>
                  </button>
                )}
              </motion.div>
            );
          })}
          </AnimatePresence>

          <button
            type="button"
            onClick={() => onInsertClip(null, "after")}
            className="sequence-pill-add-btn ui-chip-button flex h-6 w-6 items-center justify-center rounded-full text-[var(--primary-color)]"
            title={t.sequenceInsertAtEnd}
            aria-label={t.sequenceInsertAtEnd}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {(
        <div
          className={`sequence-mode-popover playback-keystroke-delay-popover absolute left-1/2 -translate-x-1/2 z-30 top-[calc(100%+4px)] w-max px-2.5 py-2 rounded-lg border transition-all duration-150 ${
            modePopoverVisible
              ? "opacity-100 translate-y-0 pointer-events-auto"
              : "opacity-0 -translate-y-1 pointer-events-none"
          }`}
          onMouseEnter={showPopover}
          onMouseLeave={scheduleHidePopover}
          style={{ paddingTop: "10px", marginTop: "-4px" }}
        >
          {isMultiClip && <>
          <span className="sequence-mode-label text-[9px] font-semibold uppercase tracking-wider text-[var(--on-surface-variant)]/60">{t.sequenceApplyConfig}</span>
          <div className="flex items-center gap-0.5 mt-1">
            {(["separate", "unified"] as const).map((mode) => {
              const isActive = composition.mode === mode;
              return (
                <button
                  key={mode}
                  type="button"
                  onClick={() => onModeChange(mode)}
                  aria-pressed={isActive}
                  className={`sequence-mode-toggle-btn sequence-mode-toggle-btn-${mode} rounded-md px-2.5 py-1 text-[10px] font-semibold whitespace-nowrap transition-colors ${
                    isActive
                      ? "bg-[var(--primary-color)]/15 text-[var(--primary-color)]"
                      : "text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-2)] hover:text-[var(--on-surface)]"
                  }`}
                  title={
                    mode === "separate"
                      ? t.sequenceModePerClip
                      : t.sequenceModeSharedLook
                  }
                >
                  {mode === "separate"
                    ? t.sequenceModePerClip
                    : t.sequenceModeSharedLook}
                </button>
              );
            })}
          </div>
          </>}
          <ClipMetadataList clip={(hoveredClipId ? composition.clips.find(c => c.id === hoveredClipId) : isSingleClip ? composition.clips[0] : undefined)} t={t} />
        </div>
      )}
    </div>
  );
}

function ClipMetadataList({ clip, t }: { clip?: ProjectCompositionClip; t: Record<string, string> }) {
  if (!clip) return null;

  const hasVideo = !!(clip.rawVideoPath);
  const hasCursor = clip.mousePositions.length > 0;
  const hasWebcam = clip.segment.webcamAvailable === true;
  const hasMic = clip.segment.micAudioAvailable === true;
  const hasDeviceAudio = clip.segment.deviceAudioAvailable !== false;
  const hasKeystrokes = (clip.segment.keystrokeEvents?.length ?? 0) > 0;

  const items: { label: string; available: boolean }[] = [
    { label: t.clipInfoVideo, available: hasVideo },
    { label: t.clipInfoCursor, available: hasCursor },
    { label: t.clipInfoWebcam, available: hasWebcam },
    { label: t.clipInfoMic, available: hasMic },
    { label: t.clipInfoDeviceAudio, available: hasDeviceAudio },
    { label: t.clipInfoKeystrokes, available: hasKeystrokes },
  ];

  return (
    <div className="clip-metadata-list mt-2 pt-2 border-t border-[var(--ui-border)]">
      <p className="text-[9px] font-semibold uppercase tracking-wider text-[var(--on-surface-variant)]/60 mb-1 truncate max-w-[180px]">{clip.name}</p>
      <div className="flex flex-col gap-1">
        {items.map((item) => (
          <div key={item.label} className="flex items-center gap-2">
            <span className={`shrink-0 flex items-center justify-center w-3.5 h-3.5 rounded-full ${
              item.available
                ? 'bg-green-500/20'
                : 'bg-[var(--on-surface-variant)]/8'
            }`}>
              {item.available ? (
                <Check className="w-2 h-2 text-green-500" strokeWidth={3} />
              ) : (
                <Minus className="w-2 h-2 text-[var(--on-surface-variant)]/30" strokeWidth={3} />
              )}
            </span>
            <span className={`text-[10px] ${item.available ? 'text-[var(--on-surface)]' : 'text-[var(--on-surface-variant)]/40 line-through'}`}>{item.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
