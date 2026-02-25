import { X, Lock } from 'lucide-react';
import { WindowInfo } from '@/hooks/useAppHooks';
import { useSettings } from '@/hooks/useSettings';

interface WindowSelectDialogProps {
  show: boolean;
  onClose: () => void;
  windows: WindowInfo[];
  onSelectWindow: (windowId: string, captureMethod: 'game' | 'window') => void;
}

export function WindowSelectDialog({
  show,
  onClose,
  windows,
  onSelectWindow
}: WindowSelectDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="window-select-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100] px-6 py-6">
      <div className="window-select-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg w-full max-w-5xl h-full max-h-[80vh] flex flex-col">
        <div className="window-select-header flex items-center justify-between mb-4 flex-shrink-0">
          <h3 className="window-select-title text-sm font-medium text-[var(--on-surface)]">
            {t.selectWindow}
          </h3>
          <button
            onClick={onClose}
            className="window-select-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="window-select-grid overflow-y-auto thin-scrollbar grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4 p-1">
          {windows.map((win) => {
            const initial = win.processName.charAt(0).toUpperCase() || 'W';
            return (
              <div
                key={win.id}
                onClick={() => {
                  if (win.isAdmin) return;
                  onClose();
                  onSelectWindow(win.id, 'window');
                }}
                className={`window-select-card relative group border border-[var(--glass-border)] rounded-xl overflow-hidden flex flex-col h-36 shadow-sm ${
                  win.isAdmin
                    ? 'bg-[var(--surface-dim)] opacity-90 cursor-not-allowed'
                    : 'bg-[var(--surface-container)] hover:border-[var(--primary-color)] transition-colors cursor-pointer'
                }`}
              >
                {win.isAdmin && (
                  <div className="window-select-card-admin-overlay absolute inset-0 bg-black/60 z-20 flex flex-col items-center justify-center text-white backdrop-blur-sm">
                    <Lock className="w-6 h-6 mb-2 text-amber-400" />
                    <span className="text-[10px] font-bold text-amber-400 uppercase tracking-wide">{t.adminRequired}</span>
                    <span className="text-[9px] text-white/80 mt-1 px-4 text-center">{t.adminRequiredDesc}</span>
                  </div>
                )}
                <div className="window-select-card-preview flex-1 flex items-center justify-center bg-black/10 dark:bg-black/20 relative">
                  {win.previewDataUrl ? (
                    <>
                      <img
                        src={win.previewDataUrl}
                        alt=""
                        className="window-select-card-preview-bg absolute inset-0 w-full h-full object-cover opacity-30 blur-sm"
                      />
                      <img
                        src={win.previewDataUrl}
                        alt=""
                        className="window-select-card-preview-image absolute inset-0 w-full h-full object-contain drop-shadow-lg"
                      />
                    </>
                  ) : win.iconDataUrl ? (
                    <img
                      src={win.iconDataUrl}
                      alt=""
                      className="window-select-card-icon w-14 h-14 rounded-2xl object-contain bg-black/20 p-2 shadow-md"
                    />
                  ) : (
                    <div className="window-select-card-badge w-14 h-14 rounded-2xl bg-gradient-to-br from-[var(--primary-color)] to-[#8a72d8] flex items-center justify-center text-white text-2xl font-bold shadow-md">
                      {initial}
                    </div>
                  )}

                  {win.previewDataUrl && (
                    <div className="window-select-card-mini-badge absolute bottom-1.5 right-1.5 z-10">
                      {win.iconDataUrl ? (
                        <img
                          src={win.iconDataUrl}
                          alt=""
                          className="window-select-card-mini-icon w-5 h-5 rounded object-contain bg-black/50 p-[1px] shadow-sm"
                        />
                      ) : (
                        <div className="window-select-card-mini-fallback w-5 h-5 rounded bg-gradient-to-br from-[var(--primary-color)] to-[#8a72d8] flex items-center justify-center text-white text-[10px] font-bold shadow-sm">
                          {initial}
                        </div>
                      )}
                    </div>
                  )}
                </div>

                <div className="window-select-card-meta p-2.5 border-t border-[var(--glass-border)] bg-[var(--surface)]">
                  <div className="window-select-card-title text-[11px] font-medium text-[var(--on-surface)] truncate" title={win.title}>
                    {win.title}
                  </div>
                  <div className="window-select-card-process text-[9px] text-[var(--on-surface-variant)] truncate mt-0.5">
                    {win.processName}
                  </div>
                </div>

              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
