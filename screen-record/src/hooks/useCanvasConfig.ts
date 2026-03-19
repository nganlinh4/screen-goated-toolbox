import {
  useCallback,
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
}: UseCanvasConfigParams) {

  const getAutoCanvasSelectionConfig = useCallback(() => {
    const crop = segment?.crop ?? { x: 0, y: 0, width: 1, height: 1 };
    const sourceWidth =
      videoRef.current?.videoWidth || canvasRef.current?.width || 0;
    const sourceHeight =
      videoRef.current?.videoHeight || canvasRef.current?.height || 0;
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
  const customCanvasBaseDimensions = {
    width: Math.max(
      2,
      backgroundConfig.canvasWidth ??
        customCanvasAutoConfig.canvasWidth ??
        videoRef.current?.videoWidth ??
        canvasRef.current?.width ??
        1920,
    ),
    height: Math.max(
      2,
      backgroundConfig.canvasHeight ??
        customCanvasAutoConfig.canvasHeight ??
        videoRef.current?.videoHeight ??
        canvasRef.current?.height ??
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
