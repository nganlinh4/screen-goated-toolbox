import { useState, useEffect, useRef, useCallback } from "react";
import {
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Volume2,
  VolumeX,
} from '@/components/ui/MaterialIcon';
import { formatTime as fmtTime } from "@/utils/helpers";

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
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [dur, setDur] = useState(0);
  const [muted, setMuted] = useState(false);
  const [ctrlVisible, setCtrlVisible] = useState(true);
  const hideTimer = useRef<ReturnType<typeof setTimeout>>();
  const scrubbing = useRef(false);

  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const onMeta = () => {
      setDur(v.duration);
      onReady();
    };
    const onCanPlay = () => onReady();
    const onTime = () => {
      if (!scrubbing.current) setTime(v.currentTime);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    v.addEventListener("loadedmetadata", onMeta);
    v.addEventListener("canplay", onCanPlay);
    v.addEventListener("timeupdate", onTime);
    v.addEventListener("play", onPlay);
    v.addEventListener("pause", onPause);
    v.addEventListener("ended", onPause);
    return () => {
      v.removeEventListener("loadedmetadata", onMeta);
      v.removeEventListener("canplay", onCanPlay);
      v.removeEventListener("timeupdate", onTime);
      v.removeEventListener("play", onPlay);
      v.removeEventListener("pause", onPause);
      v.removeEventListener("ended", onPause);
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

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
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
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
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

  const seekTo = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const v = videoRef.current;
    if (v && dur > 0) {
      v.currentTime = pct * dur;
      setTime(pct * dur);
    }
  };

  const progress = dur > 0 ? (time / dur) * 100 : 0;
  const visible = ctrlVisible || !playing;

  return (
    <div
      className="custom-video-player absolute inset-0 bg-[var(--ui-surface-2)] select-none"
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

      {!playing && dur > 0 && (
        <div
          className="custom-player-big-play absolute inset-0 flex items-center justify-center cursor-pointer"
          onClick={toggle}
        >
          <div className="w-14 h-14 rounded-full bg-black/72 flex items-center justify-center border border-white/10 shadow-xl">
            <Play className="w-7 h-7 text-white ml-0.5" fill="white" />
          </div>
        </div>
      )}

      <div
        className={`custom-player-controls absolute bottom-0 inset-x-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pt-10 pb-2 px-3 transition-opacity duration-300 ${
          visible ? "opacity-100" : "opacity-0 pointer-events-none"
        }`}
      >
        <div
          className="custom-player-seek group relative h-5 flex items-center cursor-pointer touch-none"
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            scrubbing.current = true;
            seekTo(e);
          }}
          onPointerMove={(e) => scrubbing.current && seekTo(e)}
          onPointerUp={() => {
            scrubbing.current = false;
          }}
        >
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
            onClick={toggle}
            className="custom-player-play-btn p-1.5 text-white hover:text-white/80 transition-colors"
          >
            {playing ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" fill="white" />}
          </button>
          <span className="custom-player-time text-[11px] font-mono text-white/90 tabular-nums select-none">
            {fmtTime(time)} / {fmtTime(dur)}
          </span>
          <div className="flex-1" />
          <button
            onClick={() => {
              const v = videoRef.current;
              if (v) {
                v.muted = !v.muted;
                setMuted(!muted);
              }
            }}
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

export function CustomAudioPlayer({
  src,
  onReady,
}: {
  src: string;
  onReady: () => void;
}) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [dur, setDur] = useState(0);
  const [muted, setMuted] = useState(false);
  const scrubbing = useRef(false);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const onMeta = () => {
      setDur(audio.duration);
      onReady();
    };
    const onCanPlay = () => onReady();
    const onTime = () => {
      if (!scrubbing.current) setTime(audio.currentTime);
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    audio.addEventListener("loadedmetadata", onMeta);
    audio.addEventListener("canplay", onCanPlay);
    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("ended", onPause);
    return () => {
      audio.removeEventListener("loadedmetadata", onMeta);
      audio.removeEventListener("canplay", onCanPlay);
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("ended", onPause);
    };
  }, [onReady, src]);

  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (audio) audio.paused ? audio.play() : audio.pause();
  }, []);

  const seekTo = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const audio = audioRef.current;
    if (audio && dur > 0) {
      audio.currentTime = pct * dur;
      setTime(pct * dur);
    }
  };

  const progress = dur > 0 ? (time / dur) * 100 : 0;

  return (
    <div className="custom-audio-player flex h-full min-h-[180px] flex-col justify-center gap-5 border border-[var(--ui-border)] bg-[var(--ui-surface-3)] px-6">
      <audio ref={audioRef} src={src} preload="metadata" />
      <div className="audio-player-title text-center text-xs font-semibold uppercase tracking-[0.14em] text-[var(--on-surface-variant)]">
        Audio
      </div>
      <div
        className="custom-audio-seek group relative h-6 flex items-center cursor-pointer touch-none"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          scrubbing.current = true;
          seekTo(e);
        }}
        onPointerMove={(e) => scrubbing.current && seekTo(e)}
        onPointerUp={() => {
          scrubbing.current = false;
        }}
      >
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
          onClick={toggle}
          className="custom-audio-play-btn rounded-full bg-[var(--primary-color)] p-2.5 text-[var(--primary-foreground)] shadow-sm hover:brightness-105"
        >
          {playing ? <Pause className="h-5 w-5" /> : <Play className="ml-0.5 h-5 w-5" fill="currentColor" />}
        </button>
        <span className="custom-audio-time text-xs font-mono tabular-nums text-[var(--on-surface-variant)]">
          {fmtTime(time)} / {fmtTime(dur)}
        </span>
        <div className="flex-1" />
        <button
          onClick={() => {
            const audio = audioRef.current;
            if (audio) {
              audio.muted = !audio.muted;
              setMuted(audio.muted);
            }
          }}
          className="custom-audio-volume-btn rounded-full p-2 text-[var(--on-surface-variant)] hover:bg-[var(--ui-hover)] hover:text-[var(--on-surface)]"
        >
          {muted ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
        </button>
      </div>
    </div>
  );
}
