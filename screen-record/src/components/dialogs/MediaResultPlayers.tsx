import { useState, useEffect, useRef, useCallback } from "react";
import {
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Volume2,
  VolumeX,
} from '@/components/ui/MaterialIcon';
import { useSettings } from '@/hooks/useSettings';
import { formatTime as fmtTime } from "@/utils/helpers";

/** Shared <video>/<audio> playback controller: play/pause + time + duration +
 * mute state, the media-element event wiring, and pointer-scrub seeking. The
 * video and audio players keep only their own chrome/JSX (and the video's
 * keyboard + auto-hide layer). */
function useMediaElementPlayback<T extends HTMLMediaElement>(
  src: string,
  onReady: () => void,
) {
  const mediaRef = useRef<T>(null);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [dur, setDur] = useState(0);
  const [muted, setMuted] = useState(false);
  const scrubbing = useRef(false);

  useEffect(() => {
    const el = mediaRef.current;
    if (!el) return;
    const onMeta = () => {
      setDur(el.duration);
      onReady();
    };
    const onCanPlay = () => onReady();
    const onTime = () => {
      if (!scrubbing.current) setTime(el.currentTime);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    el.addEventListener("loadedmetadata", onMeta);
    el.addEventListener("canplay", onCanPlay);
    el.addEventListener("timeupdate", onTime);
    el.addEventListener("play", onPlay);
    el.addEventListener("pause", onPause);
    el.addEventListener("ended", onPause);
    return () => {
      el.removeEventListener("loadedmetadata", onMeta);
      el.removeEventListener("canplay", onCanPlay);
      el.removeEventListener("timeupdate", onTime);
      el.removeEventListener("play", onPlay);
      el.removeEventListener("pause", onPause);
      el.removeEventListener("ended", onPause);
    };
  }, [src, onReady]);

  const toggle = useCallback(() => {
    const el = mediaRef.current;
    if (!el) return;
    if (el.paused) {
      void el.play().catch((error: unknown) => {
        console.error("Media playback failed", error);
      });
    } else {
      el.pause();
    }
  }, []);

  const seekTo = useCallback(
    (nextTime: number) => {
      const el = mediaRef.current;
      if (el && dur > 0) {
        const clamped = Math.max(0, Math.min(dur, nextTime));
        el.currentTime = clamped;
        setTime(clamped);
      }
    },
    [dur],
  );

  const toggleMute = useCallback(() => {
    const el = mediaRef.current;
    if (el) {
      el.muted = !el.muted;
      setMuted(el.muted);
    }
  }, []);

  const seekProps = {
    min: 0,
    max: Math.max(dur, 0.01),
    step: 0.01,
    value: Math.min(time, Math.max(dur, 0.01)),
    onPointerDown: () => {
      scrubbing.current = true;
    },
    onPointerUp: () => {
      scrubbing.current = false;
    },
    onPointerCancel: () => {
      scrubbing.current = false;
    },
    onBlur: () => {
      scrubbing.current = false;
    },
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => {
      seekTo(Number(event.currentTarget.value));
    },
  };

  const progress = dur > 0 ? (time / dur) * 100 : 0;

  return { mediaRef, playing, time, dur, muted, toggle, toggleMute, seekProps, progress };
}

export function CustomVideoPlayer({
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
  const { t } = useSettings();
  const {
    mediaRef: videoRef,
    playing,
    time,
    dur,
    muted,
    toggle,
    toggleMute,
    seekProps,
    progress,
  } = useMediaElementPlayback<HTMLVideoElement>(src, onReady);
  const [ctrlVisible, setCtrlVisible] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const seekDelta = useCallback(
    (d: number) => {
      const v = videoRef.current;
      if (v) v.currentTime = Math.max(0, Math.min(v.duration || 0, v.currentTime + d));
    },
    [videoRef],
  );

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "BUTTON") return;
      if (e.code === "Space") {
        e.preventDefault();
        toggle();
      }
      if (e.code === "ArrowLeft") {
        e.preventDefault();
        seekDelta(-5);
      }
      if (e.code === "ArrowRight") {
        e.preventDefault();
        seekDelta(5);
      }
  }, [toggle, seekDelta]);

  const showCtrl = useCallback(() => {
    setCtrlVisible(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setCtrlVisible(false), 3000);
  }, []);

  useEffect(() => {
    if (!playing) {
      setCtrlVisible(true);
      if (hideTimer.current) clearTimeout(hideTimer.current);
    }
  }, [playing]);

  useEffect(() => () => {
    if (hideTimer.current) clearTimeout(hideTimer.current);
  }, []);

  const visible = ctrlVisible || !playing;

  return (
    <div
      className="custom-video-player absolute inset-0 bg-[var(--ui-surface-2)] select-none"
      tabIndex={0}
      aria-label={t.videoPlayer}
      onKeyDown={handleKeyDown}
      onMouseMove={showCtrl}
      onMouseLeave={() => playing && setCtrlVisible(false)}
    >
      <video
        ref={videoRef}
        src={src}
        preload="metadata"
        aria-label={t.videoPlayer}
        className="custom-player-video absolute inset-0 w-full h-full object-contain cursor-pointer"
        onClick={toggle}
      />

      {!playing && dur > 0 && (
        <button
          type="button"
          aria-label={t.play}
          className="custom-player-big-play absolute inset-0 flex items-center justify-center cursor-pointer"
          onClick={toggle}
        >
          <span className="w-14 h-14 rounded-full bg-black/72 flex items-center justify-center border border-white/10 shadow-xl">
            <Play className="w-7 h-7 text-white ml-0.5" fill="white" />
          </span>
        </button>
      )}

      <div
        className={`custom-player-controls absolute bottom-0 inset-x-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pt-10 pb-2 px-3 transition-opacity duration-300 ${
          visible ? "opacity-100" : "opacity-0 pointer-events-none"
        }`}
      >
        <div
          className="custom-player-seek group relative h-5 flex items-center cursor-pointer touch-none focus-within:ring-2 focus-within:ring-white"
        >
          <input
            type="range"
            aria-label={t.seekMedia}
            aria-valuetext={`${fmtTime(time)} / ${fmtTime(dur)}`}
            className="absolute inset-0 z-10 h-full w-full cursor-pointer opacity-0"
            {...seekProps}
          />
          <div className="custom-seek-track w-full h-[3px] rounded-full bg-white/25 overflow-hidden">
            <div
              className="custom-seek-fill h-full bg-white rounded-full"
              style={{ width: `${progress}%` }}
            />
          </div>
          <div
            className="custom-seek-thumb absolute top-1/2 w-3 h-3 rounded-full bg-white shadow-md -translate-y-1/2 -translate-x-1/2 scale-0 group-hover:scale-100 transition-transform"
            style={{ left: `${progress}%` }}
          />
        </div>

        <div className="custom-player-bar flex items-center gap-2 mt-0.5">
          <button
            type="button"
            onClick={toggle}
            aria-label={playing ? t.pause : t.play}
            className="custom-player-play-btn p-1.5 text-white hover:text-white/80 transition-colors"
          >
            {playing ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" fill="white" />}
          </button>
          <span className="custom-player-time text-[11px] font-mono text-white/90 tabular-nums select-none">
            {fmtTime(time)} / {fmtTime(dur)}
          </span>
          <div className="flex-1" />
          <button
            type="button"
            onClick={toggleMute}
            aria-label={muted ? t.unmute : t.mute}
            className="custom-player-volume-btn p-1.5 text-white/80 hover:text-white transition-colors"
          >
            {muted ? <VolumeX className="w-4 h-4" /> : <Volume2 className="w-4 h-4" />}
          </button>
          <button
            type="button"
            onClick={isFullscreen ? onExitFullscreen : onEnterFullscreen}
            aria-label={isFullscreen ? t.exitFullscreen : t.enterFullscreen}
            className="custom-player-fullscreen-btn p-1.5 text-white/80 hover:text-white transition-colors"
          >
            {isFullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
          </button>
        </div>
      </div>
    </div>
  );
}

export function CustomAudioPlayer({
  src,
  onReady,
}: {
  src: string;
  onReady: () => void;
}) {
  const { t } = useSettings();
  const {
    mediaRef: audioRef,
    playing,
    time,
    dur,
    muted,
    toggle,
    toggleMute,
    seekProps,
    progress,
  } = useMediaElementPlayback<HTMLAudioElement>(src, onReady);

  return (
    <div className="custom-audio-player flex h-full min-h-[180px] flex-col justify-center gap-5 border border-[var(--ui-border)] bg-[var(--ui-surface-3)] px-6">
      <audio ref={audioRef} src={src} preload="metadata" aria-label={t.audioPlayer} />
      <div className="audio-player-title text-center text-xs font-semibold uppercase tracking-[0.14em] text-[var(--on-surface-variant)]">
        {t.audioPlayer}
      </div>
      <div
        className="custom-audio-seek group relative h-6 flex items-center cursor-pointer touch-none focus-within:ring-2 focus-within:ring-[var(--primary-color)]"
      >
        <input
          type="range"
          aria-label={t.seekMedia}
          aria-valuetext={`${fmtTime(time)} / ${fmtTime(dur)}`}
          className="absolute inset-0 z-10 h-full w-full cursor-pointer opacity-0"
          {...seekProps}
        />
        <div className="custom-audio-seek-track h-[4px] w-full overflow-hidden rounded-full bg-[var(--ui-hover-strong)]">
          <div
            className="custom-audio-seek-fill h-full rounded-full bg-[var(--primary-color)]"
            style={{ width: `${progress}%` }}
          />
        </div>
        <div
          className="custom-audio-seek-thumb absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[var(--primary-color)] shadow-md ring-2 ring-[var(--surface)] transition-transform group-hover:scale-110"
          style={{ left: `${progress}%` }}
        />
      </div>
      <div className="custom-audio-controls flex items-center gap-3 text-[var(--on-surface)]">
        <button
          type="button"
          onClick={toggle}
          aria-label={playing ? t.pause : t.play}
          className="custom-audio-play-btn rounded-full bg-[var(--primary-color)] p-2.5 text-[var(--primary-foreground)] shadow-xs hover:brightness-105"
        >
          {playing ? <Pause className="h-5 w-5" /> : <Play className="ml-0.5 h-5 w-5" fill="currentColor" />}
        </button>
        <span className="custom-audio-time text-xs font-mono tabular-nums text-[var(--on-surface-variant)]">
          {fmtTime(time)} / {fmtTime(dur)}
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={toggleMute}
          aria-label={muted ? t.unmute : t.mute}
          className="custom-audio-volume-btn rounded-full p-2 text-[var(--on-surface-variant)] hover:bg-[var(--ui-hover)] hover:text-[var(--on-surface)]"
        >
          {muted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
        </button>
      </div>
    </div>
  );
}
