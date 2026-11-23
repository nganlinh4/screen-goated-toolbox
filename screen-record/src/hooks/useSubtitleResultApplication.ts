import { startTransition, useCallback, useEffect, useRef } from 'react';
import {
  getEffectiveCompositionMode,
  updateCompositionClip,
} from '@/lib/projectComposition';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  mergeCompositionSegmentsToSequence,
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
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';
import { subtitleSourceMatches } from './subtitleSourceOwnership';
import { narrationProjectEpoch } from './narrationProjectOwnership';
import { isSubtitleSaveEditorActive, requestSubtitleCompletedSave, subscribeSubtitleSaveInvalidation, wakeSubtitleCompletedSaves } from './subtitleCompletedPersistence';
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
  segmentRef?: React.MutableRefObject<VideoSegment | null>;
  currentProjectDataRef?: React.MutableRefObject<Project | null>;
  currentProjectIdRef?: React.MutableRefObject<string | null>;
  projectResetKey?: string | null;
  currentRawVideoPathRef?: React.MutableRefObject<string>;
  currentRawMicAudioPathRef?: React.MutableRefObject<string>;
  currentRawVideoPath?: string;
  currentRawMicAudioPath?: string;
  onStaleResults?: () => void;
  onQueuedResultsFlushed?: () => void;
  ownsProjectEpoch?: () => boolean;
  loadedClipIdRef: React.MutableRefObject<string | null>;
  setSegment: (
    segment:
      | VideoSegment
      | null
      | ((prev: VideoSegment | null) => VideoSegment | null),
  ) => void;
  setSegmentSilently: (
    segment: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null),
  ) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  setCompositionSilently: (
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
  segmentRef,
  currentProjectDataRef,
  currentProjectIdRef,
  projectResetKey,
  currentRawVideoPathRef,
  currentRawMicAudioPathRef,
  currentRawVideoPath,
  currentRawMicAudioPath,
  onStaleResults,
  onQueuedResultsFlushed,
  ownsProjectEpoch,
  setSegment,
  setSegmentSilently,
  loadedClipIdRef,
  composition,
  setComposition,
  setCompositionSilently,
  jobContext,
  jobId,
  persistProject,
}: UseSubtitleResultApplicationParams) {
  const pendingSubtitleResultsRef = useRef<SubtitleClipResult[]>([]);
  const onQueuedResultsFlushedRef = useRef(onQueuedResultsFlushed);
  onQueuedResultsFlushedRef.current = onQueuedResultsFlushed;
  const pendingSubtitleApplyTimerRef = useRef<number | null>(null);
  const jobContextRef = useRef<SubtitleJobContext | null>(null);
  const persistProjectRef = useRef<typeof persistProject>(persistProject);
  const lastSubtitleApplyPerfLogRef = useRef(0);
  const partialApplyStateRef = useRef(new Map<string, { signature: string; appliedAt: number }>());
  const historyJobIdRef = useRef<string | null | undefined>(undefined);

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
    if (ownsProjectEpoch && !ownsProjectEpoch()) return;
    const applyEpoch = currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0;
    const ownsAppliedEpoch = () => (!ownsProjectEpoch || ownsProjectEpoch())
      && (!currentProjectIdRef || narrationProjectEpoch(currentProjectIdRef) === applyEpoch);
    const currentProject = currentProjectDataRef?.current;
    if (currentProjectDataRef && (!context || context.projectKey !== (currentProject?.id ?? null))) {
      onStaleResults?.();
      return;
    }
    const currentComposition = currentProject?.composition ?? composition;
    const isCurrent = (result: SubtitleClipResult, candidate = currentComposition) => subtitleSourceMatches(
      context?.sourceOwnersByClip?.[result.clipId], candidate, loadedClipIdRef.current,
      segmentRef?.current ?? segment,
      currentRawVideoPathRef?.current ?? currentRawVideoPath ?? '',
      currentRawMicAudioPathRef?.current ?? currentRawMicAudioPath ?? '',
    );
    const valid = results.filter((result) => isCurrent(result));
    if (valid.length !== results.length) onStaleResults?.();
    results = valid;
    if (results.length === 0) return;
    const separate = !currentComposition || getEffectiveCompositionMode(currentComposition) === 'separate';
    const useLiveSegmentOnly = !currentComposition;
    if (useLiveSegmentOnly && !segment) return;
    if (!useLiveSegmentOnly && !results.some((result) =>
      context?.clipTransformsByClip[result.clipId]
      || currentComposition.clips.some((clip) => clip.id === result.clipId),
    )) return;
    const startedAt = performance.now();
    const subtitleStyle = defaultSubtitleStyle();
    const saveUndoHistory = historyJobIdRef.current !== jobId;
    historyJobIdRef.current = jobId;
    const hasFinal = results.some((result) => !result.isPartial);
    if (useLiveSegmentOnly) {
      const applySegmentUpdate = () => (saveUndoHistory ? setSegment : setSegmentSilently)((prev) => {
        if (!prev || !ownsAppliedEpoch()) return prev;
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
      });
      if (hasFinal) {
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

    const applyClipResult = (clipSegment: VideoSegment, result: SubtitleClipResult) => {
      const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
      const inserted = result.segments.map((entry, index) =>
        buildInsertedSubtitle(result, entry, index, subtitleStyle, undefined, context?.sourceTypeForNative),
      );
      const replacedSegment = result.isPartial
        ? mergePartialOriginalSubtitleSegments(clipSegment, inserted, replacementRanges)
        : replaceOriginalSubtitleSegments(clipSegment, inserted, replacementRanges);
      return setActiveSubtitleTrackView(clearDerivedSubtitleTracks(replacedSegment), ORIGINAL_SUBTITLE_TRACK_ID);
    };

    const applyCompositionUpdate = () => (saveUndoHistory ? setComposition : setCompositionSilently)((prev) => {
      if (!prev || !ownsAppliedEpoch()) return prev;
      const timeline = buildSequenceTimeline(prev);
      if (!timeline) return prev;

      let next = prev;
      for (const result of results) {
        if (!isCurrent(result, next)) continue;
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
          const baseSegment = next.globalSegment ?? mergeCompositionSegmentsToSequence(timeline);
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
        const updatedSegment = applyClipResult(clip.segment, result);
        logSubtitleApplyDiagnostics('apply-composition', result, clip.segment, updatedSegment, replacementRanges);

        next = updateCompositionClip(next, clip.id, { segment: updatedSegment });

        if (!separate && next.globalSegment) {
          const timelineClip = getSequenceClipById(timeline, clip.id);
          if (timelineClip) {
            next = {
              ...next,
              globalSegment: replaceSequenceClipSegmentInGlobal(
                next.globalSegment,
                updatedSegment,
                timelineClip,
                timeline.totalDuration,
                new Set((next.audioSegments ?? []).map((audio) => audio.id)),
              ),
            };
          }
        }
      }
      return next;
    });
    const applyUnifiedUpdate = () => {
      applyCompositionUpdate();
      // The clip loader updates this ref synchronously with the editor segment;
      // the rendered activeClipId can lag behind a completed async handoff.
      const editorClipId = loadedClipIdRef.current
        ?? (currentComposition.clips.length === 1 ? currentComposition.clips[0].id : null);
      const activeResults = results.filter((result) =>
        result.clipId === editorClipId && !context?.clipTransformsByClip[result.clipId] && isCurrent(result),
      );
      if (activeResults.length > 0 && currentComposition.clips.some((clip) => clip.id === editorClipId)) {
        setSegmentSilently((prev) => prev && ownsAppliedEpoch()
          ? activeResults.reduce(applyClipResult, prev)
          : prev);
      }
    };
    if (hasFinal) {
      applyUnifiedUpdate();
    } else {
      startTransition(applyUnifiedUpdate);
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
  }, [composition, currentProjectDataRef, currentProjectIdRef, currentRawMicAudioPath, currentRawMicAudioPathRef, currentRawVideoPath, currentRawVideoPathRef, jobId, loadedClipIdRef, onStaleResults, ownsProjectEpoch, segment, segmentRef, setComposition, setCompositionSilently, setSegment, setSegmentSilently]);

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
      onQueuedResultsFlushedRef.current?.();
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
        if (pendingSubtitleResultsRef.current.length > 0) {
          pendingSubtitleResultsRef.current = coalesceSubtitleClipResults(pendingSubtitleResultsRef.current, results);
        }
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

  const markCompletedJobForPersist = useCallback((retainRecovery = false) => {
    const projectId = jobContextRef.current?.projectKey;
    if (!jobId || !projectId || !currentProjectIdRef || !currentProjectDataRef) return;
    const epoch = narrationProjectEpoch(currentProjectIdRef);
    const ownerState = () => {
      if (!isSubtitleSaveEditorActive(currentProjectIdRef) || currentProjectIdRef.current !== projectId || narrationProjectEpoch(currentProjectIdRef) !== epoch) return 'superseded' as const;
      return currentProjectDataRef.current?.id === projectId ? 'ready' as const : 'waiting' as const;
    };
    const current = currentProjectDataRef.current;
    if (!current || current.id !== projectId) return;
    const clipId = loadedClipIdRef.current
      ?? (current?.composition?.clips.length === 1 ? current.composition.clips[0].id : null);
    const liveSegment = segmentRef?.current ?? current.segment ?? null;
    const retained = { projectId, composition: current.composition ?? null, segment: liveSegment, loadedClipId: clipId };
    let snapshot = retained;
    try { snapshot = structuredClone(retained); } catch { /* Retain the existing editor references if cloning is unavailable. */ }
    requestSubtitleCompletedSave(jobId, snapshot, ownerState, async (isCurrent) => {
      const project = currentProjectDataRef.current;
      if (!project?.composition || !persistProjectRef.current) return false;
      const clipId = loadedClipIdRef.current
        ?? (project.composition.clips.length === 1 ? project.composition.clips[0].id : null);
      const liveSegment = segmentRef?.current ?? project.segment;
      if (!clipId || !liveSegment || !project.composition.clips.some((clip) => clip.id === clipId)) return false;
      const latestComposition = updateCompositionClip(project.composition, clipId, { segment: liveSegment });
      const canCommit = () => isCurrent() && ownerState() === 'ready' && currentProjectDataRef.current === project
        && (segmentRef?.current ?? project.segment) === liveSegment
        && (loadedClipIdRef.current ?? (project.composition?.clips.length === 1 ? project.composition.clips[0].id : null)) === clipId;
      let committed = false;
      await persistProjectRef.current({
        refreshList: true, includeMedia: false, throwOnError: true,
        expectedProjectId: projectId, onCommitted: () => { committed = true; }, canCommit,
        onInvalidated: (listener) => subscribeSubtitleSaveInvalidation(currentProjectIdRef, listener),
        compositionOverride: latestComposition, skipLiveCompositionSync: true,
      });
      return committed;
    }, retainRecovery);
  }, [currentProjectDataRef, currentProjectIdRef, jobId, loadedClipIdRef, segmentRef]);

  useEffect(() => { wakeSubtitleCompletedSaves(); }, [projectResetKey, !!composition, !!segment]);

  useEffect(() => clearQueuedSubtitleResults, [clearQueuedSubtitleResults]);

  return {
    clearQueuedSubtitleResults,
    flushQueuedSubtitleResults,
    getQueuedSubtitleResults: () => pendingSubtitleResultsRef.current,
    markCompletedJobForPersist,
    queueSubtitleResults,
  };
}
