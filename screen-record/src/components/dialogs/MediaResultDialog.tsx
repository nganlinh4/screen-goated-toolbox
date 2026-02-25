import { useState, useEffect, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { X, FolderOpen, Copy, CheckCircle2 } from 'lucide-react';
import { invoke } from '@/lib/ipc';
import { useSettings } from '@/hooks/useSettings';

interface MediaResultDialogProps {
  show: boolean;
  onClose: () => void;
  title: string;
  filePath: string;
  onFilePathChange: (newPath: string) => void;
  isBusy?: boolean;
  extraControls?: ReactNode;
}

export function MediaResultDialog({
  show,
  onClose,
  title,
  filePath,
  onFilePathChange,
  isBusy = false,
  extraControls
}: MediaResultDialogProps) {
  const { t } = useSettings();
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [streamUrl, setStreamUrl] = useState('');
  const [previewReady, setPreviewReady] = useState(false);

  useEffect(() => {
    if (show && filePath) {
      setPreviewReady(false);
      const parts = filePath.split(/[/\\]/);
      setRenameValue(parts[parts.length - 1] || '');
      invoke<number>('get_media_server_port').then((port) => {
        if (port > 0) {
          setStreamUrl(`http://127.0.0.1:${port}/?path=${encodeURIComponent(filePath)}`);
        }
      }).catch(console.error);
    } else {
      setStreamUrl('');
    }
  }, [show, filePath]);

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
    if (!renameValue.trim() || !filePath) {
      setIsRenaming(false);
      return;
    }
    try {
      const newPath = await invoke<string>('rename_file', { path: filePath, newName: renameValue.trim() });
      if (newPath) onFilePathChange(newPath);
    } catch (e) {
      console.error('Rename failed', e);
    } finally {
      setIsRenaming(false);
    }
  };

  return (
    <div className="media-result-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="media-result-dialog bg-[var(--surface-dim)] p-5 rounded-xl border border-[var(--glass-border)] shadow-2xl max-w-[640px] w-full mx-4 flex flex-col gap-4">
        <div className="media-result-title-row flex items-center justify-between gap-3">
          <h3 className="media-result-title text-sm font-semibold text-[var(--on-surface)]">{title}</h3>
          <button
            onClick={onClose}
            className="media-result-close-btn p-1.5 rounded-lg text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {isBusy && !filePath ? (
          <div className="media-result-header-row flex items-center gap-2 rounded-lg border border-[var(--glass-border)] bg-[var(--glass-bg)] px-3 py-2.5">
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
            <div className="media-preview-box relative aspect-video min-h-[220px] rounded-lg overflow-hidden bg-black border border-[var(--glass-border)] shadow-inner">
              {streamUrl && (
                <video
                  src={streamUrl}
                  controls
                  preload="metadata"
                  onLoadedData={() => setPreviewReady(true)}
                  onCanPlay={() => setPreviewReady(true)}
                  className="media-preview-video absolute inset-0 h-full w-full object-contain"
                />
              )}
              {!previewReady && (
                <div className="media-preview-placeholder absolute inset-0 flex items-center justify-center text-xs text-white/70">
                  {title}
                </div>
              )}
            </div>

            <div className="media-path-editor bg-[var(--surface-container)] rounded-lg p-3 border border-[var(--glass-border)] shadow-sm">
              {isRenaming ? (
                <div className="flex gap-2">
                  <input
                    autoFocus
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') submitRename();
                      if (e.key === 'Escape') setIsRenaming(false);
                    }}
                    className="flex-1 bg-[var(--surface)] text-sm px-3 py-1.5 rounded border border-[var(--primary-color)] outline-none"
                  />
                  <Button size="sm" onClick={submitRename} disabled={isBusy} className="h-8 text-xs bg-[var(--primary-color)] text-white hover:opacity-90">{t.save}</Button>
                </div>
              ) : (
                <div className="flex items-center justify-between gap-3">
                  <div className="text-sm text-[var(--on-surface)] truncate font-medium flex-1" title={filePath}>{renameValue}</div>
                  <Button size="sm" variant="ghost" onClick={() => setIsRenaming(true)} disabled={isBusy} className="h-8 text-xs flex-shrink-0 hover:bg-[var(--glass-bg-hover)] border border-[var(--glass-border)]">{t.rename}</Button>
                </div>
              )}
            </div>

            <div className="media-actions flex items-center justify-between mt-2 gap-3">
              <div className="flex gap-2">
                <Button variant="outline" onClick={handleShowInFolder} disabled={isBusy} className="h-9 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-all">
                  <FolderOpen className="w-3.5 h-3.5 mr-1.5" /> {t.showInFolder}
                </Button>
                <Button onClick={handleCopyVideo} disabled={isBusy} className="h-9 text-xs bg-[var(--primary-color)] hover:opacity-90 text-white transition-all shadow-sm">
                  <Copy className="w-3.5 h-3.5 mr-1.5" /> {t.copyVideo}
                </Button>
              </div>
              {extraControls}
            </div>
          </div>
        ) : (
          <div className="text-sm text-[var(--on-surface-variant)] py-4 text-center">{t.rawVideoPathUnavailable}</div>
        )}
      </div>
    </div>
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
          <input type="checkbox" className="rounded border-[var(--outline)]" checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} disabled={isBusy} />
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
  onFilePathChange: (newPath: string) => void;
  autoCopyEnabled: boolean;
  onToggleAutoCopy: (enabled: boolean) => void;
}

export function ExportSuccessDialog({ show, onClose, filePath, onFilePathChange, autoCopyEnabled, onToggleAutoCopy }: ExportSuccessDialogProps) {
  const { t } = useSettings();
  return (
    <MediaResultDialog
      show={show}
      onClose={onClose}
      title={t.exportSuccessful}
      filePath={filePath}
      onFilePathChange={onFilePathChange}
      extraControls={
        <label className="media-auto-copy-toggle flex items-center gap-2 text-xs text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] cursor-pointer transition-colors">
          <input type="checkbox" className="rounded border-[var(--outline)]" checked={autoCopyEnabled} onChange={(e) => onToggleAutoCopy(e.target.checked)} />
          <span>{t.autoCopyAfterExport}</span>
        </label>
      }
    />
  );
}
