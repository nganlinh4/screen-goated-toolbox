import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from '@/lib/ipc';

const RAW_AUTO_COPY_KEY = 'screen-record-raw-auto-copy-v1';
const RAW_SAVE_DIR_KEY = 'screen-record-raw-save-dir-v1';

function getInitialRawAutoCopy(): boolean {
  try {
    return localStorage.getItem(RAW_AUTO_COPY_KEY) === '1';
  } catch {
    return false;
  }
}

function getInitialRawSaveDir(): string {
  try {
    return localStorage.getItem(RAW_SAVE_DIR_KEY) || '';
  } catch {
    return '';
  }
}

export interface UseRawVideoHandlerReturn {
  currentRawVideoPath: string;
  setCurrentRawVideoPath: (path: string) => void;
  lastRawSavedPath: string;
  setLastRawSavedPath: (path: string) => void;
  showRawVideoDialog: boolean;
  setShowRawVideoDialog: (show: boolean) => void;
  rawAutoCopyEnabled: boolean;
  setRawAutoCopyEnabled: (enabled: boolean) => void;
  rawSaveDir: string;
  setRawSaveDir: (dir: string) => void;
  isRawActionBusy: boolean;
  setIsRawActionBusy: (busy: boolean) => void;
  rawButtonSavedFlash: boolean;
  setRawButtonSavedFlash: (flash: boolean) => void;
  flashRawSavedButton: () => void;
  ensureRawVideoSaved: () => Promise<string>;
  handleOpenRawVideoDialog: () => Promise<void>;
  handleChangeRawSavePath: () => Promise<void>;
  handleCopyRawVideo: () => Promise<void>;
  handleToggleRawAutoCopy: (enabled: boolean) => Promise<void>;
}

export function useRawVideoHandler(): UseRawVideoHandlerReturn {
  const [currentRawVideoPath, setCurrentRawVideoPath] = useState('');
  const [lastRawSavedPath, setLastRawSavedPath] = useState('');
  const [showRawVideoDialog, setShowRawVideoDialog] = useState(false);
  const [rawAutoCopyEnabled, setRawAutoCopyEnabled] = useState(getInitialRawAutoCopy);
  const [rawSaveDir, setRawSaveDir] = useState(getInitialRawSaveDir);
  const [isRawActionBusy, setIsRawActionBusy] = useState(false);
  const [rawButtonSavedFlash, setRawButtonSavedFlash] = useState(false);
  const rawButtonFlashTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Persist raw auto-copy setting
  useEffect(() => {
    try {
      localStorage.setItem(RAW_AUTO_COPY_KEY, rawAutoCopyEnabled ? '1' : '0');
    } catch {
      // ignore persistence failures
    }
  }, [rawAutoCopyEnabled]);

  // Persist raw save dir
  useEffect(() => {
    if (!rawSaveDir) return;
    try {
      localStorage.setItem(RAW_SAVE_DIR_KEY, rawSaveDir);
    } catch {
      // ignore persistence failures
    }
  }, [rawSaveDir]);

  // Initialize raw save dir from backend default if not set
  useEffect(() => {
    if (rawSaveDir) return;
    invoke<string>('get_default_export_dir')
      .then((dir) => {
        if (dir) setRawSaveDir(dir);
      })
      .catch((e) => console.error('[RawVideo] Failed to get default dir:', e));
  }, [rawSaveDir]);

  // Cleanup flash timer on unmount
  useEffect(() => {
    return () => {
      if (rawButtonFlashTimerRef.current) {
        clearTimeout(rawButtonFlashTimerRef.current);
      }
    };
  }, []);

  const flashRawSavedButton = useCallback(() => {
    setRawButtonSavedFlash(true);
    if (rawButtonFlashTimerRef.current) clearTimeout(rawButtonFlashTimerRef.current);
    rawButtonFlashTimerRef.current = setTimeout(() => {
      setRawButtonSavedFlash(false);
      rawButtonFlashTimerRef.current = null;
    }, 4000);
  }, []);

  const ensureRawVideoSaved = useCallback(async (): Promise<string> => {
    if (lastRawSavedPath) return lastRawSavedPath;
    if (!currentRawVideoPath) return '';
    if (!rawSaveDir) return '';

    const result = await invoke<{ savedPath: string }>('save_raw_video_copy', {
      sourcePath: currentRawVideoPath,
      targetDir: rawSaveDir,
    });
    const savedPath = result?.savedPath || '';
    if (savedPath) {
      setLastRawSavedPath(savedPath);
    }
    return savedPath;
  }, [lastRawSavedPath, currentRawVideoPath, rawSaveDir]);

  const handleOpenRawVideoDialog = useCallback(async () => {
    setShowRawVideoDialog(true);
    if (lastRawSavedPath || !currentRawVideoPath) return;
    try {
      setIsRawActionBusy(true);
      await ensureRawVideoSaved();
    } catch (e) {
      console.error('[RawVideo] Failed to save raw video on dialog open:', e);
    } finally {
      setIsRawActionBusy(false);
    }
  }, [lastRawSavedPath, currentRawVideoPath, ensureRawVideoSaved]);

  const handleChangeRawSavePath = useCallback(async () => {
    try {
      setIsRawActionBusy(true);
      const selected = await invoke<string | null>('pick_export_folder', {
        initialDir: rawSaveDir || null,
      });
      if (!selected) return;

      setRawSaveDir(selected);

      if (lastRawSavedPath) {
        const moved = await invoke<{ savedPath: string }>('move_saved_raw_video', {
          currentPath: lastRawSavedPath,
          targetDir: selected,
        });
        if (moved?.savedPath) {
          setLastRawSavedPath(moved.savedPath);
        }
      }
    } catch (e) {
      console.error('[RawVideo] Failed to change raw save path:', e);
    } finally {
      setIsRawActionBusy(false);
    }
  }, [rawSaveDir, lastRawSavedPath]);

  const handleCopyRawVideo = useCallback(async () => {
    try {
      setIsRawActionBusy(true);
      const savedPath = await ensureRawVideoSaved();
      if (!savedPath) return;
      await invoke('copy_video_file_to_clipboard', { filePath: savedPath });
      flashRawSavedButton();
    } catch (e) {
      console.error('[RawVideo] Failed to copy raw video to clipboard:', e);
    } finally {
      setIsRawActionBusy(false);
    }
  }, [ensureRawVideoSaved, flashRawSavedButton]);

  const handleToggleRawAutoCopy = useCallback(async (enabled: boolean) => {
    setRawAutoCopyEnabled(enabled);
    if (!enabled) return;
    try {
      setIsRawActionBusy(true);
      const savedPath = await ensureRawVideoSaved();
      if (!savedPath) return;
      await invoke('copy_video_file_to_clipboard', { filePath: savedPath });
      flashRawSavedButton();
    } catch (e) {
      console.error('[RawVideo] Failed to enable auto-copy for raw video:', e);
    } finally {
      setIsRawActionBusy(false);
    }
  }, [ensureRawVideoSaved, flashRawSavedButton]);

  return {
    currentRawVideoPath,
    setCurrentRawVideoPath,
    lastRawSavedPath,
    setLastRawSavedPath,
    showRawVideoDialog,
    setShowRawVideoDialog,
    rawAutoCopyEnabled,
    setRawAutoCopyEnabled,
    rawSaveDir,
    setRawSaveDir,
    isRawActionBusy,
    setIsRawActionBusy,
    rawButtonSavedFlash,
    setRawButtonSavedFlash,
    flashRawSavedButton,
    ensureRawVideoSaved,
    handleOpenRawVideoDialog,
    handleChangeRawSavePath,
    handleCopyRawVideo,
    handleToggleRawAutoCopy,
  };
}
