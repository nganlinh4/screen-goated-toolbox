import { useCallback, useEffect } from "react";
import type { VideoSegment } from "@/types/video";
import { useSubtitleGeneration } from "@/hooks/useSubtitleGeneration";
import { deleteSubtitleIdsAcrossTracks } from "@/lib/subtitleTrackMutations";

type SubtitleGenerationOptions = Parameters<typeof useSubtitleGeneration>[0];

interface UseAppSubtitleControllerArgs {
  subtitleOptions: SubtitleGenerationOptions;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useAppSubtitleController({
  subtitleOptions,
  segment,
  setSegment,
  beginBatch,
  commitBatch,
}: UseAppSubtitleControllerArgs) {
  const subtitleState = useSubtitleGeneration(subtitleOptions);
  const {
    editingSubtitleId,
    setEditingSubtitleId,
    subtitleSource,
    setSubtitleSource,
  } = subtitleState;
  const { composition } = subtitleOptions;

  const handleDeleteSubtitle = useCallback(() => {
    if (!segment || !editingSubtitleId) return;
    beginBatch();
    setSegment(deleteSubtitleIdsAcrossTracks(segment, [editingSubtitleId]));
    setEditingSubtitleId(null);
    commitBatch();
  }, [beginBatch, commitBatch, editingSubtitleId, segment, setSegment, setEditingSubtitleId]);

  // Auto-pick the audio subtitle source when a generated silent video is only
  // backing an imported audio timeline.
  useEffect(() => {
    if (
      composition?.placeholderVideoForAudio &&
      (composition.audioSegments?.length ?? 0) > 0
    ) {
      if (subtitleSource !== "audio" && !subtitleSource.startsWith("audio:")) {
        setSubtitleSource("audio");
      }
    }
  }, [
    composition?.placeholderVideoForAudio,
    composition?.audioSegments,
    subtitleSource,
    setSubtitleSource,
  ]);

  return {
    ...subtitleState,
    handleDeleteSubtitle,
  };
}
