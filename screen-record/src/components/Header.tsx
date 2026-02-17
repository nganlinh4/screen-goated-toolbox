import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, X, Minus, Square, Copy, Download, FolderOpen } from 'lucide-react';
import { Hotkey } from '@/hooks/useAppHooks';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';

interface HeaderProps {
  isRecording: boolean;
  recordingDuration: number;
  currentVideo: string | null;
  isProcessing: boolean;
  hotkeys: Hotkey[];
  onRemoveHotkey: (index: number) => void;
  onOpenHotkeyDialog: () => void;
  onExport: () => void;
  onOpenProjects: () => void;
  onOpenCursorLab: () => void;
  hideExport?: boolean;
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
  onOpenProjects,
  onOpenCursorLab,
  hideExport = false
}: HeaderProps) {
  const { t } = useSettings();
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);

  useEffect(() => {
    invoke<boolean>('is_maximized').then(setIsWindowMaximized).catch(() => {});
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
        </div>

        <div className="header-actions flex items-center gap-2">
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
