import { startTransition, useCallback, useEffect, useRef } from 'react';
import {
  getEffectiveCompositionMode,
  updateCompositionClip,
} from '@/lib/projectComposition';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  replaceSequenceClipSegmentInGlobal,
} from '@/lib/sequenceTimeline';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import {
  clearDerivedSubtitleTracks,
  mergePartialOriginalSubtitleSegments,
  replaceAudioSubtitlesOnOriginalTrack,
  replaceOriginalSubtitleSegments,
} from '@/lib/subtitleTrackMutations';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  setActiveSubtitleTrackView,
} from '@/lib/subtitleTracks';
import { markFrontendPerfEvent } from '@/lib/frontendPerfDiagnostics';
import type { PersistOptions } from '@/hooks/useSequenceComposition';
import type { ProjectComposition, VideoSegment } from '@/types/video';
import {
  buildInsertedSubtitle,
  coalesceSubtitleClipResults,
  logSubtitleApplyDiagnostics,
  partialApplySignature,
} from './subtitleGenerationResults';
import type {
  SubtitleClipResult,
  SubtitleJobContext,
} from './subtitleGenerationTypes';

const SUBTITLE_PARTIAL_APPLY_INTERVAL_MS = 2500;
const SUBTITLE_PARTIAL_TEXT_REFRESH_MS = 5000;
const SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS = 1000;

interface UseSubtitleResultApplicationParams {
  segment: VideoSegment | null;
  setSegment: (
    segment:
      | VideoSegment
      | null
      | ((prev: VideoSegment | null) => VideoSegment | null),
    withHistory?: boolean,
  ) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  jobContext: SubtitleJobContext | null;
  jobId: string | null;
  persistProject?: (opts?: PersistOptions) => Promise<void>;
}

export function useSubtitleResultApplication({
  segment,
  setSegment,
  composition,
  setComposition,
  jobContext,
  jobId,
  persistProject,
}: UseSubtitleResultApplicationParams) {
  const pendingSubtitleResultsRef = useRef<SubtitleClipResult[]>([]);
  const pendingSubtitleApplyTimerRef = useRef<number | null>(null);
  const jobContextRef = useRef<SubtitleJobContext | null>(null);
  const persistProjectRef = useRef<typeof persistProject>(persistProject);
  const pendingCompletedJobPersistRef = useRef(false);
  const lastSubtitleApplyPerfLogRef = useRef(0);
  const partialApplyStateRef = useRef(new Map<string, { signature: string; appliedAt: number }>());

  useEffect(() => {
    jobContextRef.current = jobContext;
  }, [jobContext]);

  useEffect(() => {
    persistProjectRef.current = persistProject;
  }, [persistProject]);

  const clearQueuedSubtitleResults = useCallback(() => {
    if (pendingSubtitleApplyTimerRef.current !== null) {
      window.clearTimeout(pendingSubtitleApplyTimerRef.current);
      pendingSubtitleApplyTimerRef.current = null;
    }
    pendingSubtitleResultsRef.current = [];
  }, []);

  const applyResults = useCallback((results: SubtitleClipResult[], context: SubtitleJobContext | null) => {
    if (results.length === 0) return;
    const startedAt = performance.now();
    const subtitleStyle = defaultSubtitleStyle();
    const saveUndoHistory = results.some((result) => !result.isPartial);
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      const applySegmentUpdate = () => setSegment((prev) => {
        if (!prev) return prev;
        return results.reduce((nextSegment, result) => {
          const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
          const transform = context?.clipTransformsByClip[result.clipId];
          const inserted = result.segments.map((entry, index) =>
            buildInsertedSubtitle(
              result,
              entry,
              index,
              subtitleStyle,
              transform,
              context?.sourceTypeForNative,
            ),
          );
          const replacedSegment = transform
            ? replaceAudioSubtitlesOnOriginalTrack(
                nextSegment,
                new Set([transform.audioSegmentId]),
                replacementRanges,
                inserted,
              )
            : result.isPartial
              ? mergePartialOriginalSubtitleSegments(nextSegment, inserted, replacementRanges)
              : replaceOriginalSubtitleSegments(nextSegment, inserted, replacementRanges);
          const updatedSegment = setActiveSubtitleTrackView(
            clearDerivedSubtitleTracks(replacedSegment),
            ORIGINAL_SUBTITLE_TRACK_ID,
          );
          logSubtitleApplyDiagnostics('apply-separate', result, nextSegment, updatedSegment, replacementRanges);
          return updatedSegment;
        }, prev);
      }, saveUndoHistory);
      if (saveUndoHistory) {
        applySegmentUpdate();
      } else {
        startTransition(applySegmentUpdate);
      }
      const elapsedMs = performance.now() - startedAt;
      const now = performance.now();
      if (elapsedMs > 8 && now - lastSubtitleApplyPerfLogRef.current > SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS) {
        lastSubtitleApplyPerfLogRef.current = now;
        const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
        console.log(
          `[SubtitleGen][Perf] apply mode=separate ms=${elapsedMs.toFixed(1)} results=${results.length} segments=${segmentCount} partial=${results.filter((result) => result.isPartial).length} final=${results.filter((result) => !result.isPartial).length}`,
        );
      }
      return;
    }

    const applyCompositionUpdate = () => setComposition((prev) => {
      if (!prev) return prev;
      const timeline = buildSequenceTimeline(prev);
      if (!timeline) return prev;

      let next = prev;
      for (const result of results) {
        const clip = next.clips.find((entry) => entry.id === result.clipId);
        const transform = context?.clipTransformsByClip[result.clipId];
        const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
        if (transform) {
          const inserted = result.segments.map((entry, index) =>
            buildInsertedSubtitle(
              result,
              entry,
              index,
              subtitleStyle,
              transform,
              context?.sourceTypeForNative,
            ),
          );
          const baseSegment = next.globalSegment ?? segment;
          if (baseSegment) {
            const updatedSegment = setActiveSubtitleTrackView(
              clearDerivedSubtitleTracks(
                replaceAudioSubtitlesOnOriginalTrack(
                  baseSegment,
                  new Set([transform.audioSegmentId]),
                  replacementRanges,
                  inserted,
                ),
              ),
              ORIGINAL_SUBTITLE_TRACK_ID,
            );
            next = {
              ...next,
              globalSegment: updatedSegment,
            };
          }
          continue;
        }
        if (!clip) continue;
        const inserted = result.segments.map((entry, index) =>
          buildInsertedSubtitle(
            result,
            entry,
            index,
            subtitleStyle,
            undefined,
            context?.sourceTypeForNative,
          ),
        );
        const replacedSegment = result.isPartial
          ? mergePartialOriginalSubtitleSegments(
              clip.segment,
              inserted,
              replacementRanges,
            )
          : replaceOriginalSubtitleSegments(
              clip.segment,
              inserted,
              replacementRanges,
            );
        const updatedSegment = setActiveSubtitleTrackView(
          clearDerivedSubtitleTracks(replacedSegment),
          ORIGINAL_SUBTITLE_TRACK_ID,
        );
        logSubtitleApplyDiagnostics('apply-composition', result, clip.segment, updatedSegment, replacementRanges);

        next = updateCompositionClip(next, clip.id, { segment: updatedSegment });

        if (next.globalSegment) {
          const timelineClip = getSequenceClipById(timeline, clip.id);
          if (timelineClip) {
            next = {
              ...next,
              globalSegment: replaceSequenceClipSegmentInGlobal(
                next.globalSegment,
                updatedSegment,
                timelineClip,
                timeline.totalDuration,
              ),
            };
          }
        }
      }
      return next;
    });
    if (saveUndoHistory) {
      applyCompositionUpdate();
    } else {
      startTransition(applyCompositionUpdate);
    }
    const elapsedMs = performance.now() - startedAt;
    const now = performance.now();
    if (elapsedMs > 8 && now - lastSubtitleApplyPerfLogRef.current > SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS) {
      lastSubtitleApplyPerfLogRef.current = now;
      const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
      console.log(
        `[SubtitleGen][Perf] apply mode=unified ms=${elapsedMs.toFixed(1)} results=${results.length} segments=${segmentCount} partial=${results.filter((result) => result.isPartial).length} final=${results.filter((result) => !result.isPartial).length}`,
      );
    }
  }, [composition, segment, setComposition, setSegment]);

  const applyResultsRef = useRef(applyResults);

  useEffect(() => {
    applyResultsRef.current = applyResults;
  }, [applyResults]);

  const flushQueuedSubtitleResults = useCallback(() => {
    if (pendingSubtitleApplyTimerRef.current !== null) {
      window.clearTimeout(pendingSubtitleApplyTimerRef.current);
      pendingSubtitleApplyTimerRef.current = null;
    }
    const results = pendingSubtitleResultsRef.current;
    pendingSubtitleResultsRef.current = [];
    if (results.length > 0) {
      const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
      const partialCount = results.filter((result) => result.isPartial).length;
      markFrontendPerfEvent(`subtitle-flush-start results=${results.length} segments=${segmentCount} partial=${partialCount}`);
      applyResultsRef.current(results, jobContextRef.current);
      markFrontendPerfEvent(`subtitle-flush-end results=${results.length} segments=${segmentCount}`);
    }
  }, []);

  const queueSubtitleResults = useCallback((results: SubtitleClipResult[], immediate: boolean) => {
    if (results.length === 0) return;
    const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
    markFrontendPerfEvent(`subtitle-queue results=${results.length} segments=${segmentCount} immediate=${immediate ? 1 : 0}`);
    if (!immediate && results.every((result) => result.isPartial)) {
      const now = performance.now();
      const shouldApply = results.some((result) => {
        const signature = partialApplySignature(result);
        const previous = partialApplyStateRef.current.get(result.clipId);
        return !previous
          || previous.signature !== signature
          || now - previous.appliedAt >= SUBTITLE_PARTIAL_TEXT_REFRESH_MS;
      });
      if (!shouldApply) {
        markFrontendPerfEvent(`subtitle-partial-skip-stable results=${results.length} segments=${segmentCount}`);
        return;
      }
      for (const result of results) {
        partialApplyStateRef.current.set(result.clipId, {
          signature: partialApplySignature(result),
          appliedAt: now,
        });
      }
    }
    if (immediate) {
      partialApplyStateRef.current.clear();
    }
    pendingSubtitleResultsRef.current = coalesceSubtitleClipResults(
      pendingSubtitleResultsRef.current,
      results,
    );
    if (immediate) {
      flushQueuedSubtitleResults();
      return;
    }
    if (pendingSubtitleApplyTimerRef.current === null) {
      pendingSubtitleApplyTimerRef.current = window.setTimeout(
        flushQueuedSubtitleResults,
        SUBTITLE_PARTIAL_APPLY_INTERVAL_MS,
      );
    }
  }, [flushQueuedSubtitleResults]);

  const markCompletedJobForPersist = useCallback(() => {
    pendingCompletedJobPersistRef.current = true;
  }, []);

  useEffect(() => {
    if (!pendingCompletedJobPersistRef.current || jobId) return;
    if (!segment && !composition) return;

    const timeoutId = window.setTimeout(() => {
      if (!pendingCompletedJobPersistRef.current) return;
      pendingCompletedJobPersistRef.current = false;
      void persistProjectRef.current?.({
        refreshList: true,
        includeMedia: false,
      }).catch((error) => {
        pendingCompletedJobPersistRef.current = true;
        console.warn('[SubtitleGen][Persist] Failed to save completed subtitles:', error);
      });
    }, 50);

    return () => window.clearTimeout(timeoutId);
  }, [composition, jobId, segment]);

  useEffect(() => clearQueuedSubtitleResults, [clearQueuedSubtitleResults]);

  return {
    clearQueuedSubtitleResults,
    flushQueuedSubtitleResults,
    markCompletedJobForPersist,
    queueSubtitleResults,
  };
}
