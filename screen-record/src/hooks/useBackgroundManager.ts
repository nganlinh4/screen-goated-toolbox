import type { MutableRefObject } from "react";
import type { BackgroundConfig } from "@/types/video";
import { useBackgroundConfig } from "@/hooks/useBackgroundConfig";
import { useBackgroundUpload } from "@/hooks/useBackgroundUpload";

export interface UseBackgroundManagerParams {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfigState: (
    updater: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  isProjectTransitionRef: MutableRefObject<boolean>;
}

export function useBackgroundManager({
  backgroundConfig,
  setBackgroundConfigState,
  isProjectTransitionRef,
}: UseBackgroundManagerParams) {
  const {
    backgroundMutationMetaRef,
    setBackgroundConfig,
    applyLoadedBackgroundConfig,
  } = useBackgroundConfig({
    initialConfig: backgroundConfig,
    setBackgroundConfigState,
    isProjectTransitionRef,
  });

  const {
    recentUploads,
    recentUploadLimit,
    setRecentUploadLimit,
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  } = useBackgroundUpload({ backgroundConfig, setBackgroundConfig });

  return {
    backgroundMutationMetaRef,
    setBackgroundConfig,
    applyLoadedBackgroundConfig,
    recentUploads,
    recentUploadLimit,
    setRecentUploadLimit,
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  };
}
