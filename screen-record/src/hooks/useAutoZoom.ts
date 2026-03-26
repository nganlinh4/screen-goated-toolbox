import { useState, useCallback } from "react";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { projectManager } from "@/lib/projectManager";
import { autoZoomGenerator } from "@/lib/autoZoom";
import {
  BackgroundConfig,
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
interface UseAutoZoomProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  mousePositions: MousePosition[];
  duration: number;
  currentProjectId: string | null;
  backgroundConfig: BackgroundConfig;
  loadProjects: () => Promise<void>;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text") => void;
}

export function useAutoZoom(props: UseAutoZoomProps) {
  const [autoZoomConfig, setAutoZoomConfigState] = useState<AutoZoomConfig>(getSavedAutoZoomConfig);

  const regenerateMotionPath = useCallback(
    (config: AutoZoomConfig) => {
      if (!props.segment || !props.mousePositions.length || !props.videoRef.current) return;
      const seg = props.segment;
      const vid = props.videoRef.current;
      const vidW = vid.videoWidth || 0;
      const vidH = vid.videoHeight || 0;
      const mp = props.mousePositions;
      const dur = props.duration;
      const projId = props.currentProjectId;
      const bgCfg = props.backgroundConfig;

      // Yield to UI thread before heavy computation
      setTimeout(() => {
        const normalizedMousePositions = normalizeMousePositionsToVideoSpace(mp, vidW, vidH);
        const motionPath = autoZoomGenerator.generateMotionPath(
          seg, normalizedMousePositions, vidW, vidH, config,
        );
        const newSegment: VideoSegment = {
          ...seg,
          smoothMotionPath: motionPath,
          zoomInfluencePoints: [
            { time: 0, value: 1.0 },
            { time: dur, value: 1.0 },
          ],
        };
        props.setSegment(newSegment);
        if (projId) {
          projectManager
            .updateProject(projId, {
              segment: newSegment,
              backgroundConfig: cloneBackgroundConfig(bgCfg),
              mousePositions: mp,
            })
            .then(() => props.loadProjects());
        }
      }, 0);
    },
    [props],
  );

  const handleAutoZoomConfigChange = useCallback(
    (config: AutoZoomConfig) => {
      setAutoZoomConfigState(config);
      saveAutoZoomConfig(config);
      // Regenerate if auto zoom is currently active
      const hasAutoPath =
        props.segment?.smoothMotionPath &&
        props.segment.smoothMotionPath.length > 0;
      if (hasAutoPath) {
        regenerateMotionPath(config);
      }
    },
    [props.segment, regenerateMotionPath],
  );

  const handleAutoZoom = useCallback(() => {
    if (!props.segment) return;

    // Toggle: if auto zoom is already active, clear it
    const hasAutoPath =
      props.segment.smoothMotionPath &&
      props.segment.smoothMotionPath.length > 0;
    if (hasAutoPath) {
      saveAutoZoomPref(false);
      const newSegment: VideoSegment = {
        ...props.segment,
        smoothMotionPath: [],
        zoomInfluencePoints: [],
      };
      props.setSegment(newSegment);
      if (props.currentProjectId) {
        projectManager
          .updateProject(props.currentProjectId, {
            segment: newSegment,
            backgroundConfig: cloneBackgroundConfig(props.backgroundConfig),
            mousePositions: props.mousePositions,
          })
          .then(() => props.loadProjects());
      }
      return;
    }

    if (!props.mousePositions.length || !props.videoRef.current) return;

    // Yield to UI thread before heavy computation
    const seg = props.segment;
    const vid = props.videoRef.current;
    const vidW = vid.videoWidth || 0;
    const vidH = vid.videoHeight || 0;
    const mp = props.mousePositions;
    const cfg = autoZoomConfig;
    const dur = props.duration;
    const projId = props.currentProjectId;
    const bgCfg = props.backgroundConfig;

    setTimeout(() => {
      const normalizedMousePositions = normalizeMousePositionsToVideoSpace(mp, vidW, vidH);
      const motionPath = autoZoomGenerator.generateMotionPath(
        seg,
        normalizedMousePositions,
        vidW,
        vidH,
        cfg,
      );

      saveAutoZoomPref(true);
      const newSegment: VideoSegment = {
        ...seg,
        smoothMotionPath: motionPath,
        zoomInfluencePoints: [
          { time: 0, value: 1.0 },
          { time: dur, value: 1.0 },
        ],
      };

      props.setSegment(newSegment);
      if (projId) {
        projectManager
          .updateProject(projId, {
            segment: newSegment,
            backgroundConfig: cloneBackgroundConfig(bgCfg),
            mousePositions: mp,
          })
          .then(() => props.loadProjects());
      }
      props.setActivePanel("zoom");
    }, 0);
  }, [props, autoZoomConfig]);

  return { handleAutoZoom, autoZoomConfig, handleAutoZoomConfigChange };
}
