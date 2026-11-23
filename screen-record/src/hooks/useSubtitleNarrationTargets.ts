import { useMemo } from 'react';
import { overlapsRange, type TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { SubtitleSegment } from '@/types/video';
import type { SubtitleNarrationRequestItem } from './subtitleNarrationHelpers';

export function useSubtitleNarrationTargets(
  visibleSubtitles: SubtitleSegment[], selectedSubtitleIds: string[] | undefined,
  selectedSubtitleRange: TrackSelectionRange | null | undefined, readUnsplitSubtitles: boolean,
  projectSubtitle?: (subtitle: SubtitleSegment) => SubtitleSegment | null,
) {
  const targetSubtitles = useMemo(() => {
    const selection = new Set(selectedSubtitleIds ?? []);
    return visibleSubtitles.filter((subtitle) => {
      if (selection.size) return selection.has(subtitle.id);
      return !selectedSubtitleRange || overlapsRange(subtitle, selectedSubtitleRange);
    }).map((subtitle) => projectSubtitle ? projectSubtitle(subtitle) : subtitle)
      .filter((subtitle): subtitle is SubtitleSegment => subtitle !== null)
      .sort((a, b) => a.startTime - b.startTime);
  }, [projectSubtitle, selectedSubtitleIds, selectedSubtitleRange, visibleSubtitles]);

  const narrationTargets = useMemo<SubtitleNarrationRequestItem[]>(() => {
    const single = (subtitle: SubtitleSegment): SubtitleNarrationRequestItem => ({
      id: subtitle.id, text: subtitle.text.trim(), startTime: subtitle.startTime,
      endTime: subtitle.endTime, sourceSubtitleId: subtitle.id, replaceSubtitleIds: [subtitle.id],
    });
    if (!readUnsplitSubtitles) return targetSubtitles.map(single);
    const groups = new Map<string, SubtitleSegment[]>();
    const ungrouped: SubtitleSegment[] = [];
    for (const subtitle of targetSubtitles) {
      if (subtitle.splitGroupId && subtitle.splitGroupText) {
        const group = groups.get(subtitle.splitGroupId) ?? [];
        group.push(subtitle);
        groups.set(subtitle.splitGroupId, group);
      } else ungrouped.push(subtitle);
    }
    const grouped = [...groups.entries()].map(([groupId, subtitles]) => {
      const sorted = [...subtitles].sort((a, b) =>
        (a.splitGroupIndex ?? 0) - (b.splitGroupIndex ?? 0) || a.startTime - b.startTime);
      const first = sorted[0];
      const start = first?.splitGroupStartTime ?? Math.min(...sorted.map((subtitle) => subtitle.startTime));
      const end = first?.splitGroupEndTime ?? Math.max(...sorted.map((subtitle) => subtitle.endTime));
      return {
        id: groupId,
        text: (first?.splitGroupText ?? sorted.map((subtitle) => subtitle.text).join(' ')).trim(),
        startTime: start,
        endTime: end,
        sourceSubtitleId: first?.id ?? groupId,
        replaceSubtitleIds: sorted.map((subtitle) => subtitle.id),
        sourceSubtitleTimings: end > start ? sorted.map((subtitle) => ({ id: subtitle.id,
          start: Math.max(0, Math.min(1, (subtitle.startTime - start) / (end - start))),
          end: Math.max(0, Math.min(1, (subtitle.endTime - start) / (end - start))) })) : undefined,
      };
    });
    return [...grouped, ...ungrouped.map(single)]
      .sort((a, b) => a.startTime - b.startTime || a.id.localeCompare(b.id));
  }, [readUnsplitSubtitles, targetSubtitles]);

  return useMemo(() => ({
    narrationTargetById: new Map(narrationTargets.map((target) => [target.id, target])),
    requestItems: narrationTargets.filter((target) => target.text.length > 0),
  }), [narrationTargets]);
}
