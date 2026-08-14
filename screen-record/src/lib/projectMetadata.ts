import type { Project, ProjectCompositionClip } from "@/types/video";

export const MAX_PROJECT_NAME_LENGTH = 120;
const WINDOWS_INVALID_NAME_CHARACTERS = /[<>:"/\\|?*]/g;
const WINDOWS_RESERVED_NAME = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i;

export function normalizeProjectName(value: string): string {
  const normalized = value
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replace(WINDOWS_INVALID_NAME_CHARACTERS, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/g, "");
  if (!normalized) {
    throw new Error("Project name cannot be empty");
  }
  const windowsSafe = WINDOWS_RESERVED_NAME.test(normalized)
    ? `Project ${normalized}`
    : normalized;
  const truncated = [...windowsSafe]
    .slice(0, MAX_PROJECT_NAME_LENGTH)
    .join("")
    .trimEnd()
    .replace(/[. ]+$/g, "");
  if (!truncated) {
    throw new Error("Project name cannot be empty");
  }
  return truncated;
}

function collectClipBackgrounds(
  clips: ProjectCompositionClip[] | undefined,
  urls: Set<string>,
): void {
  for (const clip of clips ?? []) {
    const customBackground = clip.backgroundConfig?.customBackground;
    if (typeof customBackground === "string" && customBackground) {
      urls.add(customBackground);
    }
  }
}

export function collectProjectCustomBackgroundUrls(
  projects: Array<Pick<Project, "backgroundConfig" | "composition">>,
): string[] {
  const urls = new Set<string>();
  for (const project of projects) {
    const rootBackground = project.backgroundConfig?.customBackground;
    if (typeof rootBackground === "string" && rootBackground) {
      urls.add(rootBackground);
    }
    const composition = project.composition;
    for (const config of [
      composition?.globalPresentationConfig,
      composition?.globalBackgroundConfig,
    ]) {
      if (typeof config?.customBackground === "string" && config.customBackground) {
        urls.add(config.customBackground);
      }
    }
    collectClipBackgrounds(composition?.clips, urls);
    collectClipBackgrounds(composition?.retainedRemovedClips, urls);
  }
  return [...urls];
}
