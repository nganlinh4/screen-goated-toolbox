import { cn } from '@/lib/utils';

export interface PanelCardProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * Material-style card used as the outer wrapper for every side panel.
 * Centralizes the shared elevated surface treatment.
 */
export function PanelCard({ children, className }: PanelCardProps) {
  return (
    <div
      className={cn(
        'panel-card material-surface relative rounded-xl p-3',
        className
      )}
    >
      {children}
    </div>
  );
}
