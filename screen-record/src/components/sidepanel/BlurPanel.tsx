import { BackgroundConfig } from '@/types/video';
import { useSettings } from '@/hooks/useSettings';

/** Inline style for slider active track fill */
const sv = (v: number, min: number, max: number): React.CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as React.CSSProperties);

export interface BlurPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function BlurPanel({ backgroundConfig, setBackgroundConfig, beginBatch, commitBatch }: BlurPanelProps) {
  const { t } = useSettings();
  return (
    <div className="blur-panel bg-[var(--glass-bg)] backdrop-blur-xl rounded-xl border border-[var(--glass-border)] p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]">
      <div className="blur-controls space-y-3.5">
        <div className="blur-sliders space-y-1.5">
          {([
            ['motionBlurCursor', t.motionBlurCursor, 25] as const,
            ['motionBlurZoom', t.motionBlurZoom, 10] as const,
            ['motionBlurPan', t.motionBlurPan, 10] as const,
          ]).map(([key, label, def]) => (
            <div key={key} className={`motion-blur-slider-${key} flex items-center gap-3`}>
              <span className="text-[11px] font-medium text-[var(--on-surface-variant)] w-20 flex-shrink-0">{label}</span>
              <input
                type="range" min="0" max="100" step="1"
                value={backgroundConfig[key] ?? def}
                style={sv(backgroundConfig[key] ?? def, 0, 100)}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(e) => setBackgroundConfig(prev => ({ ...prev, [key]: Number(e.target.value) }))}
                className="motion-blur-range flex-1 min-w-0"
              />
              <span className="text-[11px] font-medium text-[var(--on-surface)] tabular-nums w-12 text-right flex-shrink-0">{backgroundConfig[key] ?? def}%</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
