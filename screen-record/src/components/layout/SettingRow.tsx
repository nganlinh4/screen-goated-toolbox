import { cn } from '@/lib/utils';

export interface SettingRowProps {
  label: string;
  /** Formatted value shown in the right-hand readout. Omit to hide the readout column. */
  valueDisplay?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}

/**
 * Standard "label — control — readout" row used throughout the side panels.
 * Eliminates the duplicated flex layout and typography classes for every slider row.
 */
export function SettingRow({ label, valueDisplay, children, className }: SettingRowProps) {
  return (
    <div className={cn('setting-row flex items-center gap-3', className)}>
      <span className="setting-row-label text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">
        {label}
      </span>
      <div className="setting-row-control flex-1 min-w-0">
        {children}
      </div>
      {valueDisplay !== undefined && (
        <span className="setting-row-value text-[11px] font-medium text-on-surface tabular-nums w-12 text-right flex-shrink-0">
          {valueDisplay}
        </span>
      )}
    </div>
  );
}
