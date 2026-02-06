import { Button } from '@/components/ui/button';
import { Video, Keyboard, Loader2, AlertCircle, X } from 'lucide-react';
import { ExportOptions, VideoSegment } from '@/types/video';
import { EXPORT_PRESETS, DIMENSION_PRESETS } from '@/lib/videoExporter';
import { formatTime } from '@/utils/helpers';
import { MonitorInfo, Hotkey, FfmpegInstallStatus } from '@/hooks/useAppHooks';
import { useSettings } from '@/hooks/useSettings';

// Re-export types for backwards compatibility
export type { MonitorInfo, Hotkey, FfmpegInstallStatus };

// ============================================================================
// ProcessingOverlay
// ============================================================================
interface ProcessingOverlayProps {
  show: boolean;
  exportProgress: number;
}

export function ProcessingOverlay({ show, exportProgress }: ProcessingOverlayProps) {
  const { t } = useSettings();
  if (!show) return null;
  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)] p-6 rounded-lg border border-[var(--glass-border)] shadow-lg">
        <p className="text-lg text-[var(--on-surface)]">
          {exportProgress > 0 ? `${t.exportingVideo} ${Math.round(exportProgress)}%` : t.processingVideoShort}
        </p>
      </div>
    </div>
  );
}

// ============================================================================
// ExportDialog
// ============================================================================
interface ExportDialogProps {
  show: boolean;
  onClose: () => void;
  onExport: () => void;
  exportOptions: ExportOptions;
  setExportOptions: React.Dispatch<React.SetStateAction<ExportOptions>>;
  segment: VideoSegment | null;
}

export function ExportDialog({ show, onClose, onExport, exportOptions, setExportOptions, segment }: ExportDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-md w-full mx-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-medium text-[var(--on-surface)]">{t.exportOptions}</h3>
          <button onClick={onClose} className="p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-4 mb-6">
          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.quality}</label>
            <select
              value={exportOptions.quality}
              onChange={(e) => setExportOptions(prev => ({ ...prev, quality: e.target.value as ExportOptions['quality'] }))}
              className="w-full bg-[var(--glass-bg)] border border-[var(--glass-border)] rounded-lg px-3 py-2 text-[var(--on-surface)]"
            >
              {Object.entries(EXPORT_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.dimensions}</label>
            <select
              value={exportOptions.dimensions}
              onChange={(e) => setExportOptions(prev => ({ ...prev, dimensions: e.target.value as ExportOptions['dimensions'] }))}
              className="w-full bg-[var(--glass-bg)] border border-[var(--glass-border)] rounded-lg px-3 py-2 text-[var(--on-surface)]"
            >
              {Object.entries(DIMENSION_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.speed}</label>
            <div className="bg-[var(--glass-bg)] rounded-lg p-3">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-1.5">
                  <span className="text-sm text-[var(--on-surface)] tabular-nums">
                    {formatTime(segment ? (segment.trimEnd - segment.trimStart) / exportOptions.speed : 0)}
                  </span>
                  {segment && exportOptions.speed !== 1 && (
                    <span className={`text-xs ${exportOptions.speed > 1 ? 'text-red-400/90' : 'text-green-400/90'}`}>
                      {exportOptions.speed > 1 ? '↓' : '↑'}
                      {formatTime(Math.abs((segment.trimEnd - segment.trimStart) - ((segment.trimEnd - segment.trimStart) / exportOptions.speed)))}
                    </span>
                  )}
                </div>
                <span className="text-sm font-medium text-[var(--on-surface)] tabular-nums">{Math.round(exportOptions.speed * 100)}%</span>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-xs text-[var(--outline)] min-w-[36px]">{t.slower}</span>
                <input
                  type="range"
                  min="50"
                  max="200"
                  step="10"
                  value={exportOptions.speed * 100}
                  onChange={(e) => setExportOptions(prev => ({ ...prev, speed: Number(e.target.value) / 100 }))}
                  className="flex-1 h-1 rounded"
                />
                <span className="text-xs text-[var(--outline)] min-w-[36px]">{t.faster}</span>
              </div>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose} className="bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] hover:text-[var(--on-surface)] rounded-lg text-xs h-8">{t.cancel}</Button>
          <Button onClick={onExport} className="bg-[var(--primary-color)] hover:opacity-90 text-white rounded-lg text-xs h-8">{t.exportVideo}</Button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// MonitorSelectDialog
// ============================================================================
interface MonitorSelectDialogProps {
  show: boolean;
  onClose: () => void;
  monitors: MonitorInfo[];
  onSelectMonitor: (monitorId: string) => void;
}

export function MonitorSelectDialog({ show, onClose, monitors, onSelectMonitor }: MonitorSelectDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-md w-full mx-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-medium text-[var(--on-surface)]">{t.selectMonitor}</h3>
          <button onClick={onClose} className="p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="space-y-1.5">
          {monitors.map((monitor) => (
            <button
              key={monitor.id}
              onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
              className="w-full p-3 rounded-lg border border-[var(--glass-border)] hover:bg-[var(--glass-bg)] hover:border-[var(--outline)] transition-colors text-left"
            >
              <div className="text-sm text-[var(--on-surface)]">{monitor.name}</div>
              <div className="text-xs text-[var(--outline)] mt-0.5">{monitor.width}x{monitor.height} at ({monitor.x}, {monitor.y})</div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// HotkeyDialog
// ============================================================================
interface HotkeyDialogProps {
  show: boolean;
  onClose: () => void;
}

export function HotkeyDialog({ show, onClose }: HotkeyDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-sm w-full mx-4">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <Keyboard className="w-4 h-4 text-[var(--on-surface-variant)]" />
            <h3 className="text-sm font-medium text-[var(--on-surface)]">{t.pressKeys}</h3>
          </div>
          <button onClick={onClose} className="p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="text-[var(--outline)] text-xs">{t.pressKeysHint}</p>
      </div>
    </div>
  );
}

// ============================================================================
// FfmpegSetupDialog
// ============================================================================
interface FfmpegSetupDialogProps {
  show: boolean;
  ffmpegInstallStatus: FfmpegInstallStatus;
  onCancelInstall: () => void;
}

export function FfmpegSetupDialog({ show, ffmpegInstallStatus, onCancelInstall }: FfmpegSetupDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/90 flex items-center justify-center z-[100]">
      <div className="bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-sm w-full mx-4">
        <div className="flex items-center gap-2.5 mb-3">
          {ffmpegInstallStatus.type === 'Error' ? (
            <AlertCircle className="w-5 h-5 text-red-500 flex-shrink-0" />
          ) : ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting' ? (
            <Loader2 className="w-5 h-5 text-[var(--primary-color)] animate-spin flex-shrink-0" />
          ) : (
            <Video className="w-5 h-5 text-[var(--on-surface-variant)] flex-shrink-0" />
          )}
          <h3 className="text-sm font-medium text-[var(--on-surface)]">
            {ffmpegInstallStatus.type === 'Downloading' ? t.downloadingDeps :
              ffmpegInstallStatus.type === 'Extracting' ? t.settingUp :
                ffmpegInstallStatus.type === 'Error' ? t.installFailed :
                  ffmpegInstallStatus.type === 'Cancelled' ? t.installCancelled : t.preparingRecorder}
          </h3>
        </div>

        <p className="text-[var(--outline)] mb-4 text-xs leading-relaxed">
          {ffmpegInstallStatus.type === 'Downloading' ? t.ffmpegDesc :
            ffmpegInstallStatus.type === 'Extracting' ? t.extractingDesc :
              ffmpegInstallStatus.type === 'Error' ? ffmpegInstallStatus.message :
                ffmpegInstallStatus.type === 'Cancelled' ? t.cancelledDesc : t.systemCheckDesc}
        </p>

        {(ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting') && (
          <div className="space-y-2 mb-4">
            <div className="h-1 w-full bg-[var(--glass-bg-hover)] rounded-full overflow-hidden">
              <div
                className="h-full bg-[var(--primary-color)] transition-all duration-300 ease-out"
                style={{ width: `${ffmpegInstallStatus.type === 'Downloading' ? ffmpegInstallStatus.progress : 95}%` }}
              />
            </div>
            {ffmpegInstallStatus.type === 'Downloading' && (
              <div className="flex justify-between text-[10px]">
                <span className="text-[var(--on-surface-variant)]">
                  {Math.round(ffmpegInstallStatus.progress)}% {t.downloaded}
                  {ffmpegInstallStatus.totalSize > 0 && ` of ${(ffmpegInstallStatus.totalSize / (1024 * 1024)).toFixed(1)} MB`}
                </span>
                <span className="text-[var(--outline)]">{t.ffmpegEssentials}</span>
              </div>
            )}
          </div>
        )}

        <div className="flex flex-col gap-2">
          {ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled' ? (
            <Button onClick={() => window.location.reload()} className="w-full bg-[var(--primary-color)] hover:opacity-90 text-white rounded-lg text-xs h-9">
              {t.tryAgain}
            </Button>
          ) : (
            <Button
              variant="ghost"
              onClick={onCancelInstall}
              disabled={ffmpegInstallStatus.type === 'Idle' || ffmpegInstallStatus.type === 'Extracting'}
              className="w-full text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg)] rounded-lg border border-[var(--glass-border)] text-xs h-9"
            >
              {t.cancelInstallation}
            </Button>
          )}
          {(ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled') && (
            <Button variant="ghost" onClick={() => (window as any).ipc.postMessage('close_window')} className="w-full text-red-400 hover:text-red-300 hover:bg-red-500/10 rounded-lg text-xs h-9">
              {t.closeApp}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
