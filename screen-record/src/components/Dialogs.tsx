import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, Loader2, AlertCircle, X, FolderOpen, Copy, CheckCircle2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { ExportOptions, VideoSegment, BackgroundConfig } from '@/types/video';
import {
  computeResolutionOptions,
  computeBitrateSliderBounds,
  getCanvasBaseDimensions,
  resolveExportDimensions,
  estimateExportSize,
  type ResolutionOption
} from '@/lib/videoExporter';
import { getTotalTrimDuration } from '@/lib/trimSegments';
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
  onCancel?: () => void;
}

function formatEta(seconds: number): string {
  if (seconds < 1) return '';
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.ceil(seconds % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function ProcessingOverlay({ show, onCancel }: ProcessingOverlayProps) {
  const { t } = useSettings();
  const [percent, setPercent] = useState(0);
  const [eta, setEta] = useState(0);
  const [active, setActive] = useState(false);

  useEffect(() => {
    if (!show) {
      setPercent(0);
      setEta(0);
      setActive(false);
      return;
    }
    // Listen for push progress updates from Rust via PostMessageW → evaluate_script
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'sr-export-progress') {
        setActive(true);
        setPercent(e.data.percent);
        setEta(e.data.eta);
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, [show]);

  if (!show) return null;

  const pct = Math.round(percent);
  const etaStr = formatEta(eta);

  return (
    <div className="processing-overlay-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="processing-dialog bg-[var(--surface-dim)] p-5 rounded-xl border border-[var(--glass-border)] shadow-lg w-72">
        <p className="processing-title text-sm font-medium text-[var(--on-surface)] mb-3">
          {active ? t.exportingVideo : t.preparingExport}
        </p>
        <div className="progress-bar-track h-1.5 w-full bg-[var(--glass-bg-hover)] rounded-full overflow-hidden mb-2">
          <div
            className="progress-bar-fill h-full bg-[var(--primary-color)] rounded-full transition-all duration-300 ease-out"
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="progress-details flex justify-between text-[10px]">
          <span className="progress-percent text-[var(--on-surface-variant)] tabular-nums">{active ? `${pct}%` : ''}</span>
          <span className="progress-eta text-[var(--outline)] tabular-nums">{etaStr ? `${etaStr} ${t.timeRemaining}` : ''}</span>
        </div>
        {onCancel && (
          <button
            onClick={() => { console.log('[Cancel] Button clicked'); onCancel(); }}
            className="cancel-export-btn mt-3 w-full py-1.5 rounded-lg text-xs font-medium text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] bg-[var(--glass-bg)] hover:bg-[var(--glass-bg-hover)] border border-[var(--glass-border)] transition-colors"
          >
            {t.cancel}
          </button>
        )}
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
  videoRef: React.RefObject<HTMLVideoElement | null>;
  backgroundConfig: BackgroundConfig;
  hasAudio: boolean;
}

const FPS_OPTIONS = [24, 30, 60] as const;

function formatDataSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatVideoBitrateKbps(kbps: number): string {
  if (kbps >= 1000) {
    return `${(kbps / 1000).toFixed(1)} Mbps`;
  }
  return `${Math.round(kbps)} kbps`;
}

export function ExportDialog({
  show,
  onClose,
  onExport,
  exportOptions,
  setExportOptions,
  segment,
  videoRef,
  backgroundConfig,
  hasAudio
}: ExportDialogProps) {
  const { t } = useSettings();
  const [isPickingDir, setIsPickingDir] = useState(false);

  useEffect(() => {
    if (!show || exportOptions.outputDir) return;

    invoke<string>('get_default_export_dir')
      .then((dir) => {
        if (dir) {
          setExportOptions(prev => ({ ...prev, outputDir: dir }));
        }
      })
      .catch((e) => console.error('[Export] Failed to get default export dir:', e));
  }, [show, exportOptions.outputDir, setExportOptions]);

  const handleBrowseOutputDir = async () => {
    try {
      setIsPickingDir(true);
      const selected = await invoke<string | null>('pick_export_folder', {
        initialDir: exportOptions.outputDir || null,
      });
      if (selected) {
        setExportOptions(prev => ({ ...prev, outputDir: selected }));
      }
    } catch (e) {
      console.error('[Export] Failed to pick export dir:', e);
    } finally {
      setIsPickingDir(false);
    }
  };

  const vidW = videoRef.current?.videoWidth || 1920;
  const vidH = videoRef.current?.videoHeight || 1080;
  const { baseW, baseH } = getCanvasBaseDimensions(vidW, vidH, segment, backgroundConfig);
  const resOptions = computeResolutionOptions(baseW, baseH);
  const { width: outW, height: outH } = resolveExportDimensions(exportOptions.width, exportOptions.height, baseW, baseH);
  const bitrateBounds = computeBitrateSliderBounds(outW, outH, exportOptions.fps);
  const targetVideoBitrateKbps = exportOptions.targetVideoBitrateKbps > 0
    ? Math.max(bitrateBounds.minKbps, Math.min(exportOptions.targetVideoBitrateKbps, bitrateBounds.maxKbps))
    : bitrateBounds.recommendedKbps;
  const standardBitratePercent = bitrateBounds.maxKbps > bitrateBounds.minKbps
    ? ((bitrateBounds.recommendedKbps - bitrateBounds.minKbps) / (bitrateBounds.maxKbps - bitrateBounds.minKbps)) * 100
    : 0;
  const sourceDuration = videoRef.current?.duration || segment?.trimEnd || 0;
  const trimmedDurationSec = segment ? getTotalTrimDuration(segment, sourceDuration) : 0;
  const sizeEstimate = estimateExportSize({
    width: outW,
    height: outH,
    fps: exportOptions.fps,
    targetVideoBitrateKbps,
    trimmedDurationSec,
    speed: exportOptions.speed,
    hasAudio,
    backgroundConfig,
    segment
  });

  useEffect(() => {
    if (!show) return;
    setExportOptions((prev) => {
      const current = prev.targetVideoBitrateKbps;
      const next = current > 0
        ? Math.max(bitrateBounds.minKbps, Math.min(current, bitrateBounds.maxKbps))
        : bitrateBounds.recommendedKbps;
      if (current === next) return prev;
      return { ...prev, targetVideoBitrateKbps: next };
    });
  }, [
    show,
    bitrateBounds.minKbps,
    bitrateBounds.maxKbps,
    bitrateBounds.recommendedKbps,
    setExportOptions
  ]);

  if (!show) return null;

  // Find currently selected resolution key, fall back to original (0×0)
  const selectedKey = `${exportOptions.width}x${exportOptions.height}`;

  return (
    <div className="export-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="export-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-[480px] w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.exportOptions}</h3>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="export-options-form space-y-4 mb-6">
          <div className="export-resolution-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.resolution}</label>
            <div className="resolution-options flex gap-2 flex-nowrap overflow-x-auto pb-1">
              {resOptions.map((opt: ResolutionOption) => {
                const key = `${opt.width}x${opt.height}`;
                const isSelected = selectedKey === key || (exportOptions.width === 0 && exportOptions.height === 0 && opt === resOptions[0]);
                return (
                  <button
                    key={key}
                    onClick={() => setExportOptions(prev => ({ ...prev, width: opt.width, height: opt.height }))}
                    className={`resolution-option py-1.5 px-3 rounded-lg text-xs font-medium transition-colors border whitespace-nowrap ${
                      isSelected
                        ? 'bg-[var(--primary-color)] text-white border-transparent'
                        : 'bg-[var(--glass-bg)] text-[var(--on-surface)] border-[var(--glass-border)] hover:bg-[var(--glass-bg-hover)]'
                    }`}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="export-fps-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.frameRate}</label>
            <div className="fps-options flex gap-2">
              {FPS_OPTIONS.map(fps => (
                <button
                  key={fps}
                  onClick={() => setExportOptions(prev => ({ ...prev, fps }))}
                  className={`fps-option flex-1 py-1.5 rounded-lg text-xs font-medium transition-colors border ${
                    exportOptions.fps === fps
                      ? 'bg-[var(--primary-color)] text-white border-transparent'
                      : 'bg-[var(--glass-bg)] text-[var(--on-surface)] border-[var(--glass-border)] hover:bg-[var(--glass-bg-hover)]'
                  }`}
                >
                  {fps} fps
                </button>
              ))}
            </div>
          </div>

          <div className="export-bitrate-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.videoBitrate}</label>
            <div className="bitrate-control bg-[var(--glass-bg)] rounded-lg p-3">
              <div className="bitrate-display flex items-center justify-between mb-3">
                <span className="text-sm text-[var(--on-surface)] tabular-nums">
                  {formatVideoBitrateKbps(targetVideoBitrateKbps)}
                </span>
                <span className="text-[10px] text-[var(--outline)] tabular-nums">
                  {formatVideoBitrateKbps(bitrateBounds.minKbps)} - {formatVideoBitrateKbps(bitrateBounds.maxKbps)}
                </span>
              </div>
              <div className="bitrate-slider-row flex items-center gap-3">
                <input
                  type="range"
                  min={bitrateBounds.minKbps}
                  max={bitrateBounds.maxKbps}
                  step={bitrateBounds.stepKbps}
                  value={targetVideoBitrateKbps}
                  onChange={(e) => setExportOptions(prev => ({ ...prev, targetVideoBitrateKbps: Number(e.target.value) }))}
                  className="flex-1 h-1 rounded"
                />
              </div>
              <div className="bitrate-standard-marker relative mt-1 h-5">
                <div
                  className="bitrate-standard-line absolute top-0 h-2 w-px bg-[var(--outline)]"
                  style={{ left: `calc(${standardBitratePercent}% - 0.5px)` }}
                />
                <div
                  className="bitrate-standard-label absolute top-[8px] -translate-x-1/2 text-[10px] text-[var(--outline)] whitespace-nowrap"
                  style={{ left: `${standardBitratePercent}%` }}
                >
                  {t.standard}
                </div>
              </div>
            </div>
          </div>

          <div className="export-speed-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.speed}</label>
            <div className="speed-control bg-[var(--glass-bg)] rounded-lg p-3">
              <div className="speed-display flex items-center justify-between mb-3">
                <div className="flex items-center gap-1.5">
                  <span className="text-sm text-[var(--on-surface)] tabular-nums">
                    {formatTime(sizeEstimate.outputDurationSec)}
                  </span>
                  {trimmedDurationSec > 0 && exportOptions.speed !== 1 && (
                    <span className={`text-xs ${exportOptions.speed > 1 ? 'text-red-400/90' : 'text-green-400/90'}`}>
                      {exportOptions.speed > 1 ? '↓' : '↑'}
                      {formatTime(Math.abs(trimmedDurationSec - sizeEstimate.outputDurationSec))}
                    </span>
                  )}
                </div>
                <span className="text-sm font-medium text-[var(--on-surface)] tabular-nums">{Math.round(exportOptions.speed * 100)}%</span>
              </div>
              <div className="speed-slider-row flex items-center gap-3">
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

          <div className="export-size-estimate-field">
            <div className="size-estimate-header flex items-center justify-between">
              <label className="text-xs text-[var(--on-surface-variant)]">{t.estimatedSize}</label>
              <div className="size-estimate-primary text-sm font-medium text-[var(--on-surface)] tabular-nums">
                ~{formatDataSize(sizeEstimate.estimatedBytes)}
              </div>
            </div>
          </div>

          <div className="export-location-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.saveLocation}</label>
            <div className="flex items-center gap-2">
              <div
                className="flex-1 min-w-0 text-xs text-[var(--on-surface)] bg-[var(--glass-bg)] border border-[var(--glass-border)] rounded-lg px-3 py-2 truncate"
                title={exportOptions.outputDir || ''}
              >
                {exportOptions.outputDir || '-'}
              </div>
              <Button
                variant="outline"
                onClick={handleBrowseOutputDir}
                disabled={isPickingDir}
                className="h-8 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)]"
              >
                <FolderOpen className="w-3.5 h-3.5 mr-1.5" />
                {isPickingDir ? t.browsing : t.browse}
              </Button>
            </div>
          </div>
        </div>

        <div className="dialog-actions flex justify-end gap-2">
          <Button variant="outline" onClick={onClose} className="bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] hover:text-[var(--on-surface)] rounded-lg text-xs h-8">{t.cancel}</Button>
          <Button onClick={onExport} className="bg-[var(--primary-color)] hover:opacity-90 text-white rounded-lg text-xs h-8">{t.exportVideo}</Button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// RawVideoDialog
// ============================================================================
interface RawVideoDialogProps {
  show: boolean;
  onClose: () => void;
  savedPath: string;
  autoCopyEnabled: boolean;
  isBusy?: boolean;
  onChangePath: () => void;
  onCopyVideo: () => void;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function RawVideoDialog({
  show,
  onClose,
  savedPath,
  autoCopyEnabled,
  isBusy = false,
  onChangePath,
  onCopyVideo,
  onToggleAutoCopy
}: RawVideoDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="raw-video-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="raw-video-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-md w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.rawVideoDialogTitle}</h3>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="raw-video-dialog-content space-y-4">
          <div
            className={`raw-video-path-block rounded-lg px-3 py-2.5 ${
              savedPath
                ? 'border border-emerald-400/45 bg-emerald-500/10 shadow-[0_0_0_1px_rgba(16,185,129,0.2)_inset,0_8px_20px_rgba(5,150,105,0.12)]'
                : 'border border-[var(--glass-border)] bg-[var(--glass-bg)]'
            }`}
          >
            {savedPath ? (
              <>
                <div className="raw-video-saved-title-row flex items-center gap-1.5 mb-1">
                  <CheckCircle2 className="raw-video-saved-icon w-3.5 h-3.5 text-emerald-300 flex-shrink-0" />
                  <div className="raw-video-saved-title text-[11px] font-semibold text-emerald-200">{t.rawVideoSavedTo}</div>
                </div>
                <div className="raw-video-saved-path text-xs text-[var(--on-surface)] break-all">{savedPath}</div>
              </>
            ) : (
              <div className="text-xs text-[var(--on-surface-variant)]">{t.rawVideoPathUnavailable}</div>
            )}
          </div>

          <div className="raw-video-action-row flex items-center gap-2">
            <Button
              variant="outline"
              onClick={onChangePath}
              disabled={isBusy}
              className="h-8 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)]"
            >
              <FolderOpen className="w-3.5 h-3.5 mr-1.5" />
              {t.changePath}
            </Button>
            <Button
              onClick={onCopyVideo}
              disabled={isBusy || !savedPath}
              className="h-8 text-xs bg-[var(--primary-color)] hover:opacity-90 text-white"
            >
              <Copy className="w-3.5 h-3.5 mr-1.5" />
              {t.copyVideo}
            </Button>
          </div>

          <label className="raw-video-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface)]">
            <input
              type="checkbox"
              checked={autoCopyEnabled}
              onChange={(e) => onToggleAutoCopy(e.target.checked)}
              disabled={isBusy}
            />
            <span>{t.autoCopyAfterRecording}</span>
          </label>
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
    <div className="monitor-select-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="monitor-select-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-md w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.selectMonitor}</h3>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="monitor-list space-y-1.5">
          {monitors.map((monitor) => (
            <button
              key={monitor.id}
              onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
              className="monitor-item w-full p-3 rounded-lg border border-[var(--glass-border)] hover:bg-[var(--glass-bg)] hover:border-[var(--outline)] transition-colors text-left"
            >
              <div className="monitor-name text-sm text-[var(--on-surface)]">{monitor.name}</div>
              <div className="monitor-specs text-xs text-[var(--outline)] mt-0.5">{monitor.width}x{monitor.height} at ({monitor.x}, {monitor.y})</div>
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
    <div className="hotkey-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-50">
      <div className="hotkey-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-sm w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <div className="dialog-header-icon flex items-center gap-2">
            <Keyboard className="w-4 h-4 text-[var(--on-surface-variant)]" />
            <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.pressKeys}</h3>
          </div>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="hotkey-hint text-[var(--outline)] text-xs">{t.pressKeysHint}</p>
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
    <div className="ffmpeg-setup-backdrop fixed inset-0 bg-black/90 flex items-center justify-center z-[100]">
      <div className="ffmpeg-setup-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-sm w-full mx-4">
        <div className="setup-status-header flex items-center gap-2.5 mb-3">
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

        <p className="setup-description text-[var(--outline)] mb-4 text-xs leading-relaxed">
          {ffmpegInstallStatus.type === 'Downloading' ? t.ffmpegDesc :
            ffmpegInstallStatus.type === 'Extracting' ? t.extractingDesc :
              ffmpegInstallStatus.type === 'Error' ? ffmpegInstallStatus.message :
                ffmpegInstallStatus.type === 'Cancelled' ? t.cancelledDesc : t.systemCheckDesc}
        </p>

        {(ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting') && (
          <div className="progress-section space-y-2 mb-4">
            <div className="progress-bar-track h-1 w-full bg-[var(--glass-bg-hover)] rounded-full overflow-hidden">
              <div
                className="progress-bar-fill h-full bg-[var(--primary-color)] transition-all duration-300 ease-out"
                style={{ width: `${ffmpegInstallStatus.type === 'Downloading' ? ffmpegInstallStatus.progress : 95}%` }}
              />
            </div>
            {ffmpegInstallStatus.type === 'Downloading' && (
              <div className="progress-details flex justify-between text-[10px]">
                <span className="text-[var(--on-surface-variant)]">
                  {Math.round(ffmpegInstallStatus.progress)}% {t.downloaded}
                  {ffmpegInstallStatus.totalSize > 0 && ` of ${(ffmpegInstallStatus.totalSize / (1024 * 1024)).toFixed(1)} MB`}
                </span>
                <span className="text-[var(--outline)]">{t.ffmpegEssentials}</span>
              </div>
            )}
          </div>
        )}

        <div className="setup-actions flex flex-col gap-2">
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
