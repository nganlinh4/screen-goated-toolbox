import { Link2, Plus, X } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import type { ProjectComposition, ProjectCompositionMode } from "@/types/video";
import { motion, AnimatePresence } from "framer-motion";
import { useRef, useCallback, useState } from "react";

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

  // --- Drag-to-scroll ---
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragState = useRef<{ startX: number; scrollLeft: number; dragging: boolean }>({ startX: 0, scrollLeft: 0, dragging: false });
  const [isDragging, setIsDragging] = useState(false);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    const el = scrollRef.current;
    if (!el) return;
    // Only initiate drag on the scrollable container itself or empty space, not on buttons
    if ((e.target as HTMLElement).closest("button")) return;
    dragState.current = { startX: e.clientX, scrollLeft: el.scrollLeft, dragging: true };
    setIsDragging(true);
    el.setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragState.current.dragging) return;
    const el = scrollRef.current;
    if (!el) return;
    const dx = e.clientX - dragState.current.startX;
    el.scrollLeft = dragState.current.scrollLeft - dx;
  }, []);

  const onPointerUp = useCallback(() => {
    dragState.current.dragging = false;
    setIsDragging(false);
  }, []);

  return (
    <div
      className={`sequence-focus-breadcrumb group/seqchain relative flex items-center gap-1 px-1 text-[11px] text-[var(--on-surface-variant)] ${isSingleClip ? "justify-center" : ""}`}
    >
      <div
        ref={scrollRef}
        className="sequence-pill-chain flex min-w-0 flex-1 items-center overflow-x-auto py-0.5"
        style={{ scrollbarWidth: "none", msOverflowStyle: "none", cursor: isDragging ? "grabbing" : "grab" }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div className="flex min-w-max items-center gap-1">
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
                <div className="relative">
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

      {isMultiClip && (
        <div className="sequence-mode-popover playback-keystroke-delay-popover absolute left-1/2 -translate-x-1/2 z-30 top-[calc(100%+4px)] w-max px-1.5 py-1 rounded-lg border pointer-events-none opacity-0 -translate-y-1 transition-all duration-150 group-hover/seqchain:opacity-100 group-hover/seqchain:translate-y-0 group-hover/seqchain:pointer-events-auto">
          <div className="flex items-center gap-0.5">
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
        </div>
      )}
    </div>
  );
}
