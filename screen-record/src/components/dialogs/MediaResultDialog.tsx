import { useState, useEffect, useRef, useCallback, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { FolderOpen, Copy, CheckCircle2 } from '@/components/ui/MaterialIcon';
import { invoke } from '@/lib/ipc';
import { useSettings } from '@/hooks/useSettings';
import type { ExportArtifact } from '@/types/video';
import {
  CustomAudioPlayer,
  CustomVideoPlayer,
} from '@/components/dialogs/MediaResultPlayers';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
} from '@/components/ui/Dialog';

// ============================================================================
// MediaResultDialog
// ============================================================================
interface MediaResultDialogProps {
  show: boolean;
  onClose: () => void;
  title: string;
  filePath: string;
  onFilePathChange: (newPath: string) => void;
  isBusy?: boolean;
  mediaKind?: 'video' | 'audio';
  extraControls?: ReactNode;
}

export function MediaResultDialog({
  show,
  onClose,
  title,
  filePath,
  onFilePathChange,
  isBusy = false,
  mediaKind = 'video',
  extraControls
}: MediaResultDialogProps) {
  const { t } = useSettings();
  const isAudio = mediaKind === 'audio';
  const isGif = filePath.toLowerCase().endsWith('.gif');
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [streamUrl, setStreamUrl] = useState('');
  const [previewReady, setPreviewReady] = useState(false);
  const [isVideoFullscreen, setIsVideoFullscreen] = useState(false);
  const streamSessionRef = useRef(false);
  const isVideoFullscreenRef = useRef(false);
  const renameInputRef = useRef<HTMLInputElement | null>(null);

  const enterVideoFullscreen = useCallback(() => {
    (window as any).ipc?.postMessage('enter_fullscreen');
    setIsVideoFullscreen(true);
    isVideoFullscreenRef.current = true;
    setIsRenaming(false);
  }, []);

  const exitVideoFullscreen = useCallback(() => {
    setIsVideoFullscreen(false);
    isVideoFullscreenRef.current = false;
    (window as any).ipc?.postMessage('exit_fullscreen');
  }, []);

  // Escape key exits fullscreen
  useEffect(() => {
    if (!isVideoFullscreen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopImmediatePropagation();
        exitVideoFullscreen();
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [isVideoFullscreen, exitVideoFullscreen]);

  useEffect(() => {
    if (!show) {
      if (isVideoFullscreenRef.current) {
        isVideoFullscreenRef.current = false;
        setIsVideoFullscreen(false);
        (window as any).ipc?.postMessage('exit_fullscreen');
      }
      streamSessionRef.current = false;
      setStreamUrl('');
      setPreviewReady(false);
      return;
    }
    if (!filePath) return;
    const parts = filePath.split(/[/\\]/);
    setRenameValue(parts[parts.length - 1] || '');
    if (streamSessionRef.current) return;
    streamSessionRef.current = true;
    setPreviewReady(false);
    invoke<number>('get_media_server_port').then((port) => {
      if (port > 0) {
        setStreamUrl(`http://127.0.0.1:${port}/?path=${encodeURIComponent(filePath)}`);
      }
    }).catch(console.error);
  }, [show, filePath]);

  useEffect(() => {
    if (!isRenaming) return;
    const input = renameInputRef.current;
    if (!input) return;
    const extensionStart = renameValue.lastIndexOf('.');
    const selectionEnd = extensionStart > 0 ? extensionStart : renameValue.length;
    requestAnimationFrame(() => {
      input.focus();
      input.setSelectionRange(0, selectionEnd);
    });
  }, [isRenaming, renameValue]);

  if (!show) return null;

  const handleShowInFolder = async () => {
    if (!filePath) return;
    try { await invoke('show_in_folder', { path: filePath }); } catch (e) { console.error(e); }
  };

  const handleCopyVideo = async () => {
    if (!filePath) return;
    try { await invoke('copy_video_file_to_clipboard', { filePath }); } catch (e) { console.error(e); }
  };

  const submitRename = async () => {
    if (!renameValue.trim() || !filePath) { setIsRenaming(false); return; }
    try {
      const newPath = await invoke<string>('rename_file', { path: filePath, newName: renameValue.trim() });
      if (newPath) onFilePathChange(newPath);
    } catch (e) {
      console.error('Rename failed', e);
    } finally {
      setIsRenaming(false);
    }
  };

  // Fullscreen mode bypasses Radix Dialog for full-window takeover
  if (isVideoFullscreen) {
    return (
      <div className="media-result-dialog-backdrop fixed inset-0 z-[200] bg-black flex items-center justify-center">
        <div className="media-result-dialog fixed inset-0 bg-black flex flex-col">
          {filePath && streamUrl && !isGif && !isAudio && (
            <div className="flex-1 relative min-h-0">
              <div className="absolute inset-0 overflow-hidden bg-[var(--ui-surface-2)]">
                <CustomVideoPlayer
                  src={streamUrl}
                  isFullscreen
                  onEnterFullscreen={enterVideoFullscreen}
                  onExitFullscreen={exitVideoFullscreen}
                  onReady={() => setPreviewReady(true)}
                />
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="max-w-[640px]">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <DialogBody className="flex flex-col gap-4">
          {isBusy && !filePath ? (
            <div className="media-result-header-row ui-inline-note flex items-center gap-2 rounded-xl px-3 py-2.5">
              <div className="h-4 w-4 flex-shrink-0 rounded-full border-2 border-[var(--primary-color)] border-t-transparent animate-spin" />
              <span className="text-sm text-[var(--on-surface-variant)]">{t.saving}</span>
            </div>
          ) : (
            <div className="media-result-header-row flex items-center gap-2 rounded-lg border border-emerald-400/45 bg-emerald-500/10 px-3 py-2.5">
              <CheckCircle2 className="h-4 w-4 flex-shrink-0 text-emerald-600 dark:text-emerald-300" />
              <div className="min-w-0 flex-1 text-sm text-emerald-700 dark:text-emerald-200 whitespace-nowrap">
                <span className="font-semibold">{t.savedTo}</span>
                <span className="mx-1 opacity-70">·</span>
                <span className="inline-block max-w-full truncate align-middle" title={`${title}: ${filePath}`}>{filePath}</span>
              </div>
            </div>
          )}

          {filePath ? (
            <div className="media-result-content flex flex-col gap-4">
              <div className={`media-preview-box relative ${isAudio ? 'min-h-[220px]' : 'aspect-video min-h-[220px]'} overflow-hidden bg-[var(--ui-surface-2)]`}>
                {streamUrl ? (
                  isAudio ? (
                    <CustomAudioPlayer
                      src={streamUrl}
                      onReady={() => setPreviewReady(true)}
                    />
                  ) : isGif ? (
                    <img
                      src={streamUrl}
                      alt="GIF preview"
                      className="gif-preview absolute inset-0 w-full h-full object-contain"
                      onLoad={() => setPreviewReady(true)}
                    />
                  ) : (
                    <CustomVideoPlayer
                      src={streamUrl}
                      isFullscreen={isVideoFullscreen}
                      onEnterFullscreen={enterVideoFullscreen}
                      onExitFullscreen={exitVideoFullscreen}
                      onReady={() => setPreviewReady(true)}
                    />
                  )
                ) : (
                  <div className="media-preview-placeholder absolute inset-0 flex items-center justify-center text-xs text-white/70">{title}</div>
                )}
                {!previewReady && (
                  <div className="media-preview-loading absolute inset-0 flex items-center justify-center text-xs text-white/50">{title}</div>
                )}
              </div>

              <div className="media-path-editor ui-surface rounded-xl p-3">
                {isRenaming ? (
                  <div className="flex gap-2">
                    <input
                      ref={renameInputRef}
                      autoFocus
                      value={renameValue}
                      onChange={(e) => setRenameValue(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') submitRename();
                        if (e.key === 'Escape') setIsRenaming(false);
                      }}
                      className="media-rename-input ui-input flex-1 rounded-lg border border-[var(--primary-color)] bg-[var(--ui-surface-3)] px-3 py-1.5 text-sm outline-none"
                    />
                    <Button
                      size="sm"
                      onClick={submitRename}
                      disabled={isBusy}
                      className="media-rename-save-btn ui-action-button h-8 text-xs"
                      data-tone="primary"
                      data-active="true"
                      data-emphasis="strong"
                    >
                      {t.save}
                    </Button>
                  </div>
                ) : (
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-sm text-[var(--on-surface)] truncate font-medium flex-1" title={filePath}>{renameValue}</div>
                    <Button size="sm" variant="outline" onClick={() => setIsRenaming(true)} disabled={isBusy} className="h-8 text-xs flex-shrink-0">{t.rename}</Button>
                  </div>
                )}
              </div>

              <div className="media-actions flex items-center justify-between mt-2 gap-3">
                <div className="flex gap-2">
                  <Button variant="outline" onClick={handleShowInFolder} disabled={isBusy} className="h-9 rounded-lg text-xs transition-all">
                    <FolderOpen className="w-3.5 h-3.5 mr-1.5" /> {t.showInFolder}
                  </Button>
                  <Button
                    onClick={handleCopyVideo}
                    disabled={isBusy}
                    className="media-copy-btn ui-action-button h-9 text-xs transition-all"
                    data-tone="primary"
                    data-active="true"
                    data-emphasis="strong"
                  >
                    <Copy className="w-3.5 h-3.5 mr-1.5" /> {isAudio ? t.copyAudio : isGif ? t.copyGif : t.copyVideo}
                  </Button>
                </div>
                {extraControls}
              </div>
            </div>
          ) : (
            <div className="text-sm text-[var(--on-surface-variant)] py-4 text-center">{t.rawVideoPathUnavailable}</div>
          )}
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}

// RawVideoDialog & ExportSuccessDialog Wrappers
// ============================================================================
interface RawVideoDialogProps {
  show: boolean;
  onClose: () => void;
  savedPath: string;
  autoCopyEnabled: boolean;
  isBusy?: boolean;
  onChangePath: (newPath: string) => void;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function RawVideoDialog({
  show,
  onClose,
  savedPath,
  autoCopyEnabled,
  isBusy = false,
  onChangePath,
  onToggleAutoCopy
}: RawVideoDialogProps) {
  const { t } = useSettings();
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.rawVideoDialogTitle}
      filePath={savedPath}
      onFilePathChange={onChangePath}
      isBusy={isBusy}
      extraControls={
        <label className="media-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
          <Checkbox checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} disabled={isBusy} />
          <span>{t.autoCopyAfterRecording}</span>
        </label>
      }
    />
  );
}

interface ExportSuccessDialogProps {
  show: boolean;
  onClose: () => void;
  filePath: string;
  artifacts?: ExportArtifact[];
  onFilePathChange: (newPath: string) => void;
  autoCopyEnabled: boolean;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function ExportSuccessDialog({
  show,
  onClose,
  filePath,
  artifacts = [],
  onFilePathChange,
  autoCopyEnabled,
  onToggleAutoCopy,
}: ExportSuccessDialogProps) {
  const { t } = useSettings();
  const isGif = filePath.toLowerCase().endsWith('.gif');
  const extraArtifactSummary = artifacts.length > 1
    ? artifacts
        .map((artifact) => artifact.path.split(/[/\\]/).pop() || artifact.path)
        .join(' • ')
    : '';
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.exportSuccessful}
      filePath={filePath}
      onFilePathChange={onFilePathChange}
      extraControls={
        <div className="media-export-extra flex flex-col items-end gap-1">
          {extraArtifactSummary ? (
            <div className="media-export-artifact-summary text-[10px] text-[var(--on-surface-variant)] text-right">
              {t.exportArtifactsSaved}: {extraArtifactSummary}
            </div>
          ) : null}
          <label className="media-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
            <Checkbox checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} />
            <span>{isGif ? t.autoCopyGifAfterExport : t.autoCopyVideoAfterExport}</span>
          </label>
        </div>
      }
    />
  );
}

interface AudioDownloadSuccessDialogProps {
  show: boolean;
  onClose: () => void;
  filePath: string;
  onFilePathChange: (newPath: string) => void;
}

export function AudioDownloadSuccessDialog({
  show,
  onClose,
  filePath,
  onFilePathChange,
}: AudioDownloadSuccessDialogProps) {
  const { t } = useSettings();
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.audioDownloadSuccessful}
      filePath={filePath}
      onFilePathChange={onFilePathChange}
      mediaKind="audio"
    />
  );
}
