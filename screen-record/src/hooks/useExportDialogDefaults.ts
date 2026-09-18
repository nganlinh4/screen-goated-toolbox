import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import type { ExportOptions } from "@/types/video";
import { getExportDefaults, resolvePreferredResolution, saveExportDefaults, type ExportDefaults } from "@/lib/exportPreferences";

export function useExportDialogDefaults(
  show: boolean, format: "mp4" | "gif", baseW: number, baseH: number,
  setExportOptions: Dispatch<SetStateAction<ExportOptions>>,
) {
  const [preferences, setPreferences] = useState(getExportDefaults);
  const updateDefaults = (patch: Partial<ExportDefaults>) => {
    setPreferences(saveExportDefaults(patch));
  };
  useEffect(() => {
    if (!show) return;
    const { width, height } = resolvePreferredResolution(preferences, format, baseW, baseH);
    setExportOptions((prev) => prev.width === width && prev.height === height
      ? prev : { ...prev, width, height });
  }, [show, format, baseW, baseH, preferences, setExportOptions]);
  return updateDefaults;
}
