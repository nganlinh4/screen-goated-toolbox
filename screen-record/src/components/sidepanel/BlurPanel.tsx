import { BackgroundConfig } from '@/types/video';
import { Slider } from '@/components/ui/Slider';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';

export interface BlurPanelProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function BlurPanel({ backgroundConfig, setBackgroundConfig, beginBatch, commitBatch }: BlurPanelProps) {
  const { t } = useSettings();
  return (
    <PanelCard className="blur-panel">
      <div className="blur-controls space-y-3.5">
        <div className="blur-sliders space-y-1.5">
          {([
            ['motionBlurCursor', t.motionBlurCursor, 25] as const,
            ['motionBlurZoom', t.motionBlurZoom, 10] as const,
            ['motionBlurPan', t.motionBlurPan, 10] as const,
          ]).map(([key, label, def]) => (
            <SettingRow key={key} label={label} valueDisplay={`${backgroundConfig[key] ?? def}%`} className={`motion-blur-slider-${key}`}>
              <Slider
                min={0} max={100} step={1}
                value={backgroundConfig[key] ?? def}
                onPointerDown={beginBatch} onPointerUp={commitBatch}
                onChange={(val) => setBackgroundConfig(prev => ({ ...prev, [key]: val }))}
                className="motion-blur-range"
              />
            </SettingRow>
          ))}
        </div>
      </div>
    </PanelCard>
  );
}
