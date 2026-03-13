import { useState, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { FolderOpen } from 'lucide-react';
import { invoke } from '@/lib/ipc';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
} from '@/components/ui/Dialog';
import { ExportOptions, VideoSegment, BackgroundConfig } from '@/types/video';
import {
  computeResolutionOptions,
  computeGifResolutionOptions,
  GIF_MAX_WIDTH,
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
  trimmedDurationSec?: number;
  clipCount?: number;
  autoCopyEnabled: boolean;
  onToggleAutoCopy: (enabled: boolean) => void;
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
  sourceVideoFps,
  trimmedDurationSec,
  clipCount = 1,
  autoCopyEnabled,
  onToggleAutoCopy
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
  const selectedFormat = exportOptions.format === 'gif' ? 'gif' : 'mp4';
  const isGif = selectedFormat === 'gif';
  const fpsChoiceValues = isGif
    ? [10, 15, 24]
    : Array.from(new Set([sourceFpsValue, 24, 30, 60, 90, 120])).sort((a, b) => a - b);
  const resOptions = (
    isGif
      ? computeGifResolutionOptions(baseW, baseH)
      : computeResolutionOptions(baseW, baseH, vidH)
  ).slice().sort((a, b) => b.width - a.width || b.height - a.height);
  const { width: outW, height: outH } = resolveExportDimensions(exportOptions.width, exportOptions.height, baseW, baseH);
  const bitrateBounds = computeBitrateSliderBounds(outW, outH, exportOptions.fps);
  const targetVideoBitrateKbps = exportOptions.targetVideoBitrateKbps > 0
    ? Math.max(bitrateBounds.minKbps, Math.min(exportOptions.targetVideoBitrateKbps, bitrateBounds.maxKbps))
    : bitrateBounds.recommendedKbps;
  const standardBitratePercent = bitrateBounds.maxKbps > bitrateBounds.minKbps
    ? ((bitrateBounds.recommendedKbps - bitrateBounds.minKbps) / (bitrateBounds.maxKbps - bitrateBounds.minKbps)) * 100
    : 0;
  const sourceDuration = videoRef.current?.duration || segment?.trimEnd || 0;
  const resolvedTrimmedDurationSec =
    typeof trimmedDurationSec === 'number'
      ? trimmedDurationSec
      : segment
        ? getTotalTrimDuration(segment, sourceDuration)
        : 0;
  const primaryEstimate = estimateExportSize({
    width: outW,
    height: outH,
    fps: exportOptions.fps,
    format: isGif ? 'gif' : 'mp4',
    targetVideoBitrateKbps,
    trimmedDurationSec: resolvedTrimmedDurationSec,
    hasAudio,
    backgroundConfig,
    segment
  });
  const sizeEstimate = primaryEstimate;
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
        detail: t.exportBackendMfH264Encode,
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

    // Default resolution each time the dialog opens — GIF gets explicit width, MP4 gets "match recorded" (0x0).
    setExportOptions((prev) => {
      const gif = (prev.format || 'mp4') === 'gif';
      if (gif) {
        const gifOptions = computeGifResolutionOptions(baseW, baseH);
        const def = gifOptions[0];
        if (!def || (prev.width === def.width && prev.height === def.height)) return prev;
        return { ...prev, width: def.width, height: def.height };
      }
      if (prev.width === 0 && prev.height === 0) return prev;
      return { ...prev, width: 0, height: 0 };
    });

    // Only auto-match FPS if the user hasn't saved a preference yet.
    const hasSavedFps = (() => {
      try { return localStorage.getItem('screen-record-export-fps-pref-v1') !== null; } catch { return false; }
    })();
    if (!hasSavedFps) {
      autoMatchFpsPendingRef.current = true;
    }
  }, [show, setExportOptions, baseW, baseH]);

  useEffect(() => {
    if (!show || !autoMatchFpsPendingRef.current) return;
    setExportOptions((prev) => {
      // Don't override GIF fps — it has its own limited choices
      if ((prev.format || 'mp4') === 'gif') {
        autoMatchFpsPendingRef.current = false;
        return prev;
      }
      return prev.fps === sourceFpsValue ? prev : { ...prev, fps: sourceFpsValue };
    });
  }, [show, sourceFpsValue, setExportOptions]);

  useEffect(() => {
    if (!show || exportOptions.format !== 'both') return;
    setExportOptions((prev) => ({ ...prev, format: 'mp4' }));
  }, [show, exportOptions.format, setExportOptions]);

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

  // Find currently selected resolution key, fall back to original (0x0)
  const selectedKey = `${exportOptions.width}x${exportOptions.height}`;

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="max-w-[760px]">
        <DialogHeader>
          <DialogTitle>{t.exportOptions}</DialogTitle>
        </DialogHeader>

        <DialogBody className="export-dialog-body pt-4">
          <div className="export-dialog-grid grid gap-4 md:grid-cols-[minmax(0,1.45fr)_minmax(260px,0.95fr)]">
            <div className="export-dialog-main space-y-4">
              {clipCount > 1 && (
                <div className="export-chain-summary ui-inline-note rounded-2xl px-3 py-2 text-xs">
                  {t.exportChainSummary.replace('{count}', String(clipCount)).replace('{duration}', resolvedTrimmedDurationSec.toFixed(1))}
                </div>
              )}

              <div className="export-resolution-field">
                <div className="mb-2 flex items-center justify-between">
                  <label className="text-xs font-medium text-[var(--on-surface-variant)]">{t.resolution}</label>
                </div>
                <div className="resolution-options grid grid-cols-3 gap-2">
                  {resOptions.map((opt: ResolutionOption) => {
                    const key = `${opt.width}x${opt.height}`;
                    const isSourceOption = !isGif && opt.width === sourceResOptionW && opt.height === sourceResOptionH;
                    const isSelected = selectedKey === key || (!isGif && exportOptions.width === 0 && exportOptions.height === 0 && isSourceOption);
                    return (
                      <button
                        key={key}
                        onClick={() => setExportOptions(prev => ({ ...prev, width: isSourceOption ? 0 : opt.width, height: isSourceOption ? 0 : opt.height }))}
                        className={`resolution-option ui-choice-tile min-h-[72px] rounded-2xl px-3 py-2 text-xs font-semibold flex flex-col items-center justify-center gap-0.5 relative ${
                          isSelected
                            ? 'ui-choice-tile-active text-[var(--on-surface)]'
                            : 'text-[var(--on-surface)]'
                        }`}
                      >
                        {isGif ? (
                          <>
                            <span>{opt.width}w</span>
                            <span className="text-[9px] opacity-70 font-mono">{opt.width}×{opt.height}</span>
                          </>
                        ) : (
                          <>
                            <span>{formatResolutionPLabel(isSourceOption ? sourceResOptionH : opt.height)}</span>
                            <span className="text-[9px] opacity-70 font-mono">
                              {isSourceOption ? sourceResLabel : `${opt.width}×${opt.height}`}
                            </span>
                            <span className={`text-[9px] opacity-70 ${isSourceOption ? 'block' : 'invisible'}`}>
                              {isSourceOption ? t.matchRecorded : '.'}
                            </span>
                          </>
                        )}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="export-compact-options grid gap-4 sm:grid-cols-2">
                <div className="export-format-field">
                  <div className="mb-2 flex items-center justify-between">
                    <label className="text-xs font-medium text-[var(--on-surface-variant)]">{t.exportFormat}</label>
                  </div>
                  <div className="format-options flex gap-2">
                    {(['mp4', 'gif'] as const).map(fmt => (
                      <button
                        key={fmt}
                        onClick={() => setExportOptions(prev => {
                          if (fmt === 'gif') {
                            const fps = prev.fps > 24 ? 24 : prev.fps;
                            const gifOptions = computeGifResolutionOptions(baseW, baseH);
                            const def = gifOptions[0];
                            return { ...prev, format: fmt, fps, width: def?.width ?? GIF_MAX_WIDTH, height: def?.height ?? 540 };
                          }
                          return { ...prev, format: fmt, width: 0, height: 0 };
                        })}
                        className={`format-option ui-chip-button flex-1 rounded-xl py-2 text-xs font-medium ${
                          selectedFormat === fmt
                            ? 'ui-chip-button-active'
                            : ''
                        }`}
                      >
                        {fmt === 'mp4' ? t.exportFormatMp4 : t.exportFormatGif}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="export-fps-field">
                  <div className="mb-2 flex items-center justify-between">
                    <label className="text-xs font-medium text-[var(--on-surface-variant)]">{t.frameRate}</label>
                  </div>
                  <div className="fps-options grid grid-cols-3 gap-2">
                    {fpsChoiceValues.map((fps) => {
                      const isSourceOption = fps === sourceFpsValue;
                      return (
                        <button
                          key={fps}
                          onClick={() => {
                            autoMatchFpsPendingRef.current = false;
                            setExportOptions(prev => ({ ...prev, fps }));
                            try { localStorage.setItem('screen-record-export-fps-pref-v1', String(fps)); } catch { /* ignore */ }
                          }}
                          className={`fps-option ui-chip-button rounded-xl py-2 text-xs font-medium ${
                            exportOptions.fps === fps
                              ? 'ui-chip-button-active'
                              : ''
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
              </div>

              {!isGif && <div className="export-bitrate-field">
                <label className="mb-2 block text-xs text-[var(--on-surface-variant)]">{t.videoBitrate}</label>
                <div className="bitrate-control ui-surface rounded-2xl p-3">
                  <div className="bitrate-display mb-3 flex items-center justify-between gap-3">
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
              </div>}
            </div>

            <div className="export-dialog-side space-y-4">
              <div className="export-summary-card ui-surface rounded-2xl p-4">
                <div className="mb-3 flex items-start justify-between gap-3">
                  <div>
                    <div className="text-[10px] uppercase tracking-[0.14em] text-[var(--on-surface-variant)]/70">{t.estimatedSize}</div>
                    <div className="mt-1 text-2xl font-semibold text-[var(--on-surface)] tabular-nums">
                      ~{formatDataSize(sizeEstimate.estimatedBytes)}
                    </div>
                  </div>
                  <div className="text-right text-[11px] text-[var(--on-surface-variant)]">
                    <div>{formatResolutionPLabel(outH)}</div>
                    <div>{exportOptions.fps} fps</div>
                  </div>
                </div>
                <div className="grid gap-2 text-[11px] text-[var(--on-surface-variant)]">
                  <div className="flex items-center justify-between gap-3">
                    <span>{t.duration}</span>
                    <span className="tabular-nums text-[var(--on-surface)]">{resolvedTrimmedDurationSec.toFixed(1)}s</span>
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span>{t.exportFormat}</span>
                    <span className="text-[var(--on-surface)]">{selectedFormat === 'mp4' ? t.exportFormatMp4 : t.exportFormatGif}</span>
                  </div>
                  {clipCount > 1 && (
                    <div className="flex items-center justify-between gap-3">
                      <span>{t.projects}</span>
                      <span className="text-[var(--on-surface)]">{clipCount}</span>
                    </div>
                  )}
                </div>
                <div className="mt-4 border-t border-[var(--ui-border)] pt-3">
                  <div className="mb-1 text-[10px] uppercase tracking-[0.14em] text-[var(--on-surface-variant)]/70">{t.backendExport}</div>
                  <div className={`text-[11px] font-medium ${backendStatus.tone}`}>
                    {backendStatus.label}
                  </div>
                  {backendStatus.detail ? (
                    <div className="mt-1 text-[10px] text-[var(--on-surface-variant)]">
                      {backendStatus.detail}
                    </div>
                  ) : null}
                </div>
              </div>

              <div className="export-location-card ui-surface rounded-2xl p-4">
                <label className="mb-2 block text-xs text-[var(--on-surface-variant)]">{t.saveLocation}</label>
                <div className="export-location-stack space-y-2">
                  <div
                    className="export-location-value ui-input thin-scrollbar w-full overflow-x-auto whitespace-nowrap rounded-xl px-3 py-2 text-xs text-[var(--on-surface)]"
                    title={exportOptions.outputDir || ''}
                  >
                    {exportOptions.outputDir || '-'}
                  </div>
                  <div className="export-location-actions flex justify-end">
                    <Button
                      variant="outline"
                      onClick={handleBrowseOutputDir}
                      disabled={isPickingDir}
                      className="h-9 rounded-xl text-xs"
                    >
                      <FolderOpen className="w-3.5 h-3.5 mr-1.5" />
                      {isPickingDir ? t.browsing : t.browse}
                    </Button>
                  </div>
                </div>
                <label className="export-auto-copy-toggle mt-3 flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
                  <Checkbox
                    checked={autoCopyEnabled}
                    onChange={(e) => onToggleAutoCopy(e.target.checked)}
                  />
                  <span>{isGif ? t.autoCopyGifAfterExport : t.autoCopyVideoAfterExport}</span>
                </label>
              </div>
            </div>
          </div>
        </DialogBody>

        <DialogFooter>
          <Button variant="outline" onClick={onClose} className="h-8 rounded-lg text-xs">{t.cancel}</Button>
          <Button
            onClick={onExport}
            className="export-confirm-btn ui-action-button rounded-lg text-xs h-8"
            data-tone="primary"
            data-active="true"
            data-emphasis="strong"
          >
            {t.exportVideo}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
