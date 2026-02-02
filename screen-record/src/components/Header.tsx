import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, X, Minus, Square, Copy, Download, FolderOpen } from 'lucide-react';
import { Hotkey } from '@/hooks/useAppHooks';
import { formatTime } from '@/utils/helpers';

interface HeaderProps {
  isRecording: boolean;
  recordingDuration: number;
  currentVideo: string | null;
  isProcessing: boolean;
  hotkeys: Hotkey[];
  keyvizStatus: { installed: boolean; enabled: boolean };
  onRemoveHotkey: (index: number) => void;
  onOpenHotkeyDialog: () => void;
  onToggleKeyviz: () => void;
  onExport: () => void;
  onOpenProjects: () => void;
}

export function Header({
  isRecording,
  recordingDuration,
  currentVideo,
  isProcessing,
  hotkeys,
  keyvizStatus,
  onRemoveHotkey,
  onOpenHotkeyDialog,
  onToggleKeyviz,
  onExport,
  onOpenProjects
}: HeaderProps) {
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);

  useEffect(() => {
    invoke<boolean>('is_maximized').then(setIsWindowMaximized).catch(() => {});
  }, []);

  return (
    <header
      className="bg-[#1a1a1b] border-b border-[#343536] select-none h-11 flex items-center justify-between cursor-default"
      onMouseDown={() => {
        (window as any).ipc.postMessage('drag_window');
      }}
    >
      <div className="flex items-center gap-4 px-4 h-full">
        <div className="flex items-center gap-3">
          <Video className="w-5 h-5 text-[#0079d3]" />
          <span className="text-[#d7dadc] text-sm font-medium">Screen Record</span>
        </div>

        <div className="h-full flex items-center">
          {isRecording && currentVideo && (
            <div className="flex items-center gap-3 bg-red-500/10 border border-red-500/30 px-3 py-1 rounded-full animate-in fade-in slide-in-from-left-2 duration-300">
              <div className="w-2 h-2 rounded-full bg-red-500 animate-pulse" />
              <div className="flex flex-col">
                <span className="text-red-500 text-[10px] font-bold leading-none uppercase tracking-wider">Recording</span>
                <span className="text-[#818384] text-[9px] leading-tight">Screen is being captured</span>
              </div>
              <span className="text-[#d7dadc] text-xs font-mono ml-1">{formatTime(recordingDuration)}</span>
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center gap-3 h-full px-2">
        <div className="flex items-center gap-2 flex-wrap max-w-[400px] justify-end">
          {hotkeys.map((h, i) => (
            <Button
              key={i}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={() => onRemoveHotkey(i)}
              className="bg-[#272729] hover:bg-red-500/20 text-[#d7dadc] hover:text-red-400 px-2 h-7 text-xs border border-transparent hover:border-red-500/30 flex-shrink-0"
              title="Click to remove"
            >
              <span className="truncate max-w-[80px]">{h.name}</span>
              <X className="w-3 h-3 ml-1 flex-shrink-0" />
            </Button>
          ))}
          <Button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenHotkeyDialog}
            className="bg-[#0079d3] hover:bg-[#0079d3]/90 text-white px-2 h-7 text-xs flex-shrink-0"
            title="Add Global Hotkey"
          >
            <Keyboard className="w-3 h-3 mr-1" />
            Add Hotkey
          </Button>
          <Button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onToggleKeyviz}
            className={`px-2 h-7 text-xs flex-shrink-0 transition-colors ${
              keyvizStatus.enabled
                ? 'bg-green-600 hover:bg-green-700 text-white'
                : 'bg-[#272729] hover:bg-[#343536] text-[#d7dadc]'
            }`}
            title={keyvizStatus.installed ? "Toggle Keyviz" : "Install & Enable Keyviz"}
          >
            <Keyboard className="w-3 h-3 mr-1" />
            {keyvizStatus.enabled ? "Keystrokes: ON" : "Show Keystrokes"}
          </Button>
        </div>

        <div className="flex items-center gap-2">
          {currentVideo && (
            <Button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={onExport}
              disabled={isProcessing}
              className={`flex items-center px-4 py-2 h-8 text-xs font-medium ${
                isProcessing
                  ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
                  : 'bg-[#9C17FF] hover:bg-[#9C17FF]/90 text-white'
              }`}
            >
              <Download className="w-4 h-4 mr-2" />Export
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onOpenProjects}
            className="h-8 text-xs text-[#d7dadc] hover:bg-[#272729]"
          >
            <FolderOpen className="w-4 h-4 mr-2" />Projects
          </Button>
        </div>

        <div className="flex items-center h-full ml-4">
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              (window as any).ipc.postMessage('minimize_window');
            }}
            className="px-3 h-full text-[#d7dadc] hover:bg-[#272729] transition-colors flex items-center"
            title="Minimize"
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
            className="px-3 h-full text-[#d7dadc] hover:bg-[#272729] transition-colors flex items-center"
            title={isWindowMaximized ? "Restore" : "Maximize"}
          >
            {isWindowMaximized ? <Copy className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
          </button>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              (window as any).ipc.postMessage('close_window');
            }}
            className="px-3 h-full text-[#d7dadc] hover:bg-[#e81123] hover:text-white transition-colors flex items-center"
            title="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>
    </header>
  );
}
