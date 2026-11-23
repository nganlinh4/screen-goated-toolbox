import { useCallback, useMemo, useRef, type MutableRefObject } from 'react';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';
import { editCompositionSubtitleView } from '@/lib/compositionSubtitleEditing';
import { narrationProjectEpoch } from './narrationProjectOwnership';
import { notifyUserError } from '@/lib/userNotifications';
import { populateEmptyS2sSubtitleTracks, type PopulateS2sSubtitleTracksOptions } from './s2sNarrationSubtitles';
import { buildSequenceTimeline, mergeCompositionSegmentsToSequence } from '@/lib/sequenceTimeline';
import { updateCompositionClip } from '@/lib/projectComposition';
import type { SubtitleSegment } from '@/types/video';

export type S2sSubtitleWriter = (source: SubtitleSegment[], target: SubtitleSegment[], language: string,
  options?: PopulateS2sSubtitleTracksOptions) => VideoSegment;

type SegmentValue = VideoSegment | null | ((previous: VideoSegment | null) => VideoSegment | null);
type SegmentSetter = (value: SegmentValue) => void;
type CompositionSetter = (value: ProjectComposition | null) => void;
const VIEW_OWNER = Symbol('composition subtitle view owner');
type EditorView = VideoSegment & { [VIEW_OWNER]?: {
  local: VideoSegment | null; global: VideoSegment | undefined;
  projectId: string | null; clipId: string | null; epoch: number;
} };

export interface CompositionSubtitleEditorOptions {
  segment?: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  setSegment: SegmentSetter;
  setSegmentSilently?: SegmentSetter;
  composition?: ProjectComposition | null;
  setComposition?: CompositionSetter;
  setCompositionSilently?: CompositionSetter;
  currentProjectDataRef?: MutableRefObject<Project | null>;
  currentProjectIdRef?: MutableRefObject<string | null>;
  loadedClipIdRef?: MutableRefObject<string | null>;
  isProjectTransitionRef?: MutableRefObject<boolean>;
  beginBatch?: () => void;
  commitBatch?: () => void;
}

export function useCompositionSubtitleEditor(options: CompositionSubtitleEditorOptions) {
  const { segmentRef, composition, currentProjectDataRef, currentProjectIdRef, loadedClipIdRef,
    isProjectTransitionRef, beginBatch, commitBatch } = options;
  const rawSetSegment = options.setSegment;
  const rawSetSegmentSilently = options.setSegmentSilently;
  const rawSetComposition = options.setComposition;
  const rawSetCompositionSilently = options.setCompositionSilently;
  const viewCache = useRef<{ composition: ProjectComposition | null | undefined; local: VideoSegment | null;
    projectId: string | null; clipId: string | null; epoch: number; view: EditorView | null } | null>(null);
  const projectId = currentProjectIdRef?.current ?? currentProjectDataRef?.current?.id ?? null;
  const epoch = currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0;
  const clipId = loadedClipIdRef?.current ?? (composition?.clips.length === 1 ? composition.clips[0].id : null);
  const renderedSegment = segmentRef.current;
  const renderedGlobal = (currentProjectDataRef?.current?.composition ?? composition)?.globalSegment;

  const read = useCallback(() => {
    if (isProjectTransitionRef?.current
      || (currentProjectIdRef && (currentProjectIdRef.current !== projectId || narrationProjectEpoch(currentProjectIdRef) !== epoch))
      || (currentProjectDataRef && (currentProjectDataRef.current?.id ?? null) !== projectId)) return null;
    const current = currentProjectDataRef?.current?.composition ?? composition;
    const owner = loadedClipIdRef?.current ?? (current?.clips.length === 1 ? current.clips[0].id : null);
    if (owner !== clipId || (current && owner && !current.clips.some((clip) => clip.id === owner))) return null;
    const local = segmentRef.current;
    const cached = viewCache.current;
    if (cached && cached.composition === current && cached.local === local && cached.projectId === projectId
      && cached.clipId === owner && cached.epoch === epoch) {
      return { composition: current, local, clipId: owner, view: cached.view };
    }
    const projected = local ? getCompositionClipRenderSegment(current, owner, local) : null;
    const view: EditorView | null = projected ? { ...projected,
      [VIEW_OWNER]: { local, global: current?.globalSegment, projectId, clipId: owner, epoch },
    } : null;
    viewCache.current = { composition: current, local, clipId: owner, projectId, epoch, view };
    return { composition: current, local, clipId: owner, view };
  }, [clipId, composition, currentProjectDataRef, currentProjectIdRef, epoch, isProjectTransitionRef, loadedClipIdRef, projectId, segmentRef]);

  const apply = useCallback((value: SegmentValue, silent: boolean) => {
    const current = read();
    if (!current) return;
    // Functional edits rebase onto the current view. An object from a retired
    // render cannot overwrite a newer caption result or local editor revision.
    const source = value && typeof value !== 'function' ? (value as EditorView)[VIEW_OWNER] : undefined;
    if (source && (source.projectId !== projectId || source.epoch !== epoch || source.clipId !== current.clipId)) return;
    if (typeof value !== 'function' && (current.local !== (source?.local ?? renderedSegment)
      || current.composition?.globalSegment !== (source ? source.global : renderedGlobal))) {
      notifyUserError('subtitleEditSourceChanged');
      return;
    }
    const editedView = typeof value === 'function' ? value(current.view) : value;
    if (editedView === current.view) return;
    const edited: EditorView | null = editedView ? { ...editedView } : null;
    if (edited) delete edited[VIEW_OWNER];
    const setLocal = silent ? rawSetSegmentSilently ?? rawSetSegment : rawSetSegment;
    if (!current.composition?.globalSegment || !current.local || !edited || !current.clipId) {
      // An unresolved multi-clip owner must not persist a projected view.
      if (current.composition?.globalSegment && !current.clipId) return;
      setLocal(edited);
      return;
    }
    try {
      const next = editCompositionSubtitleView(current.composition, current.clipId, current.local, edited);
      if (next.segment === current.local && next.composition === current.composition) return;
      const setComposition = silent
        ? rawSetCompositionSilently ?? rawSetComposition : rawSetComposition;
      if (!setComposition && next.composition !== current.composition) return;
      if (!silent) beginBatch?.();
      try {
        if (next.segment !== current.local) setLocal(next.segment);
        if (next.composition !== current.composition) setComposition?.(next.composition);
      } finally {
        if (!silent) commitBatch?.();
      }
    } catch (error) {
      notifyUserError('subtitleEditFailed', error);
    }
  }, [beginBatch, commitBatch, epoch, projectId, rawSetComposition, rawSetCompositionSilently,
    rawSetSegment, rawSetSegmentSilently, read, renderedGlobal, renderedSegment]);
  const setSegment = useCallback((value: SegmentValue) => apply(value, false), [apply]);
  const setSegmentSilently = useCallback((value: SegmentValue) => apply(value, true), [apply]);
  const populateS2sSubtitles = useCallback<S2sSubtitleWriter>((source, target, language, update) => {
    const current = read();
    if (!current?.local || !update?.coordinateSpace) throw new Error('Subtitle destination is unavailable');
    const sequence = update.coordinateSpace === 'sequence';
    const timeline = current.composition ? buildSequenceTimeline(current.composition) : null;
    const base = sequence && current.composition
      ? current.composition.globalSegment ?? (timeline ? mergeCompositionSegmentsToSequence(timeline) : null)
      : current.local;
    if (!base || (sequence && !current.composition)) throw new Error('Sequence subtitle destination is unavailable');
    const next = populateEmptyS2sSubtitleTracks(base, source, target, language, update);
    const silent = update.liveUpdate === true;
    const setLocal = silent ? rawSetSegmentSilently ?? rawSetSegment : rawSetSegment;
    const setComposition = silent ? rawSetCompositionSilently ?? rawSetComposition : rawSetComposition;
    if (current.composition && (!current.clipId || !setComposition)) throw new Error('Subtitle destination is unavailable');
    const nextComposition = current.composition && (sequence
      ? { ...current.composition, globalSegment: next }
      : updateCompositionClip(current.composition, current.clipId!, { segment: next }));
    if (!silent) beginBatch?.();
    try {
      if (!sequence) setLocal(next);
      if (nextComposition) setComposition!(nextComposition);
    } finally {
      if (!silent) commitBatch?.();
    }
    return next;
  }, [beginBatch, commitBatch, rawSetComposition, rawSetCompositionSilently, rawSetSegment, rawSetSegmentSilently, read]);
  const viewRef = useMemo(() => ({ get current() { return read()?.view ?? null; } }), [read]);
  return { segment: read()?.view ?? options.segment ?? null, segmentRef: viewRef, setSegment, setSegmentSilently, populateS2sSubtitles };
}
