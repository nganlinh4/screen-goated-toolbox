import { useEffect, useRef } from "react";
import { BackgroundConfig } from "@/types/video";
import {
  BACKGROUND_MUTATION_DEBUG,
  PLAYBACK_RESET_DEBUG,
  summarizeBackgroundConfig,
} from "@/lib/appUtils";
import type { MutableRefObject } from "react";

export interface UseDebugEffectsParams {
  // Background mutation debug
  backgroundConfig: BackgroundConfig;
  isProjectTransitionRef: MutableRefObject<boolean>;
  isSwitchingCompositionClipRef: MutableRefObject<boolean>;
  isCropping: boolean;
  currentProjectId: string | null;
  showProjectsDialog: boolean;
  backgroundMutationMetaRef: MutableRefObject<{
    at: number;
    stack: string[];
  } | null>;
  // Playback reset debug
  currentTime: number;
  currentVideo: string | null;
  isRecording: boolean;
  isLoadingVideo: boolean;
  isPlaying: boolean;
  isVideoReady: boolean;
  hasSequenceChain: boolean;
  loadedClipId: string | null;
  selectedClipId: string | null;
}

export function useDebugEffects({
  backgroundConfig,
  isProjectTransitionRef,
  isSwitchingCompositionClipRef,
  isCropping,
  currentProjectId,
  showProjectsDialog,
  backgroundMutationMetaRef,
  currentTime,
  currentVideo,
  isRecording,
  isLoadingVideo,
  isPlaying,
  isVideoReady,
  hasSequenceChain,
  loadedClipId,
  selectedClipId,
}: UseDebugEffectsParams) {
  const previousBackgroundSummaryRef = useRef<string | null>(
    JSON.stringify(summarizeBackgroundConfig(backgroundConfig)),
  );
  const playbackResetPrevTimeRef = useRef(0);
  const playbackResetLastSignatureRef = useRef<string | null>(null);
  const playbackResetLastAtRef = useRef(0);

  useEffect(() => {
    if (!BACKGROUND_MUTATION_DEBUG) return;
    const nextSummary = summarizeBackgroundConfig(backgroundConfig);
    const nextSerialized = JSON.stringify(nextSummary);
    const prevSerialized = previousBackgroundSummaryRef.current;
    if (!prevSerialized) {
      previousBackgroundSummaryRef.current = nextSerialized;
      return;
    }
    if (prevSerialized === nextSerialized) return;
    const meta = backgroundMutationMetaRef.current;
    const via =
      meta && Date.now() - meta.at < 1500 ? "setter" : "outside-setter";
    console.warn(
      `[BackgroundMutation] ${JSON.stringify({
        projectId: currentProjectId ?? null,
        via,
        prev: JSON.parse(prevSerialized),
        next: nextSummary,
        stack: via === "setter" ? meta?.stack ?? [] : [],
        isProjectTransition: isProjectTransitionRef.current,
        isSwitchingClip: isSwitchingCompositionClipRef.current,
        isCropping,
        showProjectsDialog,
      })}`,
    );
    previousBackgroundSummaryRef.current = nextSerialized;
  }, [
    backgroundConfig,
    isCropping,
    currentProjectId,
    showProjectsDialog,
    backgroundMutationMetaRef,
    isProjectTransitionRef,
    isSwitchingCompositionClipRef,
  ]);

  useEffect(() => {
    if (!PLAYBACK_RESET_DEBUG) return;
    const previousTime = playbackResetPrevTimeRef.current;
    playbackResetPrevTimeRef.current = currentTime;
    if (!currentVideo || isRecording || isLoadingVideo) return;
    if (previousTime <= 0.5 || currentTime > 0.05 || previousTime - currentTime <= 0.5)
      return;
    const payload = {
      reason: "app-state-regressed-to-start",
      previousTime,
      currentTime,
      isPlaying,
      isVideoReady,
      currentProjectId,
      hasSequenceChain,
      loadedClipId,
      selectedClipId,
      switchingClip: isSwitchingCompositionClipRef.current,
    };
    const signature = JSON.stringify({
      previousTime: Number(previousTime.toFixed(3)),
      currentTime: Number(currentTime.toFixed(3)),
      isPlaying,
      currentProjectId,
      loadedClipId,
      selectedClipId,
    });
    const now = Date.now();
    if (
      playbackResetLastSignatureRef.current === signature &&
      now - playbackResetLastAtRef.current < 800
    ) {
      return;
    }
    playbackResetLastSignatureRef.current = signature;
    playbackResetLastAtRef.current = now;
    console.warn("[PlaybackReset]", payload);
  }, [
    currentTime,
    currentVideo,
    hasSequenceChain,
    isLoadingVideo,
    isPlaying,
    isRecording,
    isVideoReady,
    loadedClipId,
    currentProjectId,
    selectedClipId,
    isSwitchingCompositionClipRef,
  ]);
}
