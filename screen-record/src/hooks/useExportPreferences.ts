import { useEffect, useState } from "react";
import { setPreviewExportFrameRate } from "@/lib/exportFrameRateStore";
import { createInitialExportOptions } from "./exportHookUtils";

const EXPORT_AUTO_COPY_KEY = "screen-record-export-auto-copy-v1";

export function useExportPreferences() {
  const [exportAutoCopyEnabled, setExportAutoCopyEnabled] = useState(() => {
    try {
      return localStorage.getItem(EXPORT_AUTO_COPY_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [exportOptions, setExportOptions] = useState(createInitialExportOptions);

  useEffect(() => {
    setPreviewExportFrameRate(exportOptions.fps);
  }, [exportOptions.fps]);

  useEffect(() => {
    try {
      localStorage.setItem(
        EXPORT_AUTO_COPY_KEY,
        exportAutoCopyEnabled ? "1" : "0",
      );
    } catch (error) {
      console.warn("Unable to persist export auto-copy preference", error);
    }
  }, [exportAutoCopyEnabled]);

  return {
    exportAutoCopyEnabled,
    setExportAutoCopyEnabled,
    exportOptions,
    setExportOptions,
  };
}
