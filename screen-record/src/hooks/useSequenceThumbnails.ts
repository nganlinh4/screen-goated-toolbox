import { useEffect, useMemo, useRef, useState } from "react";
import type { Project, ProjectComposition } from "@/types/video";
import { buildSequenceTimeline } from "@/lib/sequenceTimeline";
import { thumbnailGenerator } from "@/lib/thumbnailGenerator";
import { projectManager } from "@/lib/projectManager";

function buildClipThumbnailStamp(
  clip: ProjectComposition["clips"][number],
): string {
  return JSON.stringify({
    duration: clip.duration,
    trimStart: clip.segment.trimStart,
    trimEnd: clip.segment.trimEnd,
    trimSegments: (clip.segment.trimSegments ?? []).map((trimSegment) => ({
      startTime: trimSegment.startTime,
      endTime: trimSegment.endTime,
    })),
    rawVideoPath: clip.rawVideoPath ?? "",
  });
}

interface UseSequenceThumbnailsOptions {
  currentProjectId: string | null;
  currentProjectData: Project | null;
  composition: ProjectComposition | null;
}

export function useSequenceThumbnails({
  currentProjectId,
  currentProjectData,
  composition,
}: UseSequenceThumbnailsOptions) {
  const [thumbnailsByClipId, setThumbnailsByClipId] = useState<
    Record<string, string[]>
  >({});
  const thumbnailStampRef = useRef<Record<string, string>>({});
  const requestIdRef = useRef(0);
  const timeline = useMemo(
    () => buildSequenceTimeline(composition),
    [composition],
  );

  useEffect(() => {
    if (!currentProjectId || !composition || !timeline) {
      setThumbnailsByClipId({});
      thumbnailStampRef.current = {};
      return;
    }

    let cancelled = false;
    const requestId = ++requestIdRef.current;
    const activeClipIds = new Set(timeline.clips.map((clip) => clip.clipId));

    setThumbnailsByClipId((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([clipId]) =>
          activeClipIds.has(clipId),
        ),
      ),
    );

    void (async () => {
      for (const timelineClip of timeline.clips) {
        const stamp = buildClipThumbnailStamp(timelineClip.clip);
        if (thumbnailStampRef.current[timelineClip.clipId] === stamp) continue;

        const videoBlob =
          timelineClip.clip.role === "root"
            ? (currentProjectData?.videoBlob ?? null)
            : (
                await projectManager.loadCompositionClipAssets(
                  currentProjectId,
                  timelineClip.clipId,
                )
              ).videoBlob;

        if (!videoBlob || cancelled) continue;

        const thumbnails = await thumbnailGenerator.generateSegmentThumbnails(
          videoBlob,
          timelineClip.clip.segment,
          timelineClip.sourceDuration,
          10,
        );

        if (cancelled || requestIdRef.current !== requestId) return;

        thumbnailStampRef.current[timelineClip.clipId] = stamp;
        setThumbnailsByClipId((previous) => ({
          ...previous,
          [timelineClip.clipId]: thumbnails,
        }));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [composition, currentProjectData, currentProjectId, timeline]);

  return thumbnailsByClipId;
}
