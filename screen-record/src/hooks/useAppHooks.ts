import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// Re-export types for convenience
export interface MonitorInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  is_primary: boolean;
}

export interface Hotkey {
  code: number;
  name: string;
  modifiers: number;
}

export type FfmpegInstallStatus =
  | { type: 'Idle' }
  | { type: 'Downloading'; progress: number; totalSize: number }
  | { type: 'Extracting' }
  | { type: 'Installed' }
  | { type: 'Error'; message: string }
  | { type: 'Cancelled' };

// ============================================================================
// useThrottle
// ============================================================================
export const useThrottle = (callback: Function, limit: number) => {
  const lastRunRef = useRef<number>(0);

  return useCallback((...args: any[]) => {
    const now = Date.now();
    if (now - lastRunRef.current >= limit) {
      callback(...args);
      lastRunRef.current = now;
    }
  }, [callback, limit]);
};

// ============================================================================
// useFfmpegSetup
// ============================================================================
export function useFfmpegSetup() {
  const [ffmpegMissing, setFfmpegMissing] = useState(false);
  const [ffprobeMissing, setFfprobeMissing] = useState(false);
  const [isCheckingDeps, setIsCheckingDeps] = useState(true);
  const [ffmpegInstallStatus, setFfmpegInstallStatus] = useState<FfmpegInstallStatus>({ type: 'Idle' });

  useEffect(() => {
    const checkDependencies = async () => {
      try {
        const status = await invoke<{ ffmpegMissing: boolean; ffprobeMissing: boolean }>('check_ffmpeg_status');
        setFfmpegMissing(status.ffmpegMissing);
        setFfprobeMissing(status.ffprobeMissing);

        if (status.ffmpegMissing || status.ffprobeMissing) {
          await invoke('start_ffmpeg_install');
          const pollInterval = setInterval(async () => {
            try {
              const progress = await invoke<any>('get_ffmpeg_install_progress');
              if (progress === 'Idle') {
                setFfmpegInstallStatus({ type: 'Idle' });
              } else if (progress === 'Extracting') {
                setFfmpegInstallStatus({ type: 'Extracting' });
              } else if (progress === 'Installed') {
                setFfmpegInstallStatus({ type: 'Installed' });
                setFfmpegMissing(false);
                setFfprobeMissing(false);
                clearInterval(pollInterval);
              } else if (progress === 'Cancelled') {
                setFfmpegInstallStatus({ type: 'Cancelled' });
                clearInterval(pollInterval);
              } else if (typeof progress === 'object') {
                if ('Downloading' in progress) {
                  setFfmpegInstallStatus({ type: 'Downloading', progress: progress.Downloading.progress, totalSize: progress.Downloading.total_size });
                } else if ('Error' in progress) {
                  setFfmpegInstallStatus({ type: 'Error', message: progress.Error });
                  clearInterval(pollInterval);
                }
              }
            } catch (e) {
              console.error('Failed to poll install progress:', e);
            }
          }, 200);
        }
      } catch (e) {
        console.error('Failed to check ffmpeg status:', e);
      } finally {
        setIsCheckingDeps(false);
      }
    };
    checkDependencies();
  }, []);

  const handleCancelInstall = async () => {
    try {
      await invoke('cancel_ffmpeg_install');
      setFfmpegInstallStatus({ type: 'Cancelled' });
    } catch (e) {
      console.error('Failed to cancel install:', e);
    }
  };

  const needsSetup = (isCheckingDeps || ffmpegMissing || ffprobeMissing || ffmpegInstallStatus.type !== 'Idle') && ffmpegInstallStatus.type !== 'Installed';

  return { ffmpegInstallStatus, handleCancelInstall, needsSetup };
}

// ============================================================================
// useHotkeys
// ============================================================================
export function useHotkeys() {
  const [hotkeys, setHotkeys] = useState<Hotkey[]>([]);
  const [showHotkeyDialog, setShowHotkeyDialog] = useState(false);
  const [listeningForKey, setListeningForKey] = useState(false);

  useEffect(() => {
    invoke<Hotkey[]>('get_hotkeys').then(setHotkeys).catch(() => {});
  }, []);

  const handleRemoveHotkey = async (index: number) => {
    try {
      await invoke('remove_hotkey', { index });
      setHotkeys(prev => prev.filter((_, i) => i !== index));
    } catch (err) {
      console.error("Failed to remove hotkey:", err);
    }
  };

  useEffect(() => {
    if (showHotkeyDialog && listeningForKey) {
      invoke('unregister_hotkeys').catch(() => {});
      window.focus();
    } else {
      invoke('register_hotkeys').catch(() => {});
    }
    return () => { invoke('register_hotkeys').catch(() => {}); };
  }, [showHotkeyDialog, listeningForKey]);

  useEffect(() => {
    if (showHotkeyDialog && listeningForKey) {
      const handleKeyDown = async (e: KeyboardEvent) => {
        e.preventDefault();
        if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;

        const modifiers = [];
        if (e.ctrlKey) modifiers.push('Control');
        if (e.altKey) modifiers.push('Alt');
        if (e.shiftKey) modifiers.push('Shift');
        if (e.metaKey) modifiers.push('Meta');

        try {
          const newHotkey = await invoke<Hotkey>('set_hotkey', { code: e.code, modifiers, key: e.key });
          setHotkeys(prev => [...prev, newHotkey]);
          setListeningForKey(false);
          setShowHotkeyDialog(false);
        } catch (err) {
          console.error("Failed to set hotkey:", err);
          setListeningForKey(false);
        }
      };

      window.addEventListener('keydown', handleKeyDown);
      return () => window.removeEventListener('keydown', handleKeyDown);
    }
  }, [showHotkeyDialog, listeningForKey]);

  const openHotkeyDialog = () => { setShowHotkeyDialog(true); setListeningForKey(true); };
  const closeHotkeyDialog = () => { setListeningForKey(false); setShowHotkeyDialog(false); };

  return { hotkeys, showHotkeyDialog, handleRemoveHotkey, openHotkeyDialog, closeHotkeyDialog };
}

// ============================================================================
// useKeyviz
// ============================================================================
export function useKeyviz() {
  const [keyvizStatus, setKeyvizStatus] = useState({ installed: false, enabled: false });

  useEffect(() => {
    invoke<{ installed: boolean; enabled: boolean }>('get_keyviz_status').then(setKeyvizStatus).catch(console.error);
  }, []);

  const toggleKeyviz = async () => {
    try {
      if (!keyvizStatus.installed && !keyvizStatus.enabled) {
        await invoke('install_keyviz');
        await invoke('set_keyviz_enabled', { enabled: true });
        setKeyvizStatus({ installed: true, enabled: true });
      } else {
        const newEnabled = !keyvizStatus.enabled;
        await invoke('set_keyviz_enabled', { enabled: newEnabled });
        setKeyvizStatus(prev => ({ ...prev, enabled: newEnabled }));
      }
    } catch (err) {
      console.error("Failed to toggle keyviz:", err);
    }
  };

  return { keyvizStatus, toggleKeyviz };
}

// ============================================================================
// useMonitors
// ============================================================================
const sortMonitorsByPosition = (monitors: MonitorInfo[]) => {
  return [...monitors]
    .sort((a, b) => a.x - b.x)
    .map((monitor, index) => ({ ...monitor, name: `Display ${index + 1}${monitor.is_primary ? ' (Primary)' : ''}` }));
};

export function useMonitors() {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [showMonitorSelect, setShowMonitorSelect] = useState(false);

  const getMonitors = async () => {
    try {
      const monitors = await invoke<MonitorInfo[]>("get_monitors");
      const sortedMonitors = sortMonitorsByPosition(monitors);
      setMonitors(sortedMonitors);
      return sortedMonitors;
    } catch (err) {
      console.error("Failed to get monitors:", err);
      return [];
    }
  };

  return { monitors, showMonitorSelect, setShowMonitorSelect, getMonitors };
}
