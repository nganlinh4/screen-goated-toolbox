import { cn } from '@/lib/utils';
import { motion } from 'framer-motion';

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
}

/** Toggle switch with spring-animated thumb. */
export function Switch({ checked, onCheckedChange, disabled, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        'switch-root relative inline-flex h-5 w-9 items-center rounded-full transition-colors duration-200',
        disabled
          ? 'opacity-40 cursor-not-allowed bg-outline-variant'
          : checked
            ? 'bg-primary-color shadow-[0_0_8px_rgba(59,130,246,0.25)]'
            : 'bg-outline-variant',
        className
      )}
    >
      <motion.span
        className="switch-thumb inline-block h-4 w-4 rounded-full bg-white shadow-elevation-1"
        animate={{ x: checked ? 18 : 2 }}
        transition={{ type: 'spring', stiffness: 500, damping: 30 }}
      />
    </button>
  );
}
