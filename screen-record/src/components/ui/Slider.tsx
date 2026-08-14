import type { InputHTMLAttributes } from 'react';
import { useFieldLabelId } from '@/components/ui/FieldLabelContext';
import { cn } from '@/lib/utils';

export interface SliderProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'type' | 'min' | 'max' | 'step' | 'value' | 'onChange'
> {
  min: number;
  max: number;
  step?: number;
  value: number;
  onChange: (value: number) => void;
}

/**
 * Range slider that manages the `--value-pct` CSS variable used by App.css
 * to paint the active-track fill. Replaces the duplicated `sv()` helper.
 */
export function Slider({ min, max, step = 1, value, onChange, className, disabled = false, 'aria-labelledby': ariaLabelledBy, ...props }: SliderProps) {
  const labelId = useFieldLabelId(ariaLabelledBy);
  const pct = max === min ? 0 : ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      min={min}
      max={max}
      step={step}
      value={value}
      style={{ '--value-pct': `${pct}%` } as React.CSSProperties}
      onChange={(e) => onChange(Number(e.target.value))}
      aria-labelledby={labelId}
      disabled={disabled}
      className={cn('flex-1 min-w-0', className)}
      {...props}
    />
  );
}
