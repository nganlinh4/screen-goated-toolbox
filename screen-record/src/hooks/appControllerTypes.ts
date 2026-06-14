/**
 * Shared, additive type building blocks for the App.tsx -> AppView -> controller-hook
 * seam. These alias the canonical domain/setter/ref shapes that App.tsx threads through
 * the controller hooks and the AppView component, so each consumer can compose a precise
 * named interface instead of `Record<string, any>`.
 *
 * Typing-only: no runtime, no data-flow change.
 */
import type {
  Dispatch,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";
import type {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import type { useProjects } from "@/hooks/useProjects/index";
import type { useEditorHistory } from "@/hooks/useEditorHistory";
import type { useExport } from "@/hooks/useExport";
import type { useAudioDownload } from "@/hooks/useAudioDownload";

/** History-aware composition setter (value or updater). */
export type CompositionSetter = (
  c:
    | ProjectComposition
    | null
    | ((prev: ProjectComposition | null) => ProjectComposition | null),
) => void;

/** History-aware segment setter (value or updater). */
export type SegmentSetter = (
  s:
    | VideoSegment
    | null
    | ((prev: VideoSegment | null) => VideoSegment | null),
) => void;

/** History-aware background config setter (value or updater). */
export type BackgroundConfigSetter = (
  update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
) => void;

/** Persist callback ref shared across project lifecycle/timeline controllers. */
export type PersistRef = MutableRefObject<
  ((opts?: PersistOptions) => Promise<void>) | null
>;

/** Concrete return shapes of the hooks whose results are forwarded as args. */
export type ProjectsState = ReturnType<typeof useProjects>;
export type EditorHistory = ReturnType<typeof useEditorHistory>;
export type ExportHook = ReturnType<typeof useExport>;
export type AudioDownloadHook = ReturnType<typeof useAudioDownload>;

/**
 * Re-export the most common domain aliases so consuming interfaces only need a single
 * import from this module.
 */
export type {
  BackgroundConfig,
  Dispatch,
  MousePosition,
  MutableRefObject,
  Project,
  ProjectComposition,
  RecordingMode,
  RefObject,
  SetStateAction,
  VideoSegment,
  WebcamConfig,
};
