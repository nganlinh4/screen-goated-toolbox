import { Button } from '@/components/ui/button';
import { Trash2 } from 'lucide-react';
import { VideoSegment } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';

/** Inline style for slider active track fill */
const sv = (v: number, min: number, max: number): React.CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as React.CSSProperties);

export interface ZoomPanelProps {
  segment: VideoSegment | null;
  editingKeyframeId: number | null;
  zoomFactor: number;
  setZoomFactor: (value: number) => void;
  onDeleteKeyframe: () => void;
  onUpdateZoom: (updates: { zoomFactor?: number; positionX?: number; positionY?: number }) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function ZoomPanel({
  segment,
  editingKeyframeId,
  zoomFactor,
  setZoomFactor,
  onDeleteKeyframe,
  onUpdateZoom,
  beginBatch,
  commitBatch
}: ZoomPanelProps) {
  const { t } = useSettings();
  if (editingKeyframeId !== null && segment) {
    const keyframe = segment.zoomKeyframes[editingKeyframeId];
    if (!keyframe) return null;

    return (
      <div className="zoom-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
        <div className="panel-header flex justify-between items-center mb-3">
          <h2 className="panel-title text-xs font-medium uppercase tracking-wide text-[var(--on-surface-variant)]">{t.zoomConfiguration}</h2>
          <Button
            onClick={onDeleteKeyframe}
            variant="ghost"
            size="icon"
            className="text-[var(--on-surface-variant)] hover:text-[var(--tertiary-color)] hover:bg-[var(--tertiary-color)]/10 transition-colors"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
        <div className="zoom-controls space-y-3.5">
          <div className="zoom-factor-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.zoomFactor}</span>
            <input
              type="range"
              min="1"
              max="3"
              step="0.01"
              value={zoomFactor}
              style={sv(zoomFactor, 1, 3)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => {
                const newValue = Number(e.target.value);
                setZoomFactor(newValue);
                onUpdateZoom({ zoomFactor: newValue });
              }}
              className="flex-1 min-w-0"
            />
            <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{zoomFactor.toFixed(1)}x</span>
          </div>
          <div className="position-x-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.horizontalPosition}</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={keyframe?.positionX ?? 0.5}
              style={sv(keyframe?.positionX ?? 0.5, 0, 1)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => onUpdateZoom({ positionX: Number(e.target.value) })}
              className="flex-1 min-w-0"
            />
            <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{Math.round((keyframe?.positionX ?? 0.5) * 100)}%</span>
          </div>
          <div className="position-y-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{t.verticalPosition}</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={keyframe?.positionY ?? 0.5}
              style={sv(keyframe?.positionY ?? 0.5, 0, 1)}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(e) => onUpdateZoom({ positionY: Number(e.target.value) })}
              className="flex-1 min-w-0"
            />
            <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{Math.round((keyframe?.positionY ?? 0.5) * 100)}%</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="zoom-panel-hint bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-4 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <p className="text-xs text-[var(--on-surface-variant)]">{t.zoomHint}</p>
    </div>
  );
}
