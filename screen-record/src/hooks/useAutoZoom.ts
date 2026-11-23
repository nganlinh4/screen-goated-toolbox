import { useState, useCallback } from "react";
import { autoZoomGenerator } from "@/lib/autoZoom";
import {
  VideoSegment,
  MousePosition,
  AutoZoomConfig,
} from "@/types/video";
import { normalizeMousePositionsToVideoSpace } from "@/lib/dynamicCapture";
import {
  getSavedAutoZoomConfig,
  saveAutoZoomConfig,
  saveAutoZoomPref,
} from "./videoStatePreferences";

// ============================================================================
// useAutoZoom
// ============================================================================
// Auto-zoom only edits the active editor segment. Persistence belongs to the
// composition-aware autosave, which writes the active clip to its own slot;
// writing the project root here would put one clip's zoom path on another.
interface UseAutoZoomProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  mousePositions: MousePosition[];
  duration: number;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text" | "subtitles") => void;
}

function withMotionPath(
  segment: VideoSegment,
  mousePositions: MousePosition[],
  video: HTMLVideoElement,
  duration: number,
  config: AutoZoomConfig,
): VideoSegment {
  const videoWidth = video.videoWidth || 0;
  const videoHeight = video.videoHeight || 0;
  const normalizedMousePositions = normalizeMousePositionsToVideoSpace(
    mousePositions, videoWidth, videoHeight,
  );
  return {
    ...segment,
    smoothMotionPath: autoZoomGenerator.generateMotionPath(
      segment, normalizedMousePositions, videoWidth, videoHeight, config,
    ),
    zoomInfluencePoints: [
      { time: 0, value: 1.0 },
      { time: duration, value: 1.0 },
    ],
  };
}

export function useAutoZoom(props: UseAutoZoomProps) {
  const [autoZoomConfig, setAutoZoomConfigState] = useState<AutoZoomConfig>(getSavedAutoZoomConfig);

  /** Generates the motion path off the input event, then applies it to the editor segment. */
  const applyMotionPath = useCallback(
    (config: AutoZoomConfig, onApplied?: () => void) => {
      const { segment, mousePositions, duration } = props;
      const video = props.videoRef.current;
      if (!segment || !mousePositions.length || !video) return;
      setTimeout(() => {
        props.setSegment(withMotionPath(segment, mousePositions, video, duration, config));
        onApplied?.();
      }, 0);
    },
    [props],
  );

  const handleAutoZoomConfigChange = useCallback(
    (config: AutoZoomConfig) => {
      setAutoZoomConfigState(config);
      saveAutoZoomConfig(config);
      if (props.segment?.smoothMotionPath?.length) {
        applyMotionPath(config);
      }
    },
    [applyMotionPath, props.segment],
  );

  const handleAutoZoom = useCallback(() => {
    if (!props.segment) return;

    // Toggle: if auto zoom is already active, clear it
    if (props.segment.smoothMotionPath?.length) {
      saveAutoZoomPref(false);
      props.setSegment({
        ...props.segment,
        smoothMotionPath: [],
        zoomInfluencePoints: [],
      });
      return;
    }

    applyMotionPath(autoZoomConfig, () => {
      saveAutoZoomPref(true);
      props.setActivePanel("zoom");
    });
  }, [applyMotionPath, autoZoomConfig, props]);

  return { handleAutoZoom, autoZoomConfig, handleAutoZoomConfigChange };
}
