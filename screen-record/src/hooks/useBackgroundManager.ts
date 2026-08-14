import type { MutableRefObject } from "react";
import type { BackgroundConfig, ProjectComposition } from "@/types/video";
import { useBackgroundConfig } from "@/hooks/useBackgroundConfig";
import { useBackgroundUpload } from "@/hooks/useBackgroundUpload";

export interface UseBackgroundManagerParams {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfigState: (
    updater: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  isProjectTransitionRef: MutableRefObject<boolean>;
  composition: ProjectComposition | null;
}

export function useBackgroundManager({
  backgroundConfig,
  setBackgroundConfigState,
  isProjectTransitionRef,
  composition,
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
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  } = useBackgroundUpload({
    backgroundConfig,
    composition,
    setBackgroundConfig,
  });

  return {
    backgroundMutationMetaRef,
    setBackgroundConfig,
    applyLoadedBackgroundConfig,
    recentUploads,
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  };
}
