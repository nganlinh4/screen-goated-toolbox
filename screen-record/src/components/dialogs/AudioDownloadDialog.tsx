import { useState } from "react";
import { Button } from "@/components/ui/button";
import { FolderOpen } from "lucide-react";
import { invoke } from "@/lib/ipc";
import { useSettings } from "@/hooks/useSettings";
import type { AudioDownloadFormat } from "@/types/video";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
} from "@/components/ui/Dialog";

interface AudioDownloadDialogProps {
  show: boolean;
  onClose: () => void;
  trackLabel: string;
  format: AudioDownloadFormat;
  onFormatChange: (format: AudioDownloadFormat) => void;
  outputDir: string;
  onOutputDirChange: (dir: string) => void;
  onDownload: () => void;
}

export function AudioDownloadDialog({
  show,
  onClose,
  trackLabel,
  format,
  onFormatChange,
  outputDir,
  onOutputDirChange,
  onDownload,
}: AudioDownloadDialogProps) {
  const { t } = useSettings();
  const [isPickingDir, setIsPickingDir] = useState(false);

  const handleBrowseOutputDir = async () => {
    try {
      setIsPickingDir(true);
      const selected = await invoke<string | null>("pick_export_folder", {
        initialDir: outputDir || null,
      });
      if (selected) onOutputDirChange(selected);
    } catch (error) {
      console.error("[AudioDownload] pick output dir failed:", error);
    } finally {
      setIsPickingDir(false);
    }
  };

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="max-w-[440px]">
        <DialogHeader>
          <DialogTitle>{t.audioDownloadTitle}</DialogTitle>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-4">
          <div className="audio-download-summary ui-inline-note rounded-xl px-3 py-2 text-sm">
            <div className="font-semibold text-[var(--on-surface)]">{trackLabel}</div>
            <div className="mt-1 text-xs text-[var(--on-surface-variant)]">
              {t.audioDownloadDescription}
            </div>
          </div>

          <div className="audio-download-format flex flex-col gap-2">
            <span className="text-xs font-semibold text-[var(--on-surface-variant)]">
              {t.exportFormat}
            </span>
            <div className="audio-download-format-options grid grid-cols-2 gap-2">
              {(["mp3", "wav"] as const).map((value) => (
                <button
                  key={value}
                  type="button"
                  onClick={() => onFormatChange(value)}
                  className="audio-download-format-button ui-action-button h-10 rounded-lg text-sm"
                  data-tone="primary"
                  data-active={format === value ? "true" : "false"}
                  data-emphasis={format === value ? "strong" : "normal"}
                >
                  {value === "mp3" ? t.audioFormatMp3 : t.audioFormatWav}
                </button>
              ))}
            </div>
          </div>

          <div className="audio-download-folder flex flex-col gap-2">
            <span className="text-xs font-semibold text-[var(--on-surface-variant)]">
              {t.saveLocation}
            </span>
            <div className="flex gap-2">
              <input
                readOnly
                value={outputDir}
                className="audio-download-folder-input ui-input min-w-0 flex-1 rounded-lg px-3 py-2 text-xs"
              />
              <Button
                type="button"
                variant="outline"
                onClick={handleBrowseOutputDir}
                disabled={isPickingDir}
                className="h-9 rounded-lg text-xs"
              >
                <FolderOpen className="mr-1.5 h-3.5 w-3.5" />
                {isPickingDir ? t.browsing : t.browse}
              </Button>
            </div>
          </div>
        </DialogBody>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} className="h-9 text-xs">
            {t.cancel}
          </Button>
          <Button
            onClick={onDownload}
            className="audio-download-start ui-action-button h-9 text-xs"
            data-tone="primary"
            data-active="true"
            data-emphasis="strong"
          >
            {t.downloadAudio}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
