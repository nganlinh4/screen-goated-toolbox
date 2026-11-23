import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';

const RESIZE_SETTLE_MS = 50;

/**
 * Tracks the host window's maximized state. The window can be maximized or
 * restored outside our buttons (Win+Up, Aero snap, title-bar double click), and
 * every one of those resizes the WebView, so a settled resize re-queries it.
 */
export function useWindowMaximized() {
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);
  const mountedRef = useRef(true);
  const timerRef = useRef<number | null>(null);

  const refreshMaximized = useCallback(async () => {
    try {
      const maximized = await invoke<boolean>('is_maximized');
      if (mountedRef.current) setIsWindowMaximized(Boolean(maximized));
    } catch {
      // Host query failed: keep the last known state rather than guessing.
    }
  }, []);

  const scheduleRefresh = useCallback(() => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      void refreshMaximized();
    }, RESIZE_SETTLE_MS);
  }, [refreshMaximized]);

  useEffect(() => {
    mountedRef.current = true;
    void refreshMaximized();
    window.addEventListener('resize', scheduleRefresh);
    return () => {
      mountedRef.current = false;
      window.removeEventListener('resize', scheduleRefresh);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = null;
    };
  }, [refreshMaximized, scheduleRefresh]);

  return { isWindowMaximized, scheduleRefresh };
}
