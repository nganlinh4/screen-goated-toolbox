import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { X, FolderOpen } from 'lucide-react';
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
import { useSettings } from '@/hooks/useSettings';

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
  const fpsChoiceValues = Array.from(new Set([sourceFpsValue, 24, 30, 60, 90, 120])).sort((a, b) => a - b);
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

  // Find currently selected resolution key, fall back to original (0x0)
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
            <div className="fps-options flex flex-wrap gap-2">
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
