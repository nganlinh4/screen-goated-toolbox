import { cn } from '@/lib/utils';

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
}

/** Minimal toggle switch, styled via design tokens. */
export function Switch({ checked, onCheckedChange, disabled, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        'switch-root relative inline-flex h-5 w-9 items-center rounded-full transition-colors',
        disabled
          ? 'opacity-40 cursor-not-allowed bg-outline-variant'
          : checked
            ? 'bg-primary-color'
            : 'bg-outline-variant',
        className
      )}
    >
      <span
        className={cn(
          'switch-thumb inline-block h-4 w-4 rounded-full bg-white shadow transition-transform',
          checked ? 'translate-x-4' : 'translate-x-0.5'
        )}
      />
    </button>
  );
}
