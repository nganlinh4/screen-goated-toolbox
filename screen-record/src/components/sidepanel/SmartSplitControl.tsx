import { Scissors } from '@/components/ui/MaterialIcon';
import { useState } from 'react';
import { Slider } from '@/components/ui/Slider';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';

interface SmartSplitControlProps {
  className?: string;
  disabled?: boolean;
  targetCount: number;
  onSplit: (maxUnits: number) => void;
}

export function SmartSplitControl({
  className,
  disabled = false,
  targetCount,
  onSplit,
}: SmartSplitControlProps) {
  const { t } = useSettings();
  const [maxUnits, setMaxUnits] = useState(8);

  return (
    <div className={`smart-split-control rounded-lg border border-outline/30 bg-surface-container-high/40 p-2 ${className ?? ''}`}>
      <div className="smart-split-header mb-2 flex items-center justify-between gap-2">
        <div className="smart-split-title flex items-center gap-1.5 text-[11px] font-semibold text-on-surface">
          <Scissors className="h-3.5 w-3.5 text-[var(--primary-color)]" />
          {t.smartSplitTitle}
        </div>
        <button
          type="button"
          className="smart-split-apply-button rounded-md border border-outline/30 px-2 py-1 text-[10px] font-semibold text-on-surface transition-colors hover:border-[var(--primary-color)] hover:bg-[color:color-mix(in_srgb,var(--primary-color)_12%,transparent)] disabled:cursor-not-allowed disabled:opacity-45"
          disabled={disabled || targetCount <= 0}
          onClick={() => onSplit(maxUnits)}
        >
          {t.smartSplitApply}
        </button>
      </div>
      <SettingRow
        label={t.smartSplitMaxWords}
        valueDisplay={`${maxUnits}`}
        className="smart-split-max-words-row"
      >
        <Slider
          min={3}
          max={24}
          step={1}
          value={maxUnits}
          onChange={setMaxUnits}
        />
      </SettingRow>
      <p className="smart-split-help mt-1 text-[10px] leading-4 text-on-surface-variant">
        {t.smartSplitHint.replace('{count}', String(targetCount))}
      </p>
    </div>
  );
}
