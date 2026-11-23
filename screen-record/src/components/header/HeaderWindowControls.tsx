import { Copy, Minus, Square, X } from '@/components/ui/MaterialIcon';
import type { Translations } from "@/i18n";

interface HeaderWindowControlsProps {
  isWindowMaximized: boolean;
  /** Re-reads the host maximized state once the toggle has settled. */
  onMaximizeToggled: () => void;
  t: Translations;
}

export function HeaderWindowControls({
  isWindowMaximized,
  onMaximizeToggled,
  t,
}: HeaderWindowControlsProps) {
  return (
    <div className={`window-controls flex items-center h-full ${isWindowMaximized ? "" : "ml-4"}`}>
      <button
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          (window as any).ipc.postMessage("minimize_window");
        }}
        className="window-btn-minimize ui-icon-button px-3 h-full text-[var(--on-surface)] flex items-center rounded-none"
        title={t.minimize}
      >
        <Minus className="w-4 h-4" />
      </button>
      <button
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          (window as any).ipc.postMessage("toggle_maximize");
          onMaximizeToggled();
        }}
        className="window-btn-maximize ui-icon-button px-3 h-full text-[var(--on-surface)] flex items-center rounded-none"
        title={isWindowMaximized ? t.restore : t.maximize}
      >
        {isWindowMaximized ? (
          <Copy className="w-3.5 h-3.5" />
        ) : (
          <Square className="w-3.5 h-3.5" />
        )}
      </button>
      <button
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          (window as any).ipc.postMessage("close_window");
        }}
        className={`window-btn-close px-3 h-full text-[var(--on-surface)] hover:bg-[var(--tertiary-color)] hover:text-white transition-colors flex items-center ${isWindowMaximized ? "pr-5" : ""}`}
        title={t.close}
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
}
