import { useState, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@/lib/ipc';
import { Button } from '@/components/ui/button';
import {
  Video, Keyboard, X, Minus, Square, Copy, Download, FolderOpen,
  ChevronDown, ChevronLeft, ChevronRight, Check, Monitor, AppWindow,
  Loader2, CircleCheck,
} from 'lucide-react';
import { Hotkey, MonitorInfo } from '@/hooks/useAppHooks';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';
import { RecordingMode } from '@/types/video';
import { useHeaderStatus } from '@/lib/headerStatus';

/** Returns exact integer divisors of Hz that are ≥ 30 (Hz/n for n ∈ {1,2,3,4}). */
function getPerfectFpsOptions(hz: number): number[] {
  if (hz <= 0) return [];
  const options: number[] = [];
  for (let n = 1; n <= 4; n++) {
    const fps = hz / n;
    if (Number.isInteger(fps) && fps >= 30) options.push(fps);
  }
  return options;
}

/** All unique perfect-pacing options across every monitor, sorted ascending. */
function getCombinedFpsOptions(monitors: MonitorInfo[]): number[] {
  const set = new Set<number>();
  for (const m of monitors) {
    for (const fps of getPerfectFpsOptions(m.hz)) set.add(fps);
  }
  return Array.from(set).sort((a, b) => a - b);
}

type CaptureMenuStep = 'root' | 'display-monitors' | 'display-fps' | 'window-fps';

interface HeaderProps {
  isRecording: boolean;
  recordingDuration: number;
  currentVideo: string | null;
  isProcessing: boolean;
  hotkeys: Hotkey[];
  onRemoveHotkey: (index: number) => void;
  onOpenHotkeyDialog: () => void;
  onExport: () => void;
  recordingMode: RecordingMode;
  onRecordingModeChange: (mode: RecordingMode) => void;
  rawButtonLabel: string;
  rawButtonPulse: boolean;
  rawButtonDisabled: boolean;
  onOpenRawVideoDialog: () => void;
  onOpenProjects: () => void;
  onOpenCursorLab: () => void;
  hideExport?: boolean;
  hideRawVideo?: boolean;
  captureSource: 'monitor' | 'window';
  captureFps: number | null;
  monitors: MonitorInfo[];
  onSelectMonitorCapture: (monitorId: string, fps: number | null) => void;
  onSelectWindowCapture: (fps: number | null) => void;
}

export function Header({
  isRecording,
  recordingDuration,
  currentVideo,
  isProcessing,
  hotkeys,
  onRemoveHotkey,
  onOpenHotkeyDialog,
  onExport,
  recordingMode,
  onRecordingModeChange,
  rawButtonLabel,
  rawButtonPulse,
  rawButtonDisabled,
  onOpenRawVideoDialog,
  onOpenProjects,
  onOpenCursorLab,
  hideExport = false,
  hideRawVideo = false,
  captureSource,
  captureFps,
  monitors,
  onSelectMonitorCapture,
  onSelectWindowCapture,
}: HeaderProps) {
  const { t } = useSettings();
  const headerStatus = useHeaderStatus();
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);
  const [isRecordingModeMenuOpen, setIsRecordingModeMenuOpen] = useState(false);
  const [isCaptureSourceMenuOpen, setIsCaptureSourceMenuOpen] = useState(false);
  const [menuStep, setMenuStep] = useState<CaptureMenuStep>('root');
  const [pickedMonitorId, setPickedMonitorId] = useState<string | null>(null);
  const recordingModeMenuRef = useRef<HTMLDivElement | null>(null);
  const captureSourceMenuRef = useRef<HTMLDivElement | null>(null);

  const selectedRecordingModeLabel = useMemo(
    () => recordingMode === 'withCursor' ? t.recordingModeWithCursor : t.recordingModeNoCursor,
    [recordingMode, t.recordingModeWithCursor, t.recordingModeNoCursor]
  );

  const captureSourceLabel = useMemo(() => {
    const sourceLabel = captureSource === 'monitor' ? t.displayCaptureShort : t.windowCapture;
    return captureFps ? `${sourceLabel} · ${captureFps}fps` : sourceLabel;
  }, [captureSource, captureFps, t.displayCaptureShort, t.windowCapture]);

  const combinedFpsOptions = useMemo(() => getCombinedFpsOptions(monitors), [monitors]);

  const pickedMonitor = useMemo(
    () => monitors.find(m => m.id === pickedMonitorId) ?? null,
    [monitors, pickedMonitorId]
  );

  const closeMenu = () => {
    setIsCaptureSourceMenuOpen(false);
    setMenuStep('root');
    setPickedMonitorId(null);
  };

  useEffect(() => {
    invoke<boolean>('is_maximized').then(setIsWindowMaximized).catch(() => {});
  }, []);

  useEffect(() => {
    const onMouseDown = (event: MouseEvent) => {
      if (!recordingModeMenuRef.current?.contains(event.target as Node)) {
        setIsRecordingModeMenuOpen(false);
      }
      if (!captureSourceMenuRef.current?.contains(event.target as Node)) {
        closeMenu();
      }
    };
    window.addEventListener('mousedown', onMouseDown);
    return () => window.removeEventListener('mousedown', onMouseDown);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <header
      className="app-header bg-[var(--surface)] border-b border-[var(--outline-variant)] select-none h-11 flex items-center justify-between cursor-default relative z-[100]"
      onMouseDown={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        const y = e.clientY - rect.top;
        const x = e.clientX - rect.left;
        const w = rect.width;
        if (y <= 5) {
          if (x <= 14) (window as any).ipc.postMessage('resize_nw');
          else if (x >= w - 14) (window as any).ipc.postMessage('resize_ne');
          else (window as any).ipc.postMessage('resize_n');
        } else {
          (window as any).ipc.postMessage('drag_window');
        }
      }}
      onMouseMove={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        const y = e.clientY - rect.top;
        if (y <= 5) {
          const x = e.clientX - rect.left;
          const w = rect.width;
          if (x <= 14) e.currentTarget.style.cursor = 'nwse-resize';
          else if (x >= w - 14) e.currentTarget.style.cursor = 'nesw-resize';
          else e.currentTarget.style.cursor = 'ns-resize';
        } else {
          e.currentTarget.style.cursor = '';
        }
      }}
    >
      <div className="header-left flex items-center gap-4 px-4 h-full">
        <div className="app-branding flex items-center gap-3">
          <Video className="w-5 h-5 text-[var(--primary-color)]" />
          <span className="text-[var(--on-surface)] text-sm font-medium">{t.appTitle}</span>
        </div>

        {isRecording && (
          <div className="recording-status-area h-full flex items-center">
            <div className="recording-indicator flex items-center gap-2 border border-red-500/20 bg-red-500/10 px-3 py-1 rounded-lg backdrop-blur-md animate-in fade-in slide-in-from-left-2 duration-300">
              <div className="recording-dot w-2.5 h-2.5 rounded-full bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.8)]" />
              <span className="text-red-500 text-xs font-bold drop-shadow-sm">{formatTime(recordingDuration)}</span>
            </div>
          </div>
        )}

        {headerStatus && (
          <div
            key={headerStatus.id + headerStatus.type}
            className={`header-status-badge flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-medium animate-in fade-in slide-in-from-left-2 duration-200 ${
              headerStatus.type === 'success'
                ? 'border border-emerald-500/20 bg-emerald-500/10 text-emerald-400'
                : 'border border-[var(--primary-color)]/20 bg-[var(--primary-color)]/10 text-[var(--primary-color)]'
            }`}
          >
            {headerStatus.type === 'success'
              ? <CircleCheck className="w-3 h-3 flex-shrink-0" />
              : <Loader2 className="w-3 h-3 flex-shrink-0 animate-spin" />
            }
            <span className="header-status-message">{(t as Record<string, string>)[headerStatus.messageKey] ?? headerStatus.messageKey}</span>
          </div>
        )}
      </div>

      <div className="header-right flex items-center gap-2 h-full pl-2">
        <div className="hotkey-list flex items-center gap-1.5 flex-wrap max-w-[500px] justify-end">
          {hotkeys.map((h, i) => (
            <Button
              key={i}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={() => onRemoveHotkey(i)}
              className="bg-[var(--surface-container)] hover:bg-[var(--surface-container-high)] text-[var(--on-surface)] px-2 h-6 text-[11px] border border-transparent hover:border-[var(--outline-variant)] flex-shrink-0 transition-colors"
              title={t.clickToRemove}
            >
              <span className="truncate max-w-[80px]">{h.name}</span>
              <X className="w-3 h-3 ml-1 flex-shrink-0" />
            </Button>
          ))}
          <Button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenHotkeyDialog}
            className="bg-transparent border border-[var(--outline-variant)] hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] px-2 h-6 text-[11px] flex-shrink-0 transition-colors whitespace-nowrap"
            title="Add Global Hotkey"
          >
            <Keyboard className="w-3 h-3 mr-1" />
            {t.addHotkey}
          </Button>
          <div
            ref={recordingModeMenuRef}
            className="recording-mode-dropdown relative flex-shrink-0"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <Button
              onClick={() => setIsRecordingModeMenuOpen((open) => {
                if (!open) closeMenu();
                return !open;
              })}
              className="recording-mode-toggle-btn bg-transparent border border-[var(--outline-variant)] hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] px-2 h-6 text-[11px] transition-colors whitespace-nowrap flex items-center"
              title={selectedRecordingModeLabel}
            >
              <span className="recording-mode-toggle-label">{selectedRecordingModeLabel}</span>
              <ChevronDown className="w-3 h-3 ml-1.5" />
            </Button>
            {isRecordingModeMenuOpen && (
              <div className="recording-mode-menu absolute top-[calc(100%+4px)] left-0 min-w-[360px] z-50 rounded-lg border border-[var(--glass-border)] bg-[var(--surface)] shadow-[0_8px_24px_rgba(0,0,0,0.32)] p-1.5">
                {([
                  { mode: 'withoutCursor' as const, label: t.recordingModeNoCursorDetail },
                  { mode: 'withCursor' as const, label: t.recordingModeWithCursorDetail },
                ]).map((option) => {
                  const selected = recordingMode === option.mode;
                  return (
                    <button
                      key={option.mode}
                      type="button"
                      onClick={() => {
                        onRecordingModeChange(option.mode);
                        setIsRecordingModeMenuOpen(false);
                      }}
                      className={`recording-mode-option w-full text-left rounded-md px-2 py-1.5 text-[11px] leading-tight transition-colors flex items-start gap-2 ${
                        selected
                          ? 'bg-[var(--primary-color)]/16 text-[var(--on-surface)]'
                          : 'text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]'
                      }`}
                    >
                      <span className="recording-mode-option-check w-3.5 h-3.5 mt-0.5 flex items-center justify-center">
                        {selected ? <Check className="w-3.5 h-3.5 text-[var(--primary-color)]" /> : null}
                      </span>
                      <span className="recording-mode-option-label">{option.label}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        <div className="header-actions flex items-center gap-2">
          {/* ── Capture-source dropdown (multi-step) ── */}
          <div
            ref={captureSourceMenuRef}
            className="capture-source-dropdown relative flex-shrink-0"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <Button
              disabled={isRecording}
              onClick={() => setIsCaptureSourceMenuOpen((open) => {
                if (!open) { setIsRecordingModeMenuOpen(false); setMenuStep('root'); }
                else closeMenu();
                return !open;
              })}
              className="capture-source-toggle-btn bg-transparent border border-[var(--outline-variant)] hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] px-2 h-6 text-[11px] transition-colors whitespace-nowrap flex items-center disabled:opacity-40 disabled:pointer-events-none"
            >
              <span className="capture-source-toggle-label">{captureSourceLabel}</span>
              <ChevronDown className="w-3 h-3 ml-1.5" />
            </Button>

            {isCaptureSourceMenuOpen && (
              <div className="capture-source-menu absolute top-[calc(100%+4px)] right-0 min-w-[220px] z-50 rounded-lg border border-[var(--glass-border)] bg-[var(--surface)] shadow-[0_8px_24px_rgba(0,0,0,0.32)] p-1.5 overflow-hidden">

                {/* ── Step: root ── */}
                {menuStep === 'root' && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('display-monitors')}
                      className={`capture-source-option-display w-full text-left rounded-md px-2 py-2 text-[11px] transition-colors flex items-center gap-2 ${
                        captureSource === 'monitor'
                          ? 'bg-[var(--primary-color)]/16 text-[var(--on-surface)]'
                          : 'text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]'
                      }`}
                    >
                      <Monitor className="w-3.5 h-3.5 flex-shrink-0" />
                      <span className="flex-1">{t.displayCapture}</span>
                      <ChevronRight className="w-3 h-3 opacity-50" />
                    </button>
                    <button
                      type="button"
                      onClick={() => setMenuStep('window-fps')}
                      className={`capture-source-option-window w-full text-left rounded-md px-2 py-2 text-[11px] transition-colors flex items-center gap-2 mt-0.5 ${
                        captureSource === 'window'
                          ? 'bg-[var(--primary-color)]/16 text-[var(--on-surface)]'
                          : 'text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]'
                      }`}
                    >
                      <AppWindow className="w-3.5 h-3.5 flex-shrink-0" />
                      <span className="flex-1">{t.windowCapture}</span>
                      <ChevronRight className="w-3 h-3 opacity-50" />
                    </button>
                  </>
                )}

                {/* ── Step: display → pick monitor ── */}
                {menuStep === 'display-monitors' && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('root')}
                      className="capture-back-btn w-full text-left rounded-md px-2 py-1.5 text-[11px] text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] transition-colors flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span>{t.displayCapture}</span>
                    </button>
                    <div className="border-t border-[var(--outline-variant)]/50 mb-1.5" />
                    <div className="capture-monitor-list flex flex-col gap-1.5 max-h-[320px] overflow-y-auto">
                      {monitors.length === 0 ? (
                        <div className="px-2 py-3 text-[11px] text-[var(--on-surface-variant)] text-center opacity-60">
                          Loading…
                        </div>
                      ) : monitors.map((m) => (
                        <button
                          key={m.id}
                          type="button"
                          onClick={() => { setPickedMonitorId(m.id); setMenuStep('display-fps'); }}
                          className="capture-monitor-card w-full rounded-md overflow-hidden border border-[var(--outline-variant)]/40 hover:border-[var(--primary-color)]/60 transition-all text-left group"
                        >
                          {m.thumbnail ? (
                            <img
                              src={m.thumbnail}
                              alt={m.name}
                              className="w-full h-[90px] object-cover"
                              draggable={false}
                            />
                          ) : (
                            <div className="capture-monitor-thumb-placeholder w-full h-[90px] bg-[var(--surface-container)] flex items-center justify-center">
                              <Monitor className="w-7 h-7 text-[var(--on-surface-variant)]/30" />
                            </div>
                          )}
                          <div className="capture-monitor-info px-2 py-1.5 bg-[var(--surface-container)]/40 group-hover:bg-[var(--surface-container)] transition-colors">
                            <div className="text-[11px] font-medium text-[var(--on-surface)] leading-tight">{m.name}</div>
                            <div className="text-[10px] text-[var(--on-surface-variant)] opacity-70 mt-0.5">
                              {m.width}×{m.height} · {m.hz}Hz
                              {m.is_primary && <span className="ml-1.5 opacity-60">primary</span>}
                            </div>
                          </div>
                        </button>
                      ))}
                    </div>
                  </>
                )}

                {/* ── Step: display → picked monitor → pick FPS ── */}
                {menuStep === 'display-fps' && pickedMonitor && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('display-monitors')}
                      className="capture-back-btn w-full text-left rounded-md px-2 py-1.5 text-[11px] text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] transition-colors flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span className="truncate">{pickedMonitor.name}</span>
                    </button>
                    <div className="border-t border-[var(--outline-variant)]/50 mb-1" />
                    <div className="capture-fps-section-label px-2 pb-1 text-[10px] uppercase tracking-wide text-[var(--on-surface-variant)] opacity-60">
                      FPS · {pickedMonitor.hz}Hz
                    </div>
                    {/* Auto */}
                    <button
                      type="button"
                      onClick={() => { onSelectMonitorCapture(pickedMonitor.id, null); closeMenu(); }}
                      className="capture-fps-option capture-fps-auto w-full text-left rounded-md px-2 py-1.5 text-[11px] transition-colors flex items-center gap-2 text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]"
                    >
                      <span className="w-3.5 h-3.5" />
                      <span className="flex-1">Auto</span>
                    </button>
                    {getPerfectFpsOptions(pickedMonitor.hz).map((fps) => (
                      <button
                        key={fps}
                        type="button"
                        onClick={() => { onSelectMonitorCapture(pickedMonitor.id, fps); closeMenu(); }}
                        className="capture-fps-option mt-0.5 w-full text-left rounded-md px-2 py-1.5 text-[11px] transition-colors flex items-center gap-2 text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]"
                      >
                        <span className="w-3.5 h-3.5" />
                        <span className="flex-1 font-medium">{fps}fps</span>
                        <span className="text-[10px] opacity-40">÷{pickedMonitor.hz / fps}</span>
                      </button>
                    ))}
                  </>
                )}

                {/* ── Step: window → pick FPS (combined from all monitors) ── */}
                {menuStep === 'window-fps' && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('root')}
                      className="capture-back-btn w-full text-left rounded-md px-2 py-1.5 text-[11px] text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] transition-colors flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span>{t.windowCapture}</span>
                    </button>
                    <div className="border-t border-[var(--outline-variant)]/50 mb-1" />
                    <div className="capture-fps-section-label px-2 pb-1 text-[10px] uppercase tracking-wide text-[var(--on-surface-variant)] opacity-60">
                      FPS
                    </div>
                    <button
                      type="button"
                      onClick={() => { onSelectWindowCapture(null); closeMenu(); }}
                      className="capture-fps-option capture-fps-auto w-full text-left rounded-md px-2 py-1.5 text-[11px] transition-colors flex items-center gap-2 text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]"
                    >
                      <span className="w-3.5 h-3.5" />
                      <span>Auto</span>
                    </button>
                    {combinedFpsOptions.map((fps) => (
                      <button
                        key={fps}
                        type="button"
                        onClick={() => { onSelectWindowCapture(fps); closeMenu(); }}
                        className="capture-fps-option mt-0.5 w-full text-left rounded-md px-2 py-1.5 text-[11px] transition-colors flex items-center gap-2 text-[var(--on-surface-variant)] hover:bg-[var(--surface-container)] hover:text-[var(--on-surface)]"
                      >
                        <span className="w-3.5 h-3.5" />
                        <span className="font-medium">{fps}fps</span>
                      </button>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>

          {currentVideo && !hideRawVideo && (
            <Button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={onOpenRawVideoDialog}
              disabled={rawButtonDisabled}
              className={`raw-video-button h-7 text-[11px] font-medium transition-colors ${
                rawButtonDisabled
                  ? 'bg-[var(--surface-container)]/50 text-[var(--on-surface)]/35 cursor-not-allowed'
                  : 'bg-emerald-500 hover:bg-emerald-500/85 text-white'
              } ${rawButtonPulse && !rawButtonDisabled ? 'animate-pulse' : ''}`}
            >
              {rawButtonLabel}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenCursorLab}
            className="cursor-lab-button h-7 text-[11px] text-[var(--on-surface)] hover:bg-[var(--surface-container)] transition-colors"
          >
            Cursor Lab
          </Button>
          {currentVideo && !hideExport && (
            <Button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={onExport}
              disabled={isProcessing}
              className={`flex items-center px-3 py-1.5 h-7 text-[11px] font-medium transition-colors ${
                isProcessing
                  ? 'bg-[var(--outline-variant)] text-[var(--outline)] cursor-not-allowed'
                  : 'bg-[var(--primary-color)] hover:bg-[var(--primary-color)]/85 text-white'
              }`}
            >
              <Download className="w-3.5 h-3.5 mr-1.5" />{t.export}
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenProjects}
            className="h-7 text-[11px] text-[var(--on-surface)] hover:bg-[var(--surface-container)] transition-colors"
          >
            <FolderOpen className="w-3.5 h-3.5 mr-1.5" />{t.projects}
          </Button>
        </div>

        <div className={`window-controls flex items-center h-full ${isWindowMaximized ? '' : 'ml-4'}`}>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              (window as any).ipc.postMessage('minimize_window');
            }}
            className="window-btn-minimize px-3 h-full text-[var(--on-surface)] hover:bg-[var(--surface-container)] transition-colors flex items-center"
            title={t.minimize}
          >
            <Minus className="w-4 h-4" />
          </button>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={async (e) => {
              e.stopPropagation();
              (window as any).ipc.postMessage('toggle_maximize');
              setTimeout(async () => {
                const maximized = await invoke<boolean>('is_maximized');
                setIsWindowMaximized(maximized);
              }, 50);
            }}
            className="window-btn-maximize px-3 h-full text-[var(--on-surface)] hover:bg-[var(--surface-container)] transition-colors flex items-center"
            title={isWindowMaximized ? t.restore : t.maximize}
          >
            {isWindowMaximized ? <Copy className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
          </button>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              (window as any).ipc.postMessage('close_window');
            }}
            className={`window-btn-close px-3 h-full text-[var(--on-surface)] hover:bg-[var(--tertiary-color)] hover:text-white transition-colors flex items-center ${isWindowMaximized ? 'pr-5' : ''}`}
            title={t.close}
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>
    </header>
  );
}
