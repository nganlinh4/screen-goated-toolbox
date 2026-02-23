import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@/lib/ipc';

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
