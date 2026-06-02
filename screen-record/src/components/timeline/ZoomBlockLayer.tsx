import React from 'react';
import { createPortal } from 'react-dom';
import { ZoomBlock } from '@/types/video';
import { type AdjustableLineDragVisualMode } from './adjustableLineUtils';

interface ZoomBlockLayerProps {
  blocks: ZoomBlock[];
  duration: number;
  editingKeyframeId: number | null;
  hoveredBlockIdx: number | null;
  globalDragVisualMode: AdjustableLineDragVisualMode | null;
  trackWidth: number;
  trackRef: React.RefObject<HTMLDivElement | null>;
  onKeyframeClick: (time: number, index: number) => void;
  onKeyframeDragStart: (index: number) => void;
  onHoverBlock: (index: number | null) => void;
  startResizeBlock: (index: number, edge: 'start' | 'end', rect: DOMRect) => void;
  startResizeTransition: (index: number, side: 'in' | 'out', rect: DOMRect) => void;
}

export function ZoomBlockLayer({
  blocks,
  duration,
  editingKeyframeId,
  hoveredBlockIdx,
  globalDragVisualMode,
  trackWidth,
  trackRef,
  onKeyframeClick,
  onKeyframeDragStart,
  onHoverBlock,
  startResizeBlock,
  startResizeTransition,
}: ZoomBlockLayerProps) {
  return (
    <div className="zoom-blocks-layer absolute inset-0 z-40 pointer-events-none">
      {blocks.map((block, index) => {
        if (duration <= 0) return null;
        const active = editingKeyframeId === index;
        const disabled = block.enabled === false;
        const showHandles = active || hoveredBlockIdx === index;
        const leftPct = (block.startTime / duration) * 100;
        const span = Math.max(0, block.endTime - block.startTime);
        const widthPct = (span / duration) * 100;
        const fillOpacity = Math.min(0.5, 0.16 + (block.zoomFactor - 1) * 0.18);

        let easeIn = Math.max(0, block.easeIn);
        let easeOut = Math.max(0, block.easeOut);
        if (span > 0 && easeIn + easeOut > span) {
          const s = span / (easeIn + easeOut);
          easeIn *= s;
          easeOut *= s;
        }
        const holdStart = span > 0 ? (easeIn / span) * 100 : 0;
        const holdEnd = span > 0 ? 100 - (easeOut / span) * 100 : 100;
        const solidCenterPct = (holdStart + holdEnd) / 2;
        const solidCenterTime = block.startTime + easeIn + (span - easeIn - easeOut) / 2;
        const rampFill = disabled
          ? 'repeating-linear-gradient(45deg, rgba(59,130,246,0.10) 0px, rgba(59,130,246,0.10) 4px, transparent 4px, transparent 8px)'
          : `linear-gradient(90deg, rgba(59,130,246,${fillOpacity * 0.18}) 0%, rgba(59,130,246,${fillOpacity}) ${holdStart}%, rgba(59,130,246,${fillOpacity}) ${holdEnd}%, rgba(59,130,246,${fillOpacity * 0.18}) 100%)`;
        const blockPx = (widthPct / 100) * trackWidth;
        const solidPx = blockPx * ((holdEnd - holdStart) / 100);
        const badgeLabel = `${Math.round((block.zoomFactor - 1) * 100)}%`;
        const badgeFits = solidPx >= 30 + badgeLabel.length * 2;
        const showFloatBadge = !badgeFits && showHandles;

        return (
          <div
            key={block.id}
            className={`zoom-block absolute inset-y-0.5 rounded-md pointer-events-auto cursor-grab group/block ${
              active ? 'ring-1 ring-white/80 shadow-[0_0_8px_rgba(59,130,246,0.45)]' : ''
            } ${disabled ? 'opacity-40' : ''}`}
            data-active={active ? 'true' : 'false'}
            data-disabled={disabled ? 'true' : 'false'}
            style={{
              left: `${leftPct}%`,
              width: `${widthPct}%`,
              background: rampFill,
              border: '1px solid var(--timeline-zoom-color)',
            }}
            onMouseEnter={() => {
              if (globalDragVisualMode === null) onHoverBlock(index);
            }}
            onMouseLeave={() => onHoverBlock(null)}
            onClick={(e) => {
              e.stopPropagation();
              onKeyframeClick(solidCenterTime, index);
            }}
            onPointerDown={(e) => {
              e.stopPropagation();
              onKeyframeDragStart(index);
            }}
          >
            {badgeFits && (
              <div
                className="zoom-block-label timeline-chip absolute -translate-x-1/2 top-1/2 -translate-y-1/2 px-1.5 py-0.5 text-[9px] font-medium whitespace-nowrap pointer-events-none"
                style={{ left: `${solidCenterPct}%` }}
                data-tone="accent"
                data-active={active ? 'true' : 'false'}
              >
                {badgeLabel}
              </div>
            )}
            {showFloatBadge && (() => {
              const r = trackRef.current?.getBoundingClientRect();
              if (!r) return null;
              const x = r.left + (solidCenterTime / duration) * r.width;
              return createPortal(
                <div
                  className="zoom-block-label-float timeline-chip px-1.5 py-0.5 text-[9px] font-medium whitespace-nowrap shadow-md"
                  style={{
                    position: 'fixed',
                    left: x,
                    top: r.top - 4,
                    transform: 'translate(-50%, -100%)',
                    zIndex: 9999,
                    pointerEvents: 'none',
                  }}
                  data-tone="accent"
                  data-active={active ? 'true' : 'false'}
                >
                  {badgeLabel}
                </div>,
                document.body,
              );
            })()}

            <div
              className="zoom-block-handle-left absolute inset-y-0 left-0 w-2 -ml-1 cursor-col-resize z-10"
              onClick={(e) => e.stopPropagation()}
              onPointerDown={(e) => {
                e.stopPropagation();
                startResizeBlock(index, 'start', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
              }}
            />
            <div
              className="zoom-block-handle-right absolute inset-y-0 right-0 w-2 -mr-1 cursor-col-resize z-10"
              onClick={(e) => e.stopPropagation()}
              onPointerDown={(e) => {
                e.stopPropagation();
                startResizeBlock(index, 'end', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
              }}
            />

            {showHandles && (
              <>
                <div
                  className="zoom-block-transition-in absolute inset-y-0 w-3 -translate-x-1/2 cursor-ew-resize z-20"
                  style={{ left: `${holdStart}%` }}
                  title="Ease in"
                  onClick={(e) => e.stopPropagation()}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    startResizeTransition(index, 'in', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
                  }}
                >
                  <div className="absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-white/80" />
                </div>
                <div
                  className="zoom-block-transition-out absolute inset-y-0 w-3 -translate-x-1/2 cursor-ew-resize z-20"
                  style={{ left: `${holdEnd}%` }}
                  title="Ease out"
                  onClick={(e) => e.stopPropagation()}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    startResizeTransition(index, 'out', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
                  }}
                >
                  <div className="absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-white/80" />
                </div>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
