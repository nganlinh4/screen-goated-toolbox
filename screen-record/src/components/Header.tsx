import { useState, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@/lib/ipc';
import { Button } from '@/components/ui/button';
import {
  Keyboard, X, Download, FolderOpen,
  ChevronDown, ChevronLeft, ChevronRight, Monitor, AppWindow,
  Loader2, CircleCheck,
} from '@/components/ui/MaterialIcon';
import { Hotkey, MonitorInfo } from '@/hooks/useAppHooks';
import { formatMonitorSummary, formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';
import { RecordingMode } from '@/types/video';
import { RecordingAudioSelection } from '@/types/recordingAudio';
import { useHeaderStatus } from '@/lib/headerStatus';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from '@/components/ui/DropdownMenu';
import { Tooltip } from '@/components/ui/Tooltip';
import { getCombinedFpsOptions, getPerfectFpsOptions } from '@/components/header/captureFpsOptions';
import { HeaderWindowControls } from '@/components/header/HeaderWindowControls';
import { RecordingAudioSourceDropdown } from '@/components/header/RecordingAudioSourceDropdown';

type CaptureMenuStep = 'root' | 'display-monitors' | 'display-fps' | 'window-fps';
type HeaderDropdown = 'recordingAudio' | 'recordingMode' | 'captureSource' | null;

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
  recordingAudioSelection: RecordingAudioSelection;
  isSelectingRecordingAudioApp: boolean;
  onToggleRecordingDeviceAudio: (enabled: boolean) => void;
  onToggleRecordingMicAudio: (enabled: boolean) => void;
  onSelectAllRecordingDeviceAudio: () => void;
  onRequestRecordingAudioAppSelection: () => void;
  rawButtonLabel: string;
  rawButtonPulse: boolean;
  rawButtonDisabled: boolean;
  onOpenRawVideoDialog: () => void;
  onOpenProjects: () => void;
  projectsButtonDisabled?: boolean;
  hideExport?: boolean;
  hideRawVideo?: boolean;
  captureSource: 'monitor' | 'window';
  captureFps: number | null;
  monitors: MonitorInfo[];
  onSelectMonitorCapture: (monitorId: string, fps: number | null) => void;
  onSelectWindowCapture: (fps: number | null) => void;
  sequenceBreadcrumb?: React.ReactNode;
  showProjectsDialog?: boolean;
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
  recordingAudioSelection,
  isSelectingRecordingAudioApp,
  onToggleRecordingDeviceAudio,
  onToggleRecordingMicAudio,
  onSelectAllRecordingDeviceAudio,
  onRequestRecordingAudioAppSelection,
  rawButtonLabel,
  rawButtonPulse,
  rawButtonDisabled,
  onOpenRawVideoDialog,
  onOpenProjects,
  projectsButtonDisabled = false,
  hideExport = false,
  hideRawVideo = false,
  captureSource,
  captureFps,
  monitors,
  onSelectMonitorCapture,
  onSelectWindowCapture,
  sequenceBreadcrumb,
  showProjectsDialog,
}: HeaderProps) {
  const { t } = useSettings();
  const headerStatus = useHeaderStatus();
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);
  const [activeDropdown, setActiveDropdown] = useState<HeaderDropdown>(null);
  const [menuStep, setMenuStep] = useState<CaptureMenuStep>('root');
  const [pickedMonitorId, setPickedMonitorId] = useState<string | null>(null);
  const pendingDropdownRef = useRef<Exclude<HeaderDropdown, null> | null>(null);
  const handoffTimerRef = useRef<number | null>(null);
  const suppressedCloseRef = useRef<HeaderDropdown>(null);

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

  const isRecordingAudioMenuOpen = activeDropdown === 'recordingAudio';
  const isRecordingModeMenuOpen = activeDropdown === 'recordingMode';
  const isCaptureSourceMenuOpen = activeDropdown === 'captureSource';

  const resetCaptureMenu = () => {
    setMenuStep('root');
    setPickedMonitorId(null);
  };

  const closeMenu = () => {
    resetCaptureMenu();
    setActiveDropdown((prev) => (prev === 'captureSource' ? null : prev));
  };

  const clearPendingHandoff = () => {
    if (handoffTimerRef.current !== null) {
      window.clearTimeout(handoffTimerRef.current);
      handoffTimerRef.current = null;
    }
  };

  const handleDropdownTriggerPointerDown = (
    target: Exclude<HeaderDropdown, null>,
  ) => (e: React.PointerEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (activeDropdown === target) return;
    if (activeDropdown && activeDropdown !== target) {
      pendingDropdownRef.current = target;
      clearPendingHandoff();
      setActiveDropdown(null);
      handoffTimerRef.current = window.setTimeout(() => {
        handoffTimerRef.current = null;
        const pendingTarget = pendingDropdownRef.current;
        if (!pendingTarget) {
          return;
        }
        pendingDropdownRef.current = null;
        if (pendingTarget === 'captureSource') {
          resetCaptureMenu();
        }
        suppressedCloseRef.current = pendingTarget;
        window.setTimeout(() => {
          if (suppressedCloseRef.current === pendingTarget) {
            suppressedCloseRef.current = null;
          }
        }, 0);
        setActiveDropdown(pendingTarget);
      }, 0);
      e.preventDefault();
    }
  };

  useEffect(() => {
    invoke<boolean>('is_maximized').then(setIsWindowMaximized).catch(() => {});
  }, []);

  useEffect(() => () => clearPendingHandoff(), []);

  return (
    <header
      className="app-header bg-[var(--surface)] border-b border-[var(--outline-variant)] select-none h-11 flex items-center justify-between cursor-default relative z-[20]"
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
      <div className="header-left flex items-center gap-2 px-4 h-full min-w-0 flex-1">
        {isRecording && (
          <div className="recording-status-area h-full flex items-center shrink-0">
            <div className="recording-indicator flex items-center gap-2 border border-red-500/25 bg-red-500/14 px-3 py-1 rounded-lg animate-in fade-in slide-in-from-left-2 duration-300">
              <div className="recording-dot w-2.5 h-2.5 rounded-full bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.8)]" />
              <span className="text-red-500 text-xs font-bold drop-shadow-sm">{formatTime(recordingDuration)}</span>
            </div>
          </div>
        )}

        {headerStatus && (
          <div
            key={headerStatus.id + headerStatus.type}
            className={`header-status-badge flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-medium shrink-0 animate-in fade-in slide-in-from-left-2 duration-200 ${
              headerStatus.type === 'success'
                ? 'border border-emerald-500/20 bg-emerald-500/10 text-emerald-400'
                : 'border border-[var(--primary-color)]/20 bg-[var(--primary-color)]/10 text-[var(--primary-color)]'
            }`}
          >
            {headerStatus.type === 'success'
              ? <CircleCheck className="w-3 h-3 shrink-0" />
              : <Loader2 className="w-3 h-3 shrink-0 animate-spin" />
            }
            <span className="header-status-message">{(t as Record<string, string>)[headerStatus.messageKey] ?? headerStatus.messageKey}</span>
          </div>
        )}

        {sequenceBreadcrumb && (
          <div className={`header-breadcrumb-slot flex items-center transition-opacity duration-200 ${showProjectsDialog ? 'opacity-0 pointer-events-none' : 'opacity-100'}`} style={{ width: 'min(var(--breadcrumb-content-w, 360px), 70%)' }} onMouseDown={(e) => e.stopPropagation()}>
            {sequenceBreadcrumb}
          </div>
        )}
      </div>

      <div className="header-right flex items-center gap-2 h-full pl-2 shrink-0">
        <div className="hotkey-list flex items-center gap-1.5 flex-wrap max-w-[500px] justify-end">
          {hotkeys.map((h, i) => (
            <Button
              key={i}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={() => onRemoveHotkey(i)}
              className="hotkey-chip ui-chip-button px-2 h-6 text-[11px] shrink-0"
              title={t.clickToRemove}
            >
              <span className="truncate max-w-[80px]">{h.name}</span>
              <X className="w-3 h-3 ml-1 shrink-0" />
            </Button>
          ))}
          <Button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenHotkeyDialog}
            className="add-hotkey-btn ui-toolbar-button px-2 h-6 text-[11px] shrink-0 whitespace-nowrap"
            title={t.addHotkey}
          >
            <Keyboard className="w-3 h-3 mr-1" />
            {t.addHotkey}
          </Button>
        </div>

        <div className="header-actions flex items-center gap-2">
          <RecordingAudioSourceDropdown
            open={isRecordingAudioMenuOpen}
            onOpenChange={(open) => {
              if (open) {
                pendingDropdownRef.current = null;
                clearPendingHandoff();
                resetCaptureMenu();
                setActiveDropdown('recordingAudio');
                return;
              }
              if (suppressedCloseRef.current === 'recordingAudio') {
                suppressedCloseRef.current = null;
                return;
              }
              if (pendingDropdownRef.current) return;
              setActiveDropdown((prev) => (prev === 'recordingAudio' ? null : prev));
            }}
            onTriggerPointerDown={handleDropdownTriggerPointerDown('recordingAudio')}
            selection={recordingAudioSelection}
            isSelectingApp={isSelectingRecordingAudioApp}
            onToggleDevice={onToggleRecordingDeviceAudio}
            onToggleMic={onToggleRecordingMicAudio}
            onSelectAllDeviceAudio={onSelectAllRecordingDeviceAudio}
            onRequestAppSelection={() => {
              onRequestRecordingAudioAppSelection();
              setActiveDropdown(null);
            }}
            t={t}
          />
          <div className="recording-mode-dropdown relative shrink-0" onMouseDown={(e) => e.stopPropagation()}>
            <DropdownMenu open={isRecordingModeMenuOpen} onOpenChange={(open) => {
              if (open) {
                pendingDropdownRef.current = null;
                clearPendingHandoff();
                resetCaptureMenu();
                setActiveDropdown('recordingMode');
                return;
              }
              if (suppressedCloseRef.current === 'recordingMode') {
                suppressedCloseRef.current = null;
                return;
              }
              if (pendingDropdownRef.current) return;
              setActiveDropdown((prev) => (prev === 'recordingMode' ? null : prev));
            }}>
              <DropdownMenuTrigger asChild>
                <Button
                  onPointerDown={handleDropdownTriggerPointerDown('recordingMode')}
                  className="recording-mode-toggle-btn ui-toolbar-button px-2 h-6 text-[11px] whitespace-nowrap flex items-center"
                  title={selectedRecordingModeLabel}
                >
                  <span className="recording-mode-toggle-label">{selectedRecordingModeLabel}</span>
                  <ChevronDown className="w-3 h-3 ml-1.5" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="min-w-[360px]">
                {([
                  { mode: 'withoutCursor' as const, label: t.recordingModeNoCursorDetail },
                  { mode: 'withCursor' as const, label: t.recordingModeWithCursorDetail },
                ]).map((option) => (
                  <DropdownMenuItem
                    key={option.mode}
                    selected={recordingMode === option.mode}
                    onSelect={() => {
                      onRecordingModeChange(option.mode);
                      setActiveDropdown(null);
                    }}
                    className="recording-mode-option items-start"
                  >
                    <span className="recording-mode-option-label">{option.label}</span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <div className="capture-source-dropdown relative shrink-0" onMouseDown={(e) => e.stopPropagation()}>
            <DropdownMenu open={isCaptureSourceMenuOpen} onOpenChange={(open) => {
              if (open) {
                pendingDropdownRef.current = null;
                clearPendingHandoff();
                resetCaptureMenu();
                setActiveDropdown('captureSource');
                return;
              }
              if (suppressedCloseRef.current === 'captureSource') {
                suppressedCloseRef.current = null;
                return;
              }
              if (pendingDropdownRef.current) return;
              closeMenu();
            }}>
              <DropdownMenuTrigger asChild>
                <Button
                  onPointerDown={handleDropdownTriggerPointerDown('captureSource')}
                  className="capture-source-toggle-btn ui-toolbar-button px-2 h-6 text-[11px] whitespace-nowrap flex items-center"
                >
                  <span className="capture-source-toggle-label">{captureSourceLabel}</span>
                  <ChevronDown className="w-3 h-3 ml-1.5" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-[220px]" onCloseAutoFocus={(e) => e.preventDefault()}>
                {menuStep === 'root' && (
                  <>
                    <DropdownMenuItem
                      selected={captureSource === 'monitor'}
                      onSelect={(e) => { e.preventDefault(); setMenuStep('display-monitors'); }}
                      className="capture-source-option-display py-2"
                    >
                      <Monitor className="w-3.5 h-3.5 shrink-0 mr-2" />
                      <span className="flex-1">{t.displayCapture}</span>
                      <ChevronRight className="w-3 h-3 opacity-50" />
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      selected={captureSource === 'window'}
                      onSelect={(e) => { e.preventDefault(); setMenuStep('window-fps'); }}
                      className="capture-source-option-window py-2 mt-0.5"
                    >
                      <AppWindow className="w-3.5 h-3.5 shrink-0 mr-2" />
                      <span className="flex-1">{t.windowCapture}</span>
                      <ChevronRight className="w-3 h-3 opacity-50" />
                    </DropdownMenuItem>
                  </>
                )}

                {menuStep === 'display-monitors' && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('root')}
                      className="capture-back-btn ui-toolbar-button w-full text-left rounded-md px-2 py-1.5 text-[11px] flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span>{t.displayCapture}</span>
                    </button>
                    <DropdownMenuSeparator />
                    <div className="capture-monitor-list flex flex-col gap-1.5 max-h-[320px] overflow-y-auto">
                      {monitors.length === 0 ? (
                        <div className="px-2 py-3 text-[11px] text-[var(--on-surface-variant)] text-center opacity-60">
                          {t.loading}
                        </div>
                      ) : monitors.map((m) => (
                        <button
                          key={m.id}
                          type="button"
                          onClick={() => { setPickedMonitorId(m.id); setMenuStep('display-fps'); }}
                          className="capture-monitor-card ui-choice-tile w-full rounded-md overflow-hidden text-left group"
                        >
                          {m.thumbnail ? (
                            <img src={m.thumbnail} alt={m.name} className="w-full h-[90px] object-cover" draggable={false} />
                          ) : (
                            <div className="capture-monitor-thumb-placeholder w-full h-[90px] bg-[var(--ui-surface-1)] flex items-center justify-center">
                              <Monitor className="w-7 h-7 text-[var(--on-surface-variant)]/30" />
                            </div>
                          )}
                          <div className="capture-monitor-info px-2 py-1.5 bg-[var(--ui-surface-2)]/80 group-hover:bg-[var(--ui-hover)] transition-colors">
                            <div className="text-[11px] font-medium text-[var(--on-surface)] leading-tight">{m.name}</div>
                            <div className="text-[10px] text-[var(--on-surface-variant)] opacity-70 mt-0.5">
                              {formatMonitorSummary(m, t)}
                            </div>
                          </div>
                        </button>
                      ))}
                    </div>
                  </>
                )}

                {menuStep === 'display-fps' && pickedMonitor && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('display-monitors')}
                      className="capture-back-btn ui-toolbar-button w-full text-left rounded-md px-2 py-1.5 text-[11px] flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span className="truncate">{pickedMonitor.name}</span>
                    </button>
                    <DropdownMenuSeparator />
                    <DropdownMenuLabel>FPS · {pickedMonitor.hz}Hz</DropdownMenuLabel>
                    <DropdownMenuItem
                      onSelect={() => { onSelectMonitorCapture(pickedMonitor.id, null); closeMenu(); }}
                      className="capture-fps-option capture-fps-auto"
                    >
                      <span className="w-3.5 h-3.5 mr-2" />
                      <span className="flex-1">{t.autoOption}</span>
                    </DropdownMenuItem>
                    {getPerfectFpsOptions(pickedMonitor.hz).map((fps) => (
                      <DropdownMenuItem
                        key={fps}
                        onSelect={() => { onSelectMonitorCapture(pickedMonitor.id, fps); closeMenu(); }}
                        className="capture-fps-option mt-0.5"
                      >
                        <span className="w-3.5 h-3.5 mr-2" />
                        <span className="flex-1 font-medium">{fps}fps</span>
                        <span className="text-[10px] opacity-40">÷{pickedMonitor.hz / fps}</span>
                      </DropdownMenuItem>
                    ))}
                  </>
                )}

                {menuStep === 'window-fps' && (
                  <>
                    <button
                      type="button"
                      onClick={() => setMenuStep('root')}
                      className="capture-back-btn ui-toolbar-button w-full text-left rounded-md px-2 py-1.5 text-[11px] flex items-center gap-1.5 mb-1"
                    >
                      <ChevronLeft className="w-3.5 h-3.5" />
                      <span>{t.windowCapture}</span>
                    </button>
                    <DropdownMenuSeparator />
                    <DropdownMenuLabel>FPS</DropdownMenuLabel>
                    <DropdownMenuItem
                      onSelect={() => { onSelectWindowCapture(null); closeMenu(); }}
                      className="capture-fps-option capture-fps-auto"
                    >
                      <span className="w-3.5 h-3.5 mr-2" />
                      <span>{t.autoOption}</span>
                    </DropdownMenuItem>
                    {combinedFpsOptions.map((fps) => (
                      <DropdownMenuItem
                        key={fps}
                        onSelect={() => { onSelectWindowCapture(fps); closeMenu(); }}
                        className="capture-fps-option mt-0.5"
                      >
                        <span className="w-3.5 h-3.5 mr-2" />
                        <span className="font-medium">{fps}fps</span>
                      </DropdownMenuItem>
                    ))}
                  </>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {currentVideo && !hideRawVideo && (
            <Button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={onOpenRawVideoDialog}
              disabled={rawButtonDisabled}
                className={`raw-video-button ui-action-button h-7 text-[11px] font-medium transition-colors ${
                  rawButtonDisabled
                  ? 'ui-toolbar-button text-[var(--on-surface)]/35 cursor-not-allowed'
                  : ''
              } ${rawButtonPulse && !rawButtonDisabled ? 'raw-video-button-pulse' : ''}`}
              data-tone="success"
              data-active={rawButtonDisabled ? "false" : "true"}
              data-emphasis={rawButtonDisabled ? undefined : "strong"}
            >
              {rawButtonLabel}
            </Button>
          )}
          {currentVideo && !hideExport && (
            <Tooltip content={t.export} side="bottom">
              <Button
                onMouseDown={(e) => e.stopPropagation()}
                onClick={onExport}
                disabled={isProcessing}
                className={`header-export-button ui-action-button flex items-center px-3 py-1.5 h-7 text-[11px] font-medium transition-all ${
                  isProcessing
                    ? 'bg-[var(--outline-variant)] text-[var(--outline)] cursor-not-allowed'
                    : ''
                }`}
                data-tone="primary"
                data-active={isProcessing ? "false" : "true"}
                data-emphasis={isProcessing ? undefined : "strong"}
              >
                <Download className="w-3.5 h-3.5 mr-1.5" />{t.export}
              </Button>
            </Tooltip>
          )}
          <Button
            variant="ghost"
            size="sm"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenProjects}
            disabled={projectsButtonDisabled}
            className="projects-button ui-toolbar-button h-7 text-[11px]"
            data-disabled={projectsButtonDisabled ? "true" : undefined}
          >
            <FolderOpen className="w-3.5 h-3.5 mr-1.5" />{t.projects}
          </Button>
        </div>

        <HeaderWindowControls
          isWindowMaximized={isWindowMaximized}
          setIsWindowMaximized={setIsWindowMaximized}
          t={t}
        />
      </div>
    </header>
  );
}
