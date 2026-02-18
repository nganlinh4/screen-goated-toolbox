import { useState, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, X, Minus, Square, Copy, Download, FolderOpen, ChevronDown, Check } from 'lucide-react';
import { Hotkey } from '@/hooks/useAppHooks';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';
import { RecordingMode } from '@/types/video';

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
  hideExport?: boolean;
  hideRawVideo?: boolean;
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
  hideExport = false,
  hideRawVideo = false
}: HeaderProps) {
  const { t } = useSettings();
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);
  const [isRecordingModeMenuOpen, setIsRecordingModeMenuOpen] = useState(false);
  const recordingModeMenuRef = useRef<HTMLDivElement | null>(null);
  const selectedRecordingModeLabel = useMemo(
    () => recordingMode === 'withCursor' ? t.recordingModeWithCursor : t.recordingModeNoCursor,
    [recordingMode, t.recordingModeWithCursor, t.recordingModeNoCursor]
  );

  useEffect(() => {
    invoke<boolean>('is_maximized').then(setIsWindowMaximized).catch(() => {});
  }, []);

  useEffect(() => {
    const onMouseDown = (event: MouseEvent) => {
      if (!recordingModeMenuRef.current?.contains(event.target as Node)) {
        setIsRecordingModeMenuOpen(false);
      }
    };
    window.addEventListener('mousedown', onMouseDown);
    return () => window.removeEventListener('mousedown', onMouseDown);
  }, []);

  return (
    <header
      className="app-header bg-[var(--surface)] border-b border-[var(--outline-variant)] select-none h-11 flex items-center justify-between cursor-default relative z-[60]"
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

        <div className="recording-status-area h-full flex items-center">
          {isRecording && currentVideo && (
            <div className="recording-indicator flex items-center gap-2 bg-[var(--tertiary-color)]/10 border border-[var(--tertiary-color)]/30 px-2.5 py-1 rounded-lg backdrop-blur-sm animate-in fade-in slide-in-from-left-2 duration-300">
              <div className="recording-dot w-2 h-2 rounded-full bg-[var(--tertiary-color)] animate-pulse" />
              <span className="text-[var(--tertiary-color)] text-[10px] font-bold uppercase tracking-wider">{t.rec}</span>
              <span className="text-[var(--on-surface)] text-xs font-mono">{formatTime(recordingDuration)}</span>
            </div>
          )}
        </div>
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
              onClick={() => setIsRecordingModeMenuOpen((open) => !open)}
              className="recording-mode-toggle-btn bg-transparent border border-[var(--outline-variant)] hover:bg-[var(--surface-container)] text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] px-2 h-6 text-[11px] transition-colors whitespace-nowrap flex items-center"
              title={selectedRecordingModeLabel}
            >
              <span className="recording-mode-toggle-label">{selectedRecordingModeLabel}</span>
              <ChevronDown className="w-3 h-3 ml-1.5" />
            </Button>
            {isRecordingModeMenuOpen && (
              <div className="recording-mode-menu absolute top-[calc(100%+4px)] left-0 min-w-[360px] z-50 rounded-lg border border-[var(--glass-border)] bg-[var(--surface)] shadow-[0_8px_24px_rgba(0,0,0,0.32)] p-1.5">
                {([
                  {
                    mode: 'withoutCursor' as const,
                    label: t.recordingModeNoCursorDetail,
                  },
                  {
                    mode: 'withCursor' as const,
                    label: t.recordingModeWithCursorDetail,
                  },
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
          {/* Cursor Lab hidden for now.
          <Button
            variant="ghost"
            size="sm"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenCursorLab}
            className="cursor-lab-button h-7 text-[11px] text-[var(--on-surface)] hover:bg-[var(--surface-container)] transition-colors"
          >
            Cursor Lab
          </Button>
          */}
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
