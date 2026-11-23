import { useEffect, useRef, useState } from 'react';
import { Video, Loader2, Music4, KeyboardDoubleArrowLeft, KeyboardDoubleArrowRight } from '@/components/ui/MaterialIcon';
import type { ImportedAudioSegment } from '@/types/video';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';

// ============================================================================
// Placeholder
// ============================================================================
interface PlaceholderProps {
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  onImportVideo?: (file: File) => void;
  /** Imported audio placeholder fallback — show audio info instead of "no video" copy. */
  audioSegments?: ImportedAudioSegment[];
}

export function Placeholder({
  isLoadingVideo,
  loadingProgress,
  isRecording,
  recordingDuration,
  onImportVideo,
  audioSegments,
}: PlaceholderProps) {
  const { t } = useSettings();
  return (
    <div className="placeholder-container absolute inset-0 bg-[var(--surface)] flex flex-col items-center justify-center">
      <div className="placeholder-background absolute inset-0 opacity-5">
        <div
          className="grid-pattern w-full h-full"
          style={{
            backgroundImage: `
              linear-gradient(to right, #fff 1px, transparent 1px),
              linear-gradient(to bottom, #fff 1px, transparent 1px)
            `,
            backgroundSize: '20px 20px'
          }}
        />
      </div>

      {isLoadingVideo ? (
        <div className="loading-state flex flex-col items-center">
          <Loader2 className="w-8 h-8 text-[var(--primary-color)] animate-spin mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.processingVideo}</p>
          <p className="text-[var(--outline)] text-xs mt-1">{t.processingHint}</p>
        </div>
      ) : isRecording ? (
        <div className="recording-state flex flex-col items-center">
          <div className="recording-pulse w-3 h-3 rounded-full bg-[var(--tertiary-color)] animate-pulse mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.recordingInProgress}</p>
          <span className="text-[var(--on-surface)] text-lg font-mono mt-2">{formatTime(recordingDuration)}</span>
        </div>
      ) : audioSegments && audioSegments.length > 0 ? (
        <div className="audio-placeholder-state flex flex-col items-center max-w-[80%] text-center">
          <Music4 className="w-10 h-10 text-[var(--primary-color)] mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium truncate w-full">
            {audioSegments[0].name}
          </p>
          <p className="text-[var(--outline)] text-xs mt-1 tabular-nums">
            {formatTime(audioSegments[0].duration)}
            {audioSegments.length > 1 ? ` · +${audioSegments.length - 1}` : ''}
          </p>
          <p className="text-[var(--outline)] text-[10px] mt-3 opacity-70">{t.audioPlaceholderProjectHint}</p>
        </div>
      ) : (
        <div className="no-video-state flex flex-col items-center">
          <Video className="w-8 h-8 text-[var(--outline-variant)] mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.noVideoSelected}</p>
          <p className="text-[var(--outline)] text-xs mt-1">{t.startRecordingHint}</p>
          {onImportVideo && (
            <label className="placeholder-import-btn mt-3 text-[10px] text-[var(--outline)] hover:text-[var(--primary-color)] cursor-pointer transition-colors">
              {t.orImportVideo}
              <input type="file" accept="video/*" className="hidden" onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) onImportVideo(file);
                e.target.value = "";
              }} />
            </label>
          )}
        </div>
      )}
      {isLoadingVideo && loadingProgress > 0 && (
        <div className="loading-progress mt-2">
          <p className="text-[var(--outline)] text-xs">
            {t.loadingVideo} {Math.min(Math.round(loadingProgress), 100)}%
          </p>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// SeekIndicator
// ============================================================================
export function SeekIndicator({ dir, showKey }: { dir: 'left' | 'right'; showKey: number }) {
  const [isVisible, setIsVisible] = useState(false);
  const [accumulatedSec, setAccumulatedSec] = useState(5);
  const [animKey, setAnimKey] = useState(0);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastDirRef = useRef(dir);

  useEffect(() => {
    if (showKey <= 0) return;

    // Reset accumulation if direction changed or too much time passed
    if (dir !== lastDirRef.current) {
      setAccumulatedSec(5);
    } else if (isVisible) {
      setAccumulatedSec((prev) => prev + 5);
    } else {
      setAccumulatedSec(5);
    }
    lastDirRef.current = dir;

    setIsVisible(true);
    setAnimKey((k) => k + 1);

    // Reset hide timer on each press
    if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    hideTimerRef.current = setTimeout(() => {
      setIsVisible(false);
      setAccumulatedSec(5);
    }, 700);

    return () => {
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    };
  }, [showKey]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!isVisible) return null;

  return (
    <div className="seek-indicator absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[60] pointer-events-none">
      <div
        key={animKey}
        className="seek-indicator-badge bg-black/75 backdrop-blur-xs text-white px-4 py-2.5 rounded-2xl flex items-center gap-1.5 shadow-2xl"
        style={{
          animation: 'seek-pop 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) forwards',
        }}
      >
        {dir === 'left' && <KeyboardDoubleArrowLeft className="seek-indicator-arrow" size={18} />}
        <span className="font-bold text-sm tabular-nums">{accumulatedSec}s</span>
        {dir === 'right' && <KeyboardDoubleArrowRight className="seek-indicator-arrow" size={18} />}
      </div>
    </div>
  );
}
