import { Button } from '@/components/ui/button';
import { Slider } from '@/components/ui/Slider';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { Trash2 } from 'lucide-react';
import { VideoSegment } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';

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
      <PanelCard className="zoom-panel">
        <div className="panel-header flex justify-between items-center mb-3">
          <h2 className="panel-title text-xs font-medium uppercase tracking-wide text-on-surface-variant">{t.zoomConfiguration}</h2>
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
          <SettingRow label={t.zoomFactor} valueDisplay={`${zoomFactor.toFixed(1)}x`} className="zoom-factor-field">
            <Slider
              min={1} max={3} step={0.01} value={zoomFactor}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => { setZoomFactor(val); onUpdateZoom({ zoomFactor: val }); }}
            />
          </SettingRow>
          <SettingRow label={t.horizontalPosition} valueDisplay={`${Math.round((keyframe?.positionX ?? 0.5) * 100)}%`} className="position-x-field">
            <Slider
              min={0} max={1} step={0.01} value={keyframe?.positionX ?? 0.5}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => onUpdateZoom({ positionX: val })}
            />
          </SettingRow>
          <SettingRow label={t.verticalPosition} valueDisplay={`${Math.round((keyframe?.positionY ?? 0.5) * 100)}%`} className="position-y-field">
            <Slider
              min={0} max={1} step={0.01} value={keyframe?.positionY ?? 0.5}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => onUpdateZoom({ positionY: val })}
            />
          </SettingRow>
        </div>
      </PanelCard>
    );
  }

  return (
    <PanelCard className="zoom-panel-hint p-4">
      <p className="text-xs text-on-surface-variant">{t.zoomHint}</p>
    </PanelCard>
  );
}
