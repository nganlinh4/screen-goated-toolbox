import { cn } from '@/lib/utils';

export interface PanelCardProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * Glassmorphism card used as the outer wrapper for every side panel.
 * Centralises the glass-bg / glass-border / backdrop-blur pattern.
 */
export function PanelCard({ children, className }: PanelCardProps) {
  return (
    <div
      className={cn(
        'panel-card bg-glass-bg backdrop-blur-xl rounded-xl border border-glass-border p-3 shadow-[0_2px_8px_rgba(0,0,0,0.2)]',
        className
      )}
    >
      {children}
    </div>
  );
}
