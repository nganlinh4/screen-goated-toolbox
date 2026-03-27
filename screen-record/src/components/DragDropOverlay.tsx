import { useEffect, useState, useCallback } from "react";
import { Film } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";

interface DragDropOverlayProps {
  disabled?: boolean;
  onDropVideo: (file: File) => void;
}

export function DragDropOverlay({ disabled, onDropVideo }: DragDropOverlayProps) {
  const { t } = useSettings();
  const [dragCount, setDragCount] = useState(0);
  const isVisible = dragCount > 0 && !disabled;

  const handleDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    if (disabled) return;
    if (e.dataTransfer?.types.includes("Files")) {
      setDragCount(c => c + 1);
    }
  }, [disabled]);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    setDragCount(c => Math.max(0, c - 1));
  }, []);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleDrop = useCallback((e: DragEvent) => {
    e.preventDefault();
    setDragCount(0);
    if (disabled) return;

    const files = e.dataTransfer?.files;
    if (!files) return;

    for (let i = 0; i < files.length; i++) {
      if (files[i].type.startsWith("video/")) {
        onDropVideo(files[i]);
        return;
      }
    }
  }, [disabled, onDropVideo]);

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

  return (
    <div className="drag-drop-overlay fixed inset-0 z-[200] flex items-center justify-center" style={{ background: 'color-mix(in srgb, var(--surface) 70%, transparent)', backdropFilter: 'blur(8px)' }}>
      <div className="drag-drop-content flex flex-col items-center gap-3 p-8 rounded-2xl border-2 border-dashed" style={{ borderColor: 'var(--primary-color)', background: 'color-mix(in srgb, var(--surface) 90%, transparent)' }}>
        <Film className="w-10 h-10" style={{ color: 'var(--primary-color)' }} />
        <p className="text-sm font-medium" style={{ color: 'var(--on-surface)' }}>{t.dropVideoHere}</p>
      </div>
    </div>
  );
}
