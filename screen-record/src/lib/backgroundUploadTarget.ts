import type { BackgroundUploadTarget } from "@/hooks/useBackgroundUpload";
import type { BackgroundConfig, Project } from "@/types/video";

export function resolveBackgroundUploadTarget(params: {
  project: Project | null;
  loadedClipId: string | null;
  isTransitioning: boolean;
  background: BackgroundConfig;
  selectionRevision: number;
  clipRevision: number;
}): BackgroundUploadTarget | null {
  const { project, loadedClipId, isTransitioning, background, selectionRevision, clipRevision } = params;
  if (isTransitioning) return null;
  const selected = project?.composition?.focusedClipId ?? project?.composition?.selectedClipId ?? null;
  if (project?.composition && (!loadedClipId || (selected && loadedClipId !== selected))) return null;
  return {
    projectId: project?.id ?? null,
    clipId: project?.composition ? loadedClipId : null,
    backgroundType: background.backgroundType,
    customBackground: background.customBackground,
    selectionRevision,
    clipRevision,
  };
}
