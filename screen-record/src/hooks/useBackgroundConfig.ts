import { useRef, useCallback, type MutableRefObject } from "react";
import { BackgroundConfig } from "@/types/video";
import { BACKGROUND_MUTATION_DEBUG } from "@/lib/appUtils";
import { cloneBackgroundConfig, equalBackgroundConfig } from "@/lib/backgroundConfig";

export interface UseBackgroundConfigParams {
  initialConfig: BackgroundConfig;
  setBackgroundConfigState: (
    updater: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  isProjectTransitionRef: MutableRefObject<boolean>;
}

export interface UseBackgroundConfigResult {
  backgroundMutationMetaRef: MutableRefObject<{ at: number; stack: string[] } | null>;
  backgroundConfigBypassRef: MutableRefObject<number>;
  setBackgroundConfig: (
    update: BackgroundConfig | ((value: BackgroundConfig) => BackgroundConfig),
  ) => void;
  applyLoadedBackgroundConfig: (config: BackgroundConfig) => void;
}

export function useBackgroundConfig({
  setBackgroundConfigState,
  isProjectTransitionRef,
}: UseBackgroundConfigParams): UseBackgroundConfigResult {
  const backgroundMutationMetaRef = useRef<{
    at: number;
    stack: string[];
  } | null>(null);
  const backgroundConfigBypassRef = useRef(0);

  const setBackgroundConfig = useCallback(
    (
      update:
        | BackgroundConfig
        | ((value: BackgroundConfig) => BackgroundConfig),
    ) => {
      if (
        isProjectTransitionRef.current &&
        backgroundConfigBypassRef.current <= 0
      ) {
        return;
      }
      if (BACKGROUND_MUTATION_DEBUG) {
        backgroundMutationMetaRef.current = {
          at: Date.now(),
          stack:
            new Error()
              .stack?.split("\n")
              .slice(2, 6)
              .map((line) => line.trim()) ?? [],
        };
      }
      setBackgroundConfigState((prev) => {
        const previous = cloneBackgroundConfig(prev);
        const next =
          typeof update === "function"
            ? (update as (value: BackgroundConfig) => BackgroundConfig)(previous)
            : update;
        const normalizedNext = cloneBackgroundConfig(next);
        return equalBackgroundConfig(prev, normalizedNext) ? prev : normalizedNext;
      });
    },
    [isProjectTransitionRef, setBackgroundConfigState],
  );

  const applyLoadedBackgroundConfig = useCallback(
    (backgroundConfig: BackgroundConfig) => {
      backgroundConfigBypassRef.current += 1;
      try {
        setBackgroundConfig(backgroundConfig);
      } finally {
        backgroundConfigBypassRef.current = Math.max(
          0,
          backgroundConfigBypassRef.current - 1,
        );
      }
    },
    [setBackgroundConfig],
  );

  return {
    backgroundMutationMetaRef,
    backgroundConfigBypassRef,
    setBackgroundConfig,
    applyLoadedBackgroundConfig,
  };
}
