import { useState, useCallback, useEffect } from "react";
import { invoke } from "@/lib/ipc";
import { BackgroundConfig } from "@/types/video";
import { DEFAULT_BUILT_IN_BACKGROUND_ID } from "@/lib/backgroundPresets";

export const RECENT_UPLOADS_KEY = "screen-record-recent-uploads-v1";

function getInitialRecentUploads(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_UPLOADS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((v): v is string => typeof v === "string" && v.length > 0)
      .slice(0, 12);
  } catch {
    return [];
  }
}

interface UseBackgroundUploadParams {
  setBackgroundConfig: (
    updater: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
}

export function useBackgroundUpload({
  setBackgroundConfig,
}: UseBackgroundUploadParams) {
  const [recentUploads, setRecentUploads] = useState<string[]>(
    getInitialRecentUploads,
  );
  const [isBackgroundUploadProcessing, setIsBackgroundUploadProcessing] =
    useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(RECENT_UPLOADS_KEY, JSON.stringify(recentUploads));
    } catch {
      // ignore persistence failures
    }
  }, [recentUploads]);

  const handleBackgroundUpload = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const inputEl = e.currentTarget;
      const file = e.target.files?.[0];
      if (file) {
        setIsBackgroundUploadProcessing(true);
        const img = new Image();

        img.onload = async () => {
          try {
            // Cap backgrounds at 2.5K to ensure instant decode and zero lag.
            // The GPU shader scales it up using object-fit: cover.
            const MAX_DIM = 2560;
            let w = img.naturalWidth;
            let h = img.naturalHeight;
            if (w > MAX_DIM || h > MAX_DIM) {
              const ratio = Math.min(MAX_DIM / w, MAX_DIM / h);
              w = Math.round(w * ratio);
              h = Math.round(h * ratio);
            }

            const canvas = document.createElement("canvas");
            canvas.width = w;
            canvas.height = h;
            const ctx = canvas.getContext("2d");
            if (!ctx) throw new Error("Failed to get 2D canvas context");
            ctx.imageSmoothingEnabled = true;
            ctx.imageSmoothingQuality = "high";
            ctx.drawImage(img, 0, 0, w, h);

            // Convert to JPEG to reduce IPC payload size (backgrounds do not need alpha).
            const dataUrl = canvas.toDataURL("image/jpeg", 0.92);
            const imageUrl = await invoke<string>("save_uploaded_bg_data_url", {
              dataUrl,
            });
            await invoke("prewarm_custom_background", { url: imageUrl });
            setBackgroundConfig((prev) => ({
              ...prev,
              backgroundType: "custom",
              customBackground: imageUrl,
            }));
            setRecentUploads((prev) =>
              [imageUrl, ...prev.filter((v) => v !== imageUrl)].slice(0, 12),
            );
          } catch (err) {
            console.error(
              "[Background] Failed to persist uploaded image:",
              err,
            );
          } finally {
            URL.revokeObjectURL(img.src);
            setIsBackgroundUploadProcessing(false);
            inputEl.value = "";
          }
        };

        img.onerror = () => {
          URL.revokeObjectURL(img.src);
          setIsBackgroundUploadProcessing(false);
          inputEl.value = "";
        };

        img.src = URL.createObjectURL(file);
      }
    },
    [setBackgroundConfig, setRecentUploads],
  );

  const handleRemoveRecentUpload = useCallback((imageUrl: string) => {
    setRecentUploads((prev) => prev.filter((v) => v !== imageUrl));
    setBackgroundConfig((prev) => {
      if (
        prev.backgroundType === "custom" &&
        prev.customBackground === imageUrl
      ) {
        return {
          ...prev,
          backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID,
          customBackground: undefined,
        };
      }
      return prev;
    });
  }, []);

  return {
    recentUploads,
    setRecentUploads,
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  };
}
