import { useState, useEffect, useRef, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Keyboard, X, FolderOpen, Copy, CheckCircle2 } from 'lucide-react';
import { invoke } from '@/lib/ipc';
import { ExportOptions, VideoSegment, BackgroundConfig } from '@/types/video';
import {
  computeResolutionOptions,
  computeBitrateSliderBounds,
  getCanvasBaseDimensions,
  resolveExportDimensions,
  estimateExportSize,
  videoExporter,
  type ExportCapabilities,
  type ResolutionOption
} from '@/lib/videoExporter';
import { getTotalTrimDuration } from '@/lib/trimSegments';
import { formatTime } from '@/utils/helpers';
import { MonitorInfo, Hotkey } from '@/hooks/useAppHooks';
import { useSettings } from '@/hooks/useSettings';

// Re-export types for backwards compatibility
export type { MonitorInfo, Hotkey };

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
  const [diagnosticsLine, setDiagnosticsLine] = useState('');

  useEffect(() => {
    if (!show) {
      setPercent(0);
      setEta(0);
      setActive(false);
      setDiagnosticsLine('');
      return;
    }
    // Listen for push progress updates from Rust via PostMessageW → evaluate_script
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'sr-export-progress') {
        setActive(true);
        setPercent(e.data.percent);
        setEta(e.data.eta);
      } else if (e.data?.type === 'sr-export-diagnostics') {
        const d = e.data.diagnostics || {};
        const mode = d.turbo ? 'Turbo' : 'Standard';
        const codec = typeof d.codec === 'string' ? d.codec : '-';
        const backend = typeof d.backend === 'string' ? d.backend : '-';
        const sfe = d.sfe ? ' + SFE' : '';
        const deviation = typeof d.bitrateDeviationPercent === 'number'
          ? `${Math.abs(d.bitrateDeviationPercent).toFixed(1)}%`
          : '-';
        const fallback = d.fallbackUsed ? ' · fallback' : '';
        setDiagnosticsLine(`${mode} · ${codec}${sfe} · ${backend} · dev ${deviation}${fallback}`);
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, [show]);

  if (!show) return null;

  const pct = Math.round(percent);
  const etaStr = formatEta(eta);

  return (
    <div className="processing-overlay-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
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
          <span className="progress-eta text-[var(--on-surface-variant)] tabular-nums">{etaStr ? `${etaStr} ${t.timeRemaining}` : ''}</span>
        </div>
        {diagnosticsLine && (
          <div className="processing-diagnostics mt-2 text-[10px] text-[var(--on-surface-variant)] tabular-nums">
            {diagnosticsLine}
          </div>
        )}
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
  sourceVideoFps?: number | null;
}

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

function formatResolutionPLabel(height: number): string {
  const h = Math.max(1, Math.round(height));
  return `${h}p`;
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
  hasAudio,
  sourceVideoFps
}: ExportDialogProps) {
  const { t } = useSettings();
  const [isPickingDir, setIsPickingDir] = useState(false);
  const [exportCapabilities, setExportCapabilities] = useState<ExportCapabilities | null>(null);
  const [capabilityProbeFailed, setCapabilityProbeFailed] = useState(false);
  const autoMatchFpsPendingRef = useRef(false);

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

  useEffect(() => {
    if (!show) return;

    let cancelled = false;
    setCapabilityProbeFailed(false);

    void videoExporter.getExportCapabilities()
      .then((caps) => {
        if (cancelled) return;
        setExportCapabilities(caps);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn('[ExportDialog] capability probe failed:', error);
        setCapabilityProbeFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [show]);

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
  const sourceResOptionW = baseW % 2 === 0 ? baseW : Math.max(2, baseW - 1);
  const sourceResOptionH = baseH % 2 === 0 ? baseH : Math.max(2, baseH - 1);
  const nativeSourceFps = typeof sourceVideoFps === 'number' && Number.isFinite(sourceVideoFps) && sourceVideoFps > 0
    ? sourceVideoFps
    : null;
  const sourceFpsValue = nativeSourceFps !== null
    ? Math.round(nativeSourceFps)
    : 60;
  const sourceResLabel = `${sourceResOptionW}×${sourceResOptionH}`;
  const fpsChoiceValues = Array.from(new Set([sourceFpsValue, 24, 30, 60])).sort((a, b) => a - b);
  const resOptions = computeResolutionOptions(baseW, baseH, vidH)
    .slice()
    .sort((a, b) => b.height - a.height || b.width - a.width);
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
  const wantsTurbo = exportOptions.exportProfile === 'turbo_nv' || exportOptions.preferNvTurbo;
  const turboCodecLabel = (exportOptions.turboCodec || 'hevc').toUpperCase();
  const backendStatus = (() => {
    if (!exportCapabilities && !capabilityProbeFailed) {
      return {
        label: t.exportBackendDetecting,
        detail: '',
        tone: 'text-[var(--outline)]'
      };
    }
    if (capabilityProbeFailed) {
      return {
        label: t.exportBackendCpuX264,
        detail: t.exportBackendProbeFailedFallback,
        tone: 'text-amber-700 dark:text-amber-400'
      };
    }
    if (!exportCapabilities) {
      return {
        label: t.exportBackendCpuX264,
        detail: t.exportBackendNoCapabilityData,
        tone: 'text-amber-700 dark:text-amber-400'
      };
    }
    if (exportCapabilities.pipeline === 'zero_copy_gpu') {
      return {
        label: t.exportBackendZeroCopyGpu,
        detail: exportCapabilities.mfH264Available ? t.exportBackendMfH264Encode : t.hardwareEncode,
        tone: 'text-emerald-700 dark:text-emerald-400'
      };
    }
    if (wantsTurbo) {
      if (exportCapabilities.nvencAvailable) {
        return {
          label: `${t.exportBackendNvencTurbo} (${turboCodecLabel})`,
          detail: t.exportBackendNvencFallbackIfError,
          tone: 'text-emerald-700 dark:text-emerald-400'
        };
      }
      return {
        label: t.exportBackendCpuX264,
        detail: t.exportBackendNvencUnavailable,
        tone: 'text-amber-700 dark:text-amber-400'
      };
    }
    return {
      label: t.exportBackendCpuX264,
      detail: t.softwareEncode,
      tone: 'text-amber-700 dark:text-amber-400'
    };
  })();

  useEffect(() => {
    if (!show) {
      autoMatchFpsPendingRef.current = false;
      return;
    }

    // Default to "match recorded" resolution each time the dialog opens.
    setExportOptions((prev) => {
      if (prev.width === 0 && prev.height === 0) return prev;
      return { ...prev, width: 0, height: 0 };
    });

    // Keep FPS on auto-match until the user manually picks another option.
    autoMatchFpsPendingRef.current = true;
  }, [show, setExportOptions]);

  useEffect(() => {
    if (!show || !autoMatchFpsPendingRef.current) return;
    setExportOptions((prev) => (
      prev.fps === sourceFpsValue
        ? prev
        : { ...prev, fps: sourceFpsValue }
    ));
  }, [show, sourceFpsValue, setExportOptions]);

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
    <div className="export-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="export-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-[500px] w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.exportOptions}</h3>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="export-options-form space-y-4 mb-6">
          <div className="export-resolution-field">
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs font-medium text-[var(--on-surface-variant)]">{t.resolution}</label>
            </div>
            <div className="resolution-options grid grid-cols-3 gap-2">
              {resOptions.map((opt: ResolutionOption) => {
                const key = `${opt.width}x${opt.height}`;
                const isSourceOption = opt.width === sourceResOptionW && opt.height === sourceResOptionH;
                const isSelected = selectedKey === key || (exportOptions.width === 0 && exportOptions.height === 0 && isSourceOption);
                return (
                  <button
                    key={key}
                    onClick={() => setExportOptions(prev => ({ ...prev, width: isSourceOption ? 0 : opt.width, height: isSourceOption ? 0 : opt.height }))}
                    className={`resolution-option py-2 px-3 rounded-xl text-xs font-semibold transition-all border shadow-sm flex flex-col items-center justify-center gap-0.5 relative ${
                      isSelected
                        ? 'bg-[var(--primary-color)] text-white border-[var(--primary-color)]'
                        : 'bg-[var(--surface)] text-[var(--on-surface)] border-[var(--glass-border)] hover:border-[var(--outline)] hover:bg-[var(--surface-container)]'
                    }`}
                  >
                    <span>{formatResolutionPLabel(isSourceOption ? sourceResOptionH : opt.height)}</span>
                    <span className="text-[9px] opacity-70 font-mono">
                      {isSourceOption ? sourceResLabel : `${opt.width}×${opt.height}`}
                    </span>
                    <span className={`text-[9px] opacity-70 ${isSourceOption ? 'block' : 'invisible'}`}>
                      {isSourceOption ? t.matchRecorded : '.'}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="export-fps-field">
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs font-medium text-[var(--on-surface-variant)]">{t.frameRate}</label>
            </div>
            <div className="fps-options flex gap-2">
              {fpsChoiceValues.map((fps) => {
                const isSourceOption = fps === sourceFpsValue;
                return (
                  <button
                    key={fps}
                    onClick={() => {
                      autoMatchFpsPendingRef.current = false;
                      setExportOptions(prev => ({ ...prev, fps }));
                    }}
                    className={`fps-option flex-1 py-1.5 rounded-lg text-xs font-medium transition-colors border ${
                      exportOptions.fps === fps
                        ? 'bg-[var(--primary-color)] text-white border-transparent shadow-sm'
                        : 'bg-[var(--glass-bg)] text-[var(--on-surface)] border-[var(--glass-border)] hover:bg-[var(--glass-bg-hover)]'
                    }`}
                  >
                    <div className="flex flex-col items-center leading-tight">
                      <span>{`${fps} fps`}</span>
                      <span className={`text-[9px] opacity-70 ${isSourceOption ? 'block' : 'invisible'}`}>{isSourceOption ? t.matchRecorded : '.'}</span>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="export-bitrate-field">
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">{t.videoBitrate}</label>
            <div className="bitrate-control bg-[var(--glass-bg)] rounded-lg p-3">
              <div className="bitrate-display flex items-center justify-between mb-3">
                <span className="text-sm text-[var(--on-surface)] tabular-nums">
                  {formatVideoBitrateKbps(targetVideoBitrateKbps)}
                </span>
                <span className="bitrate-range text-[10px] text-[var(--on-surface-variant)] tabular-nums">
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
                  className="bitrate-standard-line absolute top-0 h-2 w-px bg-[var(--on-surface-variant)]/70"
                  style={{ left: `calc(${standardBitratePercent}% - 0.5px)` }}
                />
                <div
                  className="bitrate-standard-label absolute top-[8px] -translate-x-1/2 text-[10px] text-[var(--on-surface-variant)] whitespace-nowrap"
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
                    <span className={`text-xs ${exportOptions.speed > 1 ? 'text-red-600 dark:text-red-400/90' : 'text-emerald-700 dark:text-green-400/90'}`}>
                      {exportOptions.speed > 1 ? '↓' : '↑'}
                      {formatTime(Math.abs(trimmedDurationSec - sizeEstimate.outputDurationSec))}
                    </span>
                  )}
                </div>
                <span className="text-sm font-medium text-[var(--on-surface)] tabular-nums">{Math.round(exportOptions.speed * 100)}%</span>
              </div>
              <div className="speed-slider-row flex items-center gap-3">
                <span className="speed-slider-label speed-slider-label-slower text-xs text-[var(--on-surface-variant)] min-w-[36px]">{t.slower}</span>
                <input
                  type="range"
                  min="50"
                  max="200"
                  step="10"
                  value={exportOptions.speed * 100}
                  onChange={(e) => setExportOptions(prev => ({ ...prev, speed: Number(e.target.value) / 100 }))}
                  className="flex-1 h-1 rounded"
                />
                <span className="speed-slider-label speed-slider-label-faster text-xs text-[var(--on-surface-variant)] min-w-[36px]">{t.faster}</span>
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
            <div className="export-backend-indicator-row mt-1.5 flex items-start justify-between gap-3">
              <span className="export-backend-label text-[10px] text-[var(--on-surface-variant)]">{t.backendExport}</span>
              <div className="export-backend-value text-right">
                <div className={`text-[10px] font-medium ${backendStatus.tone}`}>
                  {backendStatus.label}
                </div>
                {backendStatus.detail ? (
                  <div className="export-backend-detail text-[10px] text-[var(--on-surface-variant)]">
                    {backendStatus.detail}
                  </div>
                ) : null}
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
// ConfirmDialog
// ============================================================================
interface ConfirmDialogProps {
  show: boolean;
  title: string;
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ show, title, message, onConfirm, onCancel }: ConfirmDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="confirm-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[110]">
      <div className="confirm-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-2xl max-w-sm w-full mx-4">
        <h3 className="text-sm font-semibold text-[var(--on-surface)] mb-2">{title}</h3>
        <p className="text-xs text-[var(--on-surface-variant)] mb-5">{message}</p>
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onCancel} className="h-8 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)]">
            {t.cancel}
          </Button>
          <Button onClick={onConfirm} className="h-8 text-xs bg-red-600 hover:bg-red-700 text-white">
            {t.clearAll || 'Confirm'}
          </Button>
        </div>
      </div>
    </div>
  );
}

// MediaResultDialog (Used for Raw Video & Export Success)
// ============================================================================
interface MediaResultDialogProps {
  show: boolean;
  onClose: () => void;
  title: string;
  filePath: string;
  onFilePathChange: (newPath: string) => void;
  isBusy?: boolean;
  extraControls?: ReactNode;
}

export function MediaResultDialog({
  show,
  onClose,
  title,
  filePath,
  onFilePathChange,
  isBusy = false,
  extraControls
}: MediaResultDialogProps) {
  const { t } = useSettings();
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [streamUrl, setStreamUrl] = useState('');
  const [previewReady, setPreviewReady] = useState(false);

  useEffect(() => {
    if (show && filePath) {
      setPreviewReady(false);
      const parts = filePath.split(/[/\\]/);
      setRenameValue(parts[parts.length - 1] || '');
      invoke<number>('get_media_server_port').then((port) => {
        if (port > 0) {
          setStreamUrl(`http://127.0.0.1:${port}/?path=${encodeURIComponent(filePath)}`);
        }
      }).catch(console.error);
    } else {
      setStreamUrl('');
    }
  }, [show, filePath]);

  if (!show) return null;

  const handleShowInFolder = async () => {
    if (!filePath) return;
    try { await invoke('show_in_folder', { path: filePath }); } catch (e) { console.error(e); }
  };

  const handleCopyVideo = async () => {
    if (!filePath) return;
    try { await invoke('copy_video_file_to_clipboard', { filePath }); } catch (e) { console.error(e); }
  };

  const submitRename = async () => {
    if (!renameValue.trim() || !filePath) {
      setIsRenaming(false);
      return;
    }
    try {
      const newPath = await invoke<string>('rename_file', { path: filePath, newName: renameValue.trim() });
      if (newPath) onFilePathChange(newPath);
    } catch (e) {
      console.error('Rename failed', e);
    } finally {
      setIsRenaming(false);
    }
  };

  return (
    <div className="media-result-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="media-result-dialog bg-[var(--surface-dim)] p-5 rounded-xl border border-[var(--glass-border)] shadow-2xl max-w-[640px] w-full mx-4 flex flex-col gap-4">
        <div className="media-result-title-row flex items-center justify-between gap-3">
          <h3 className="media-result-title text-sm font-semibold text-[var(--on-surface)]">{title}</h3>
          <button
            onClick={onClose}
            className="media-result-close-btn p-1.5 rounded-lg text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="media-result-header-row flex items-center gap-2 rounded-lg border border-emerald-400/45 bg-emerald-500/10 px-3 py-2.5">
          <CheckCircle2 className="h-4 w-4 flex-shrink-0 text-emerald-600 dark:text-emerald-300" />
          <div className="min-w-0 flex-1 text-sm text-emerald-700 dark:text-emerald-200 whitespace-nowrap">
            <span className="font-semibold">{t.savedTo}</span>
            <span className="mx-1 opacity-70">·</span>
            <span className="inline-block max-w-full truncate align-middle" title={`${title}: ${filePath}`}>{filePath}</span>
          </div>
        </div>

        {filePath ? (
          <div className="media-result-content flex flex-col gap-4">
            <div className="media-preview-box relative aspect-video min-h-[220px] rounded-lg overflow-hidden bg-black border border-[var(--glass-border)] shadow-inner">
              {streamUrl && (
                <video
                  src={streamUrl}
                  controls
                  preload="metadata"
                  onLoadedData={() => setPreviewReady(true)}
                  onCanPlay={() => setPreviewReady(true)}
                  className="media-preview-video absolute inset-0 h-full w-full object-contain"
                />
              )}
              {!previewReady && (
                <div className="media-preview-placeholder absolute inset-0 flex items-center justify-center text-xs text-white/70">
                  {title}
                </div>
              )}
            </div>

            <div className="media-path-editor bg-[var(--surface-container)] rounded-lg p-3 border border-[var(--glass-border)] shadow-sm">
              {isRenaming ? (
                <div className="flex gap-2">
                  <input
                    autoFocus
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') submitRename();
                      if (e.key === 'Escape') setIsRenaming(false);
                    }}
                    className="flex-1 bg-[var(--surface)] text-sm px-3 py-1.5 rounded border border-[var(--primary-color)] outline-none"
                  />
                  <Button size="sm" onClick={submitRename} disabled={isBusy} className="h-8 text-xs bg-[var(--primary-color)] text-white hover:opacity-90">{t.save}</Button>
                </div>
              ) : (
                <div className="flex items-center justify-between gap-3">
                  <div className="text-sm text-[var(--on-surface)] truncate font-medium flex-1" title={filePath}>{renameValue}</div>
                  <Button size="sm" variant="ghost" onClick={() => setIsRenaming(true)} disabled={isBusy} className="h-8 text-xs flex-shrink-0 hover:bg-[var(--glass-bg-hover)] border border-[var(--glass-border)]">{t.rename}</Button>
                </div>
              )}
            </div>

            <div className="media-actions flex items-center justify-between mt-2 gap-3">
              <div className="flex gap-2">
                <Button variant="outline" onClick={handleShowInFolder} disabled={isBusy} className="h-9 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-all">
                  <FolderOpen className="w-3.5 h-3.5 mr-1.5" /> {t.showInFolder}
                </Button>
                <Button onClick={handleCopyVideo} disabled={isBusy} className="h-9 text-xs bg-[var(--primary-color)] hover:opacity-90 text-white transition-all shadow-sm">
                  <Copy className="w-3.5 h-3.5 mr-1.5" /> {t.copyVideo}
                </Button>
              </div>
              {extraControls}
            </div>
          </div>
        ) : (
          <div className="text-sm text-[var(--on-surface-variant)] py-4 text-center">{t.rawVideoPathUnavailable}</div>
        )}
      </div>
    </div>
  );
}

// RawVideoDialog & ExportSuccessDialog Wrappers
// ============================================================================
interface RawVideoDialogProps {
  show: boolean;
  onClose: () => void;
  savedPath: string;
  autoCopyEnabled: boolean;
  isBusy?: boolean;
  onChangePath: (newPath: string) => void;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function RawVideoDialog({
  show,
  onClose,
  savedPath,
  autoCopyEnabled,
  isBusy = false,
  onChangePath,
  onToggleAutoCopy
}: RawVideoDialogProps) {
  const { t } = useSettings();
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.rawVideoDialogTitle}
      filePath={savedPath}
      onFilePathChange={onChangePath}
      isBusy={isBusy}
      extraControls={
        <label className="media-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
          <input type="checkbox" className="rounded border-[var(--outline)]" checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} disabled={isBusy} />
          <span>{t.autoCopyAfterRecording}</span>
        </label>
      }
    />
  );
}

interface ExportSuccessDialogProps {
  show: boolean;
  onClose: () => void;
  filePath: string;
  onFilePathChange: (newPath: string) => void;
  autoCopyEnabled: boolean;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function ExportSuccessDialog({ show, onClose, filePath, onFilePathChange, autoCopyEnabled, onToggleAutoCopy }: ExportSuccessDialogProps) {
  const { t } = useSettings();
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.exportSuccessful}
      filePath={filePath}
      onFilePathChange={onFilePathChange}
      extraControls={
        <label className="media-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
          <input type="checkbox" className="rounded border-[var(--outline)]" checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} />
          <span>{t.autoCopyAfterExport}</span>
        </label>
      }
    />
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
    <div className="monitor-select-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
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
    <div className="hotkey-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
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
