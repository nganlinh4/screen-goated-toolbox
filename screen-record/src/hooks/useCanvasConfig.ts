import {
  useCallback,
  useEffect,
  useMemo,
  type MutableRefObject,
} from "react";
import { BackgroundConfig, ProjectComposition, VideoSegment } from "@/types/video";
import type { ActivePanel } from "@/components/sidepanel/index";
import { getCompositionAutoSourceClipId } from "@/lib/projectComposition";
import { getCanvasRatioDimensions } from "@/lib/appUtils";

export interface UseCanvasConfigParams {
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment) => void;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  composition: ProjectComposition | null;
  activeClipId: string | null | undefined;
  videoRef: MutableRefObject<HTMLVideoElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  setActivePanel: (panel: ActivePanel) => void;
  setZoomFactor: (factor: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  isCropping: boolean;
  setIsCropping: (value: boolean) => void;
  isPlaying: boolean;
  handleTogglePlayPause: () => void;
  isVideoReady: boolean;
}

export function useCanvasConfig({
  segment,
  setSegment,
  backgroundConfig,
  setBackgroundConfig,
  composition,
  activeClipId,
  videoRef,
  canvasRef,
  setActivePanel,
  setZoomFactor,
  setEditingKeyframeId,
  isCropping,
  setIsCropping,
  isPlaying,
  handleTogglePlayPause,
  isVideoReady,
}: UseCanvasConfigParams) {

  const getAutoCanvasSelectionConfig = useCallback(() => {
    const crop = segment?.crop ?? { x: 0, y: 0, width: 1, height: 1 };
    // Only use videoWidth/videoHeight from a loaded video; never fall back to
    // canvasRef dimensions (HTML canvas defaults to 300x150 which leaks as a
    // bogus resolution).
    const rawVidW = videoRef.current?.videoWidth || 0;
    const rawVidH = videoRef.current?.videoHeight || 0;
    const sourceWidth = rawVidW > 0 ? rawVidW : 0;
    const sourceHeight = rawVidH > 0 ? rawVidH : 0;
    const derivedWidth =
      sourceWidth > 0 ? Math.max(2, Math.round(sourceWidth * crop.width)) : undefined;
    const derivedHeight =
      sourceHeight > 0
        ? Math.max(2, Math.round(sourceHeight * crop.height))
        : undefined;
    return {
      canvasMode: "auto" as const,
      canvasWidth: derivedWidth,
      canvasHeight: derivedHeight,
      autoSourceClipId:
        activeClipId ??
        getCompositionAutoSourceClipId(composition) ??
        composition?.focusedClipId ??
        composition?.selectedClipId ??
        "root",
    };
  }, [
    activeClipId,
    canvasRef,
    composition,
    segment?.crop,
    videoRef,
  ]);

  const customCanvasAutoConfig = getAutoCanvasSelectionConfig();
  // In auto mode with a ready video, derive dimensions from the live video
  // so stale persisted values (e.g. 300x150 from a previous session) never
  // cause a visible blink before the useEffect corrects them.
  const liveAutoW = backgroundConfig.canvasMode === "auto" && isVideoReady
    ? (customCanvasAutoConfig.canvasWidth ?? videoRef.current?.videoWidth)
    : undefined;
  const liveAutoH = backgroundConfig.canvasMode === "auto" && isVideoReady
    ? (customCanvasAutoConfig.canvasHeight ?? videoRef.current?.videoHeight)
    : undefined;
  const customCanvasBaseDimensions = {
    width: Math.max(
      2,
      liveAutoW ??
        backgroundConfig.canvasWidth ??
        customCanvasAutoConfig.canvasWidth ??
        (videoRef.current?.videoWidth || undefined) ??
        1920,
    ),
    height: Math.max(
      2,
      liveAutoH ??
        backgroundConfig.canvasHeight ??
        customCanvasAutoConfig.canvasHeight ??
        (videoRef.current?.videoHeight || undefined) ??
        1080,
    ),
  };

  const applyCustomCanvasDimensions = useCallback(
    (canvasWidth: number, canvasHeight: number) => {
      setBackgroundConfig((prev) => ({
        ...prev,
        canvasMode: "custom",
        canvasWidth,
        canvasHeight,
        autoCanvasSourceId: null,
      }));
    },
    [setBackgroundConfig],
  );

  const handleActivateCustomCanvas = useCallback(() => {
    applyCustomCanvasDimensions(
      customCanvasBaseDimensions.width,
      customCanvasBaseDimensions.height,
    );
  }, [
    applyCustomCanvasDimensions,
    customCanvasBaseDimensions.height,
    customCanvasBaseDimensions.width,
  ]);

  const handleApplyCanvasRatioPreset = useCallback(
    (ratioWidth: number, ratioHeight: number) => {
      const nextDimensions = getCanvasRatioDimensions(
        customCanvasBaseDimensions.width,
        customCanvasBaseDimensions.height,
        ratioWidth,
        ratioHeight,
      );
      applyCustomCanvasDimensions(nextDimensions.width, nextDimensions.height);
    },
    [
      applyCustomCanvasDimensions,
      customCanvasBaseDimensions.height,
      customCanvasBaseDimensions.width,
    ],
  );

  const handleCancelCrop = useCallback(() => {
    setIsCropping(false);
    setActivePanel("background");
    setZoomFactor(1.0);
    setEditingKeyframeId(null);
  }, [setZoomFactor, setEditingKeyframeId, setActivePanel]);

  const handleApplyCrop = useCallback(
    (crop: VideoSegment["crop"]) => {
      if (segment && crop) {
        setSegment({
          ...segment,
          crop,
        });
      }
      handleCancelCrop();
    },
    [segment, setSegment, handleCancelCrop],
  );

  // When canvasMode is "auto", keep canvasWidth/canvasHeight in sync with the
  // current crop. This covers both the apply-crop path and undo/redo — undoing
  // a crop changes segment.crop which triggers this effect and re-syncs the
  // canvas dimensions automatically.
  const cropX = segment?.crop?.x;
  const cropY = segment?.crop?.y;
  const cropW = segment?.crop?.width;
  const cropH = segment?.crop?.height;
  useEffect(() => {
    if (backgroundConfig.canvasMode !== "auto") return;
    const crop = segment?.crop ?? { x: 0, y: 0, width: 1, height: 1 };
    const sourceWidth = videoRef.current?.videoWidth || 0;
    const sourceHeight = videoRef.current?.videoHeight || 0;
    if (!sourceWidth || !sourceHeight) return;
    const derivedWidth = Math.max(2, Math.round(sourceWidth * crop.width));
    const derivedHeight = Math.max(2, Math.round(sourceHeight * crop.height));
    if (
      derivedWidth === backgroundConfig.canvasWidth &&
      derivedHeight === backgroundConfig.canvasHeight
    ) return;
    setBackgroundConfig((prev) => ({
      ...prev,
      canvasWidth: derivedWidth,
      canvasHeight: derivedHeight,
    }));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cropX, cropY, cropW, cropH, backgroundConfig.canvasMode, isVideoReady]);

  const handleToggleCrop = useCallback(() => {
    if (isCropping) {
      handleCancelCrop();
    } else {
      setIsCropping(true);
      if (isPlaying) handleTogglePlayPause();
    }
  }, [
    isCropping,
    isPlaying,
    handleTogglePlayPause,
    handleCancelCrop,
  ]);

  const hasAppliedCrop = useMemo(() => {
    const crop = segment?.crop;
    if (!crop) return false;
    return (
      Math.abs(crop.x) > 0.0005 ||
      Math.abs(crop.y) > 0.0005 ||
      Math.abs(crop.width - 1) > 0.0005 ||
      Math.abs(crop.height - 1) > 0.0005
    );
  }, [
    segment?.crop?.x,
    segment?.crop?.y,
    segment?.crop?.width,
    segment?.crop?.height,
  ]);

  return {
    getAutoCanvasSelectionConfig,
    customCanvasBaseDimensions,
    applyCustomCanvasDimensions,
    handleActivateCustomCanvas,
    handleApplyCanvasRatioPreset,
    handleCancelCrop,
    handleApplyCrop,
    handleToggleCrop,
    hasAppliedCrop,
  };
}
