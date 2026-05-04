import { useEffect, useState, useCallback } from "react";
import { AudioLines, Captions, Film } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";

type DragKind = "video" | "audio" | "subtitle" | "either" | "none";

interface DragDropOverlayProps {
  disabled?: boolean;
  onDropVideo: (file: File) => void;
  onDropAudio?: (file: File) => void;
  onDropAudios?: (files: File[]) => void;
  onDropSubtitleSrt?: (file: File) => void;
}

const AUDIO_EXT_RE = /\.(mp3|wav|m4a|flac|ogg|oga|aac|alac|aiff|aif|wma|opus|mka)$/i;
const SUBTITLE_SRT_EXT_RE = /\.srt$/i;

function fileLooksLikeAudio(file: File): boolean {
  if (file.type.startsWith("audio/")) return true;
  return AUDIO_EXT_RE.test(file.name);
}

function fileLooksLikeSubtitleSrt(file: File): boolean {
  return SUBTITLE_SRT_EXT_RE.test(file.name);
}

function classifyDragItems(items: DataTransferItemList | undefined): DragKind {
  if (!items || items.length === 0) return "none";
  let sawVideo = false;
  let sawAudio = false;
  let sawSubtitle = false;
  for (let i = 0; i < items.length; i += 1) {
    const it = items[i];
    if (it.kind !== "file") continue;
    if (it.type.startsWith("video/")) sawVideo = true;
    else if (it.type.startsWith("audio/")) sawAudio = true;
    else if (it.type === "application/x-subrip") sawSubtitle = true;
  }
  const kinds = [sawVideo, sawAudio, sawSubtitle].filter(Boolean).length;
  if (kinds > 1) return "either";
  if (sawVideo) return "video";
  if (sawAudio) return "audio";
  if (sawSubtitle) return "subtitle";
  // Browsers can leave .type empty for some audio files (e.g. m4a). Defer
  // discrimination until drop, where we have the filename to inspect.
  return "either";
}

export function DragDropOverlay({
  disabled,
  onDropVideo,
  onDropAudio,
  onDropAudios,
  onDropSubtitleSrt,
}: DragDropOverlayProps) {
  const { t } = useSettings();
  const [dragCount, setDragCount] = useState(0);
  const [dragKind, setDragKind] = useState<DragKind>("none");
  const isVisible = dragCount > 0 && !disabled;

  const handleDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    if (disabled) return;
    if (!e.dataTransfer?.types.includes("Files")) return;
    setDragCount(c => c + 1);
    setDragKind(classifyDragItems(e.dataTransfer?.items));
  }, [disabled]);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    setDragCount(c => {
      const next = Math.max(0, c - 1);
      if (next === 0) setDragKind("none");
      return next;
    });
  }, []);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleDrop = useCallback((e: DragEvent) => {
    e.preventDefault();
    setDragCount(0);
    setDragKind("none");
    if (disabled) return;

    const files = e.dataTransfer?.files;
    if (!files) return;

    if (onDropSubtitleSrt) {
      for (let i = 0; i < files.length; i += 1) {
        const file = files[i];
        if (fileLooksLikeSubtitleSrt(file)) {
          onDropSubtitleSrt(file);
          return;
        }
      }
    }

    for (let i = 0; i < files.length; i += 1) {
      const file = files[i];
      if (file.type.startsWith("video/")) {
        onDropVideo(file);
        return;
      }
    }
    if (onDropAudio) {
      const audioFiles = Array.from(files).filter(fileLooksLikeAudio);
      if (audioFiles.length > 1 && onDropAudios) {
        onDropAudios(audioFiles);
        return;
      }
      for (let i = 0; i < files.length; i += 1) {
        const file = files[i];
        if (fileLooksLikeAudio(file)) {
          onDropAudio(file);
          return;
        }
      }
    }
  }, [disabled, onDropVideo, onDropAudio, onDropAudios, onDropSubtitleSrt]);

  useEffect(() => {
    window.addEventListener("dragenter", handleDragEnter);
    window.addEventListener("dragleave", handleDragLeave);
    window.addEventListener("dragover", handleDragOver);
    window.addEventListener("drop", handleDrop);
    return () => {
      window.removeEventListener("dragenter", handleDragEnter);
      window.removeEventListener("dragleave", handleDragLeave);
      window.removeEventListener("dragover", handleDragOver);
      window.removeEventListener("drop", handleDrop);
    };
  }, [handleDragEnter, handleDragLeave, handleDragOver, handleDrop]);

  if (!isVisible) return null;

  const audioCapable = !!onDropAudio;
  const subtitleCapable = !!onDropSubtitleSrt;
  const showAudio = audioCapable && (dragKind === "audio" || dragKind === "either");
  const showSubtitle = subtitleCapable && (dragKind === "subtitle" || dragKind === "either");
  const showVideo = dragKind === "video" || dragKind === "either" || !audioCapable;
  const cta = subtitleCapable && dragKind === "subtitle"
    ? t.dropSubtitleHere
    : audioCapable
    ? dragKind === "audio"
      ? t.dropAudioHere
      : dragKind === "video"
        ? t.dropVideoHere
        : t.dropMediaHere
    : t.dropVideoHere;

  return (
    <div className="drag-drop-overlay fixed inset-0 z-[200] flex items-center justify-center" style={{ background: 'color-mix(in srgb, var(--surface) 70%, transparent)', backdropFilter: 'blur(8px)' }}>
      <div className="drag-drop-content flex flex-col items-center gap-3 p-8 rounded-2xl border-2 border-dashed" style={{ borderColor: 'var(--primary-color)', background: 'color-mix(in srgb, var(--surface) 90%, transparent)' }}>
        <div className="drag-drop-icons flex items-center gap-3">
          {showVideo && <Film className="w-10 h-10" style={{ color: 'var(--primary-color)' }} />}
          {showAudio && <AudioLines className="w-10 h-10" style={{ color: 'var(--primary-color)' }} />}
          {showSubtitle && <Captions className="w-10 h-10" style={{ color: 'var(--primary-color)' }} />}
        </div>
        <p className="text-sm font-medium" style={{ color: 'var(--on-surface)' }}>{cta}</p>
      </div>
    </div>
  );
}
