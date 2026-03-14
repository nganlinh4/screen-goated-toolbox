import type {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectCanvasConfig,
  ProjectComposition,
  ProjectCompositionClip,
  ProjectCompositionMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";

function cloneSegment(segment: VideoSegment): VideoSegment {
  return JSON.parse(JSON.stringify(segment)) as VideoSegment;
}

function cloneBackgroundConfig(
  backgroundConfig: BackgroundConfig,
): BackgroundConfig {
  return JSON.parse(JSON.stringify(backgroundConfig)) as BackgroundConfig;
}

function cloneWebcamConfig(
  webcamConfig: WebcamConfig | null | undefined,
): WebcamConfig | undefined {
  if (!webcamConfig) return undefined;
  return JSON.parse(JSON.stringify(webcamConfig)) as WebcamConfig;
}

function cloneMousePositions(mousePositions: MousePosition[]): MousePosition[] {
  return JSON.parse(JSON.stringify(mousePositions)) as MousePosition[];
}

function cloneCanvasConfig(
  canvasConfig: ProjectCanvasConfig | null | undefined,
): ProjectCanvasConfig {
  return {
    canvasMode: canvasConfig?.canvasMode ?? "auto",
    canvasWidth: canvasConfig?.canvasWidth,
    canvasHeight: canvasConfig?.canvasHeight,
    autoSourceClipId: canvasConfig?.autoSourceClipId ?? null,
  };
}

export function getEffectiveCompositionMode(
  composition: Pick<ProjectComposition, "clips" | "mode"> | null | undefined,
): ProjectCompositionMode {
  if (!composition) return "separate";
  // Single-clip projects must never behave as hidden "unified" compositions,
  // otherwise live background/cursor edits can stop persisting to the root clip.
  return composition.clips.length <= 1 ? "separate" : composition.mode;
}

export function extractCanvasConfig(
  backgroundConfig: BackgroundConfig,
): ProjectCanvasConfig {
  return {
    canvasMode: backgroundConfig.canvasMode ?? "auto",
    canvasWidth: backgroundConfig.canvasWidth,
    canvasHeight: backgroundConfig.canvasHeight,
    autoSourceClipId: backgroundConfig.autoCanvasSourceId ?? null,
  };
}

export function applyCanvasConfig(
  backgroundConfig: BackgroundConfig,
  canvasConfig: ProjectCanvasConfig | null | undefined,
): BackgroundConfig {
  return {
    ...backgroundConfig,
    canvasMode: canvasConfig?.canvasMode ?? "auto",
    canvasWidth: canvasConfig?.canvasWidth,
    canvasHeight: canvasConfig?.canvasHeight,
    autoCanvasSourceId: canvasConfig?.autoSourceClipId ?? null,
  };
}

export function applyUnifiedPresentationConfig(
  clipBackgroundConfig: BackgroundConfig,
  presentationConfig: BackgroundConfig | null | undefined,
  canvasConfig: ProjectCanvasConfig | null | undefined,
): BackgroundConfig {
  return applyCanvasConfig(
    {
      ...clipBackgroundConfig,
      ...(presentationConfig ?? {}),
    },
    canvasConfig,
  );
}

export function normalizeCompositionClipToCanvas(
  clip: ProjectCompositionClip,
  canvasConfig: ProjectCanvasConfig | null | undefined,
): ProjectCompositionClip {
  return {
    ...clip,
    backgroundConfig: applyCanvasConfig(clip.backgroundConfig, canvasConfig),
  };
}

export function syncCompositionCanvasConfig(
  composition: ProjectComposition,
  canvasConfig: ProjectCanvasConfig | null | undefined,
): ProjectComposition {
  const normalizedCanvasConfig = cloneCanvasConfig(canvasConfig);
  return {
    ...composition,
    globalCanvasConfig: normalizedCanvasConfig,
    clips: composition.clips.map((clip) =>
      normalizeCompositionClipToCanvas(clip, normalizedCanvasConfig),
    ),
    globalPresentationConfig: composition.globalPresentationConfig
      ? applyCanvasConfig(
          composition.globalPresentationConfig,
          normalizedCanvasConfig,
        )
      : composition.globalPresentationConfig,
    globalBackgroundConfig: composition.globalBackgroundConfig
      ? applyCanvasConfig(
          composition.globalBackgroundConfig,
          normalizedCanvasConfig,
        )
      : composition.globalBackgroundConfig,
  };
}

export function createRootCompositionClip(
  project: Pick<
    Project,
    | "id"
    | "name"
    | "duration"
    | "thumbnail"
    | "segment"
    | "backgroundConfig"
    | "webcamConfig"
    | "mousePositions"
    | "recordingMode"
    | "rawVideoPath"
    | "rawMicAudioPath"
    | "rawWebcamVideoPath"
  >,
): ProjectCompositionClip {
  return {
    id: "root",
    role: "root",
    name: project.name,
    duration: project.duration ?? project.segment.trimEnd,
    sourceProjectId: project.id,
    sourceProjectName: project.name,
    thumbnail: project.thumbnail,
    segment: cloneSegment(project.segment),
    backgroundConfig: cloneBackgroundConfig(project.backgroundConfig),
    webcamConfig: cloneWebcamConfig(project.webcamConfig),
    mousePositions: cloneMousePositions(project.mousePositions),
    recordingMode: project.recordingMode,
    rawVideoPath: project.rawVideoPath,
    rawMicAudioPath: project.rawMicAudioPath,
    rawWebcamVideoPath: project.rawWebcamVideoPath,
  };
}

export function createCompositionSnapshotClip(
  project: Project,
): ProjectCompositionClip {
  return {
    id: crypto.randomUUID(),
    role: "snapshot",
    name: project.name,
    duration: project.duration ?? project.segment.trimEnd,
    sourceProjectId: project.id,
    sourceProjectName: project.name,
    thumbnail: project.thumbnail,
    segment: cloneSegment(project.segment),
    backgroundConfig: cloneBackgroundConfig(project.backgroundConfig),
    webcamConfig: cloneWebcamConfig(project.webcamConfig),
    mousePositions: cloneMousePositions(project.mousePositions),
    recordingMode: project.recordingMode,
    rawVideoPath: project.rawVideoPath,
    rawMicAudioPath: project.rawMicAudioPath,
    rawWebcamVideoPath: project.rawWebcamVideoPath,
  };
}

export function ensureProjectComposition(
  project: Pick<
    Project,
    | "id"
    | "name"
    | "duration"
    | "thumbnail"
    | "segment"
    | "backgroundConfig"
    | "webcamConfig"
    | "mousePositions"
    | "recordingMode"
    | "rawVideoPath"
    | "rawMicAudioPath"
    | "rawWebcamVideoPath"
    | "composition"
  >,
): ProjectComposition {
  const rootClip = createRootCompositionClip(project);
  const existing = project.composition;
  const clips = existing?.clips?.length
    ? existing.clips.map((clip) =>
        clip.role === "root" ? { ...clip, ...rootClip, id: clip.id } : clip,
      )
    : [rootClip];
  const hasRoot = clips.some((clip) => clip.role === "root");
  const normalizedClips = hasRoot ? clips : [rootClip, ...clips];
  const selectedClipId =
    existing?.selectedClipId &&
    normalizedClips.some((clip) => clip.id === existing.selectedClipId)
      ? existing.selectedClipId
      : rootClip.id;
  const focusedClipId =
    existing?.focusedClipId &&
    normalizedClips.some((clip) => clip.id === existing.focusedClipId)
      ? existing.focusedClipId
      : selectedClipId;
  const canvasConfig = cloneCanvasConfig(
    existing?.globalCanvasConfig ??
      extractCanvasConfig(project.backgroundConfig),
  );
  const hasValidAutoSource =
    !!canvasConfig.autoSourceClipId &&
    normalizedClips.some((clip) => clip.id === canvasConfig.autoSourceClipId);
  const autoSourceClipId =
    canvasConfig.canvasMode === "auto"
      ? hasValidAutoSource
        ? canvasConfig.autoSourceClipId
        : rootClip.id
      : null;
  const mode: ProjectCompositionMode =
    normalizedClips.length <= 1 ? "separate" : (existing?.mode ?? "separate");
  return syncCompositionCanvasConfig(
    {
      mode,
      selectedClipId,
      focusedClipId,
      clips: normalizedClips,
      unifiedSourceClipId:
        existing?.unifiedSourceClipId &&
        normalizedClips.some((clip) => clip.id === existing.unifiedSourceClipId)
          ? existing.unifiedSourceClipId
          : selectedClipId,
      globalCanvasConfig: canvasConfig,
      globalPresentationConfig: existing?.globalPresentationConfig
        ? cloneBackgroundConfig(existing.globalPresentationConfig)
        : existing?.globalBackgroundConfig
          ? cloneBackgroundConfig(existing.globalBackgroundConfig)
          : cloneBackgroundConfig(project.backgroundConfig),
      globalSegment: existing?.globalSegment
        ? cloneSegment(existing.globalSegment)
        : cloneSegment(project.segment),
      globalBackgroundConfig: existing?.globalBackgroundConfig
        ? cloneBackgroundConfig(existing.globalBackgroundConfig)
        : cloneBackgroundConfig(project.backgroundConfig),
    },
    {
      ...canvasConfig,
      autoSourceClipId,
    },
  );
}

export function getCompositionResolvedBackgroundConfig(
  composition: ProjectComposition,
  clipId: string,
): BackgroundConfig | null {
  const clip = getCompositionClip(composition, clipId);
  if (!clip) return null;
  if (getEffectiveCompositionMode(composition) === "unified") {
    return applyUnifiedPresentationConfig(
      clip.backgroundConfig,
      composition.globalPresentationConfig ??
        composition.globalBackgroundConfig,
      composition.globalCanvasConfig,
    );
  }
  return applyCanvasConfig(
    clip.backgroundConfig,
    composition.globalCanvasConfig,
  );
}

export function getCompositionClipIndex(
  composition: ProjectComposition | null | undefined,
  clipId: string | null | undefined,
): number {
  if (!composition || !clipId) return -1;
  return composition.clips.findIndex((clip) => clip.id === clipId);
}

export function getCompositionRootClipId(
  composition: ProjectComposition | null | undefined,
): string | null {
  if (!composition) return null;
  return composition.clips.find((clip) => clip.role === "root")?.id ?? null;
}

export function getCompositionAutoSourceClipId(
  composition: ProjectComposition | null | undefined,
): string | null {
  if (!composition) return null;
  const configuredClipId = composition.globalCanvasConfig?.autoSourceClipId;
  if (
    configuredClipId &&
    composition.clips.some((clip) => clip.id === configuredClipId)
  ) {
    return configuredClipId;
  }
  return getCompositionRootClipId(composition);
}

export function getCompositionAdjacentClipIds(
  composition: ProjectComposition | null | undefined,
  clipId: string | null | undefined,
): { previousClipId: string | null; nextClipId: string | null } {
  const clipIndex = getCompositionClipIndex(composition, clipId);
  if (!composition || clipIndex < 0) {
    return {
      previousClipId: null,
      nextClipId: null,
    };
  }
  return {
    previousClipId: composition.clips[clipIndex - 1]?.id ?? null,
    nextClipId: composition.clips[clipIndex + 1]?.id ?? null,
  };
}

export function withCompositionSelection(
  composition: ProjectComposition,
  clipId: string,
): ProjectComposition {
  return {
    ...composition,
    selectedClipId: clipId,
    focusedClipId: clipId,
  };
}

export function updateCompositionClip(
  composition: ProjectComposition,
  clipId: string,
  updates: Partial<ProjectCompositionClip>,
): ProjectComposition {
  return {
    ...composition,
    clips: composition.clips.map((clip) =>
      clip.id === clipId ? { ...clip, ...updates } : clip,
    ),
  };
}

export function insertCompositionClip(
  composition: ProjectComposition,
  targetClipId: string | null,
  placement: "before" | "after",
  clip: ProjectCompositionClip,
): ProjectComposition {
  const clips = [...composition.clips];
  const targetIndex = targetClipId
    ? clips.findIndex((item) => item.id === targetClipId)
    : -1;
  const insertIndex =
    targetIndex < 0
      ? placement === "before"
        ? 0
        : clips.length
      : placement === "before"
        ? targetIndex
        : targetIndex + 1;
  clips.splice(insertIndex, 0, clip);
  return {
    ...composition,
    mode:
      composition.clips.length === 1
        ? "separate"
        : getEffectiveCompositionMode(composition),
    clips,
    selectedClipId: clip.id,
    focusedClipId: clip.id,
  };
}

export function removeCompositionClip(
  composition: ProjectComposition,
  clipId: string,
): ProjectComposition {
  const clips = composition.clips.filter((clip) => clip.id !== clipId);
  const fallbackClipId = clips[0]?.id ?? null;
  return {
    ...composition,
    mode: clips.length <= 1 ? "separate" : getEffectiveCompositionMode(composition),
    clips,
    selectedClipId:
      composition.selectedClipId === clipId
        ? fallbackClipId
        : composition.selectedClipId,
    focusedClipId:
      composition.focusedClipId === clipId
        ? fallbackClipId
        : composition.focusedClipId,
  };
}

export function setCompositionMode(
  composition: ProjectComposition,
  mode: ProjectCompositionMode,
): ProjectComposition {
  return {
    ...composition,
    mode: composition.clips.length <= 1 ? "separate" : mode,
  };
}

export function getCompositionClip(
  composition: ProjectComposition | null | undefined,
  clipId: string | null | undefined,
): ProjectCompositionClip | null {
  if (!composition || !clipId) return null;
  return composition.clips.find((clip) => clip.id === clipId) ?? null;
}

export function getSequenceDuration(
  composition: ProjectComposition | null | undefined,
): number {
  if (!composition) return 0;
  return composition.clips.reduce(
    (sum, clip) => sum + Math.max(0, clip.duration),
    0,
  );
}

export function getClipOffsets(
  composition: ProjectComposition | null | undefined,
): Record<string, number> {
  const offsets: Record<string, number> = {};
  if (!composition) return offsets;
  let cursor = 0;
  for (const clip of composition.clips) {
    offsets[clip.id] = cursor;
    cursor += Math.max(0, clip.duration);
  }
  return offsets;
}
