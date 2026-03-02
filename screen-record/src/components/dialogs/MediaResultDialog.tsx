import { useState, useEffect, useRef, useCallback, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { X, FolderOpen, Copy, CheckCircle2, Maximize2, Minimize2, Play, Pause, Volume2, VolumeX } from 'lucide-react';
import { invoke } from '@/lib/ipc';
import { useSettings } from '@/hooks/useSettings';

// ============================================================================
// CustomVideoPlayer — replaces native <video controls>
// ============================================================================
function fmtTime(s: number) {
  if (!isFinite(s) || s < 0) return '0:00';
  const m = Math.floor(s / 60);
  return `${m}:${Math.floor(s % 60).toString().padStart(2, '0')}`;
}

function CustomVideoPlayer({
  src,
  isFullscreen,
  onEnterFullscreen,
  onExitFullscreen,
  onReady,
}: {
  src: string;
  isFullscreen: boolean;
  onEnterFullscreen: () => void;
  onExitFullscreen: () => void;
  onReady: () => void;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [dur, setDur] = useState(0);
  const [muted, setMuted] = useState(false);
  const [ctrlVisible, setCtrlVisible] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout>>();
  const scrubbing = useRef(false);

  // Video event listeners
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const onMeta = () => { setDur(v.duration); onReady(); };
    const onCanPlay = () => onReady();
    const onTime = () => { if (!scrubbing.current) setTime(v.currentTime); };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    v.addEventListener('loadedmetadata', onMeta);
    v.addEventListener('canplay', onCanPlay);
    v.addEventListener('timeupdate', onTime);
    v.addEventListener('play', onPlay);
    v.addEventListener('pause', onPause);
    v.addEventListener('ended', onPause);
    return () => {
      v.removeEventListener('loadedmetadata', onMeta);
      v.removeEventListener('canplay', onCanPlay);
      v.removeEventListener('timeupdate', onTime);
      v.removeEventListener('play', onPlay);
      v.removeEventListener('pause', onPause);
      v.removeEventListener('ended', onPause);
    };
  }, [src, onReady]);

  const toggle = useCallback(() => {
    const v = videoRef.current;
    if (v) v.paused ? v.play() : v.pause();
  }, []);

  const seekDelta = useCallback((d: number) => {
    const v = videoRef.current;
    if (v) v.currentTime = Math.max(0, Math.min(v.duration || 0, v.currentTime + d));
  }, []);

  // Keyboard: Space = play/pause, Arrows = ±5s
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.code === 'Space') { e.preventDefault(); toggle(); }
      if (e.code === 'ArrowLeft') { e.preventDefault(); seekDelta(-5); }
      if (e.code === 'ArrowRight') { e.preventDefault(); seekDelta(5); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [toggle, seekDelta]);

  // Auto-hide controls after 3s of inactivity; always visible when paused
  const showCtrl = useCallback(() => {
    setCtrlVisible(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setCtrlVisible(false), 3000);
  }, []);

  useEffect(() => {
    if (!playing) { setCtrlVisible(true); if (hideTimer.current) clearTimeout(hideTimer.current); }
  }, [playing]);

  // Seek bar pointer drag
  const seekTo = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const v = videoRef.current;
    if (v && dur > 0) { v.currentTime = pct * dur; setTime(pct * dur); }
  };

  const progress = dur > 0 ? (time / dur) * 100 : 0;
  const visible = ctrlVisible || !playing;

  return (
    <div
      className="custom-video-player absolute inset-0 bg-black select-none"
      onMouseMove={showCtrl}
      onMouseLeave={() => playing && setCtrlVisible(false)}
    >
      <video
        ref={videoRef}
        src={src}
        preload="metadata"
        className="custom-player-video absolute inset-0 w-full h-full object-contain cursor-pointer"
        onClick={toggle}
      />

      {/* Big center play button when paused */}
      {!playing && dur > 0 && (
        <div className="custom-player-big-play absolute inset-0 flex items-center justify-center cursor-pointer" onClick={toggle}>
          <div className="w-14 h-14 rounded-full bg-black/40 backdrop-blur-md flex items-center justify-center border border-white/10 shadow-xl">
            <Play className="w-7 h-7 text-white ml-0.5" fill="white" />
          </div>
        </div>
      )}

      {/* Bottom controls with gradient scrim */}
      <div className={`custom-player-controls absolute bottom-0 inset-x-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pt-10 pb-2 px-3 transition-opacity duration-300 ${visible ? 'opacity-100' : 'opacity-0 pointer-events-none'}`}>
        {/* Seek bar */}
        <div
          className="custom-player-seek group relative h-5 flex items-center cursor-pointer touch-none"
          onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); scrubbing.current = true; seekTo(e); }}
          onPointerMove={(e) => scrubbing.current && seekTo(e)}
          onPointerUp={() => { scrubbing.current = false; }}
        >
          <div className="custom-seek-track w-full h-[3px] rounded-full bg-white/25 overflow-hidden">
            <div className="custom-seek-fill h-full bg-white rounded-full" style={{ width: `${progress}%` }} />
          </div>
          <div
            className="custom-seek-thumb absolute top-1/2 w-3 h-3 rounded-full bg-white shadow-md -translate-y-1/2 -translate-x-1/2 scale-0 group-hover:scale-100 transition-transform"
            style={{ left: `${progress}%` }}
          />
        </div>

        {/* Controls row */}
        <div className="custom-player-bar flex items-center gap-2 mt-0.5">
          <button onClick={toggle} className="custom-player-play-btn p-1.5 text-white hover:text-white/80 transition-colors">
            {playing ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" fill="white" />}
          </button>
          <span className="custom-player-time text-[11px] font-mono text-white/90 tabular-nums select-none">
            {fmtTime(time)} / {fmtTime(dur)}
          </span>
          <div className="flex-1" />
          <button
            onClick={() => { const v = videoRef.current; if (v) { v.muted = !v.muted; setMuted(!muted); } }}
            className="custom-player-volume-btn p-1.5 text-white/80 hover:text-white transition-colors"
          >
            {muted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          <button
            onClick={isFullscreen ? onExitFullscreen : onEnterFullscreen}
            className="custom-player-fullscreen-btn p-1.5 text-white/80 hover:text-white transition-colors"
          >
            {isFullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
          </button>
        </div>
      </div>
    </div>
  );
}

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
  const [isVideoFullscreen, setIsVideoFullscreen] = useState(false);
  const streamSessionRef = useRef(false);
  const isVideoFullscreenRef = useRef(false);

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

  return (
    <div className={`media-result-dialog-backdrop fixed inset-0 flex items-center justify-center ${isVideoFullscreen ? 'z-[200] bg-black' : 'z-[100] bg-black/70'}`}>
      <div className={`media-result-dialog flex flex-col ${isVideoFullscreen ? 'fixed inset-0 bg-black' : 'bg-[var(--surface-dim)] p-5 rounded-xl border border-[var(--glass-border)] shadow-2xl max-w-[640px] w-full mx-4 gap-4'}`}>

        {/* Title row — hidden in fullscreen */}
        {!isVideoFullscreen && (
          <div className="media-result-title-row flex items-center justify-between gap-3">
            <h3 className="media-result-title text-sm font-semibold text-[var(--on-surface)]">{title}</h3>
            <button onClick={onClose} className="media-result-close-btn p-1.5 rounded-lg text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
              <X className="w-4 h-4" />
            </button>
          </div>
        )}

        {/* Status row — hidden in fullscreen */}
        {!isVideoFullscreen && (isBusy && !filePath ? (
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
        ))}

        {filePath ? (
          <div className={`media-result-content ${isVideoFullscreen ? 'flex-1 relative min-h-0' : 'flex flex-col gap-4'}`}>
            {/* Video box */}
            <div className={`overflow-hidden bg-black ${isVideoFullscreen ? 'absolute inset-0' : 'relative media-preview-box aspect-video min-h-[220px] rounded-lg border border-[var(--glass-border)] shadow-inner'}`}>
              {streamUrl ? (
                <CustomVideoPlayer
                  src={streamUrl}
                  isFullscreen={isVideoFullscreen}
                  onEnterFullscreen={enterVideoFullscreen}
                  onExitFullscreen={exitVideoFullscreen}
                  onReady={() => setPreviewReady(true)}
                />
              ) : !isVideoFullscreen && (
                <div className="media-preview-placeholder absolute inset-0 flex items-center justify-center text-xs text-white/70">{title}</div>
              )}
              {!previewReady && !streamUrl && !isVideoFullscreen && (
                <div className="media-preview-loading absolute inset-0 flex items-center justify-center text-xs text-white/50">{title}</div>
              )}
            </div>

            {/* Path editor + actions — hidden in fullscreen */}
            {!isVideoFullscreen && (
              <>
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
              </>
            )}
          </div>
        ) : (
          !isVideoFullscreen && <div className="text-sm text-[var(--on-surface-variant)] py-4 text-center">{t.rawVideoPathUnavailable}</div>
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
