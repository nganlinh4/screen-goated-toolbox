import { AudioLines, Captions, Rows3 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { PanelCard } from '@/components/layout/PanelCard';
import { useSettings } from '@/hooks/useSettings';
import type { ImportedAudioSegment, NarrationSegment, SubtitleSegment } from '@/types/video';
import {
  RATE_MAX,
  RATE_MIN,
  clampRate,
  formatSec,
  getTimelineDuration,
  readFiniteNumber,
  type AudioPanelDraft,
  type AudioPanelSegment,
} from './audioPanelUtils';

interface AudioPanelProps {
  importedSegments: ImportedAudioSegment[];
  narrationSegments: NarrationSegment[];
  subtitleSegments: SubtitleSegment[];
  selectedImportedIds: ReadonlySet<string>;
  selectedNarrationIds: ReadonlySet<string>;
  onUpdateImportedSegment: (id: string, patch: Partial<ImportedAudioSegment>) => void;
  onUpdateNarrationSegment: (id: string, patch: Partial<NarrationSegment>) => void;
  onAlignSubtitlesToNarration?: () => void;
  beginBatch?: () => void;
  commitBatch?: () => void;
  onCommitImportedSegments?: () => void;
  onCommitNarrationSegments?: () => void;
}

export function AudioPanel({
  importedSegments,
  narrationSegments,
  subtitleSegments,
  selectedImportedIds,
  selectedNarrationIds,
  onUpdateImportedSegment,
  onUpdateNarrationSegment,
  onAlignSubtitlesToNarration,
  beginBatch,
  commitBatch,
  onCommitImportedSegments,
  onCommitNarrationSegments,
}: AudioPanelProps) {
  const { t } = useSettings();
  const selected: AudioPanelSegment[] = [
    ...importedSegments
      .filter((segment) => selectedImportedIds.has(segment.id))
      .map((segment) => ({ ...segment, kind: 'imported' as const })),
    ...narrationSegments
      .filter((segment) => selectedNarrationIds.has(segment.id))
      .map((segment) => ({ ...segment, kind: 'narration' as const })),
  ];
  const headerCount = selected.length;
  const selectedRateSignature = selected
    .map((segment) => `${segment.kind}:${segment.id}:${segment.playbackRate ?? 1}`)
    .join('|');
  const selectedKeySignature = selected
    .map((segment) => `${segment.kind}:${segment.id}`)
    .join('|');
  const getSelectedAverageRate = () => {
    if (selected.length === 0) return 1;
    const total = selected.reduce((sum, segment) => sum + (segment.playbackRate ?? 1), 0);
    return clampRate(total / selected.length);
  };
  const [bulkRate, setBulkRate] = useState(getSelectedAverageRate);
  const [drafts, setDrafts] = useState<Record<string, AudioPanelDraft>>({});
  const previewFrameRef = useRef<number | null>(null);
  const pendingPreviewRef = useRef<Record<string, { segment: AudioPanelSegment; patch: AudioPanelDraft }>>({});
  const selectedNarrationWithSubtitleCount = selected.filter(
    (segment) => segment.kind === 'narration' && !!segment.sourceSubtitleId,
  ).length;

  useEffect(() => {
    setBulkRate(getSelectedAverageRate());
  }, [selectedRateSignature]);

  useEffect(() => {
    const liveKeys = new Set(selected.map((segment) => `${segment.kind}:${segment.id}`));
    setDrafts((prev) => {
      const next = Object.fromEntries(Object.entries(prev).filter(([key]) => liveKeys.has(key)));
      return Object.keys(next).length === Object.keys(prev).length ? prev : next;
    });
  }, [selectedKeySignature]);

  const getDraftKey = (segment: AudioPanelSegment) => `${segment.kind}:${segment.id}`;

  const getEffectiveSegment = (segment: AudioPanelSegment): AudioPanelSegment => ({
    ...segment,
    ...(drafts[getDraftKey(segment)] ?? {}),
  });

  const setSegmentDraft = (segment: AudioPanelSegment, patch: AudioPanelDraft) => {
    const key = getDraftKey(segment);
    setDrafts((prev) => ({
      ...prev,
      [key]: {
        ...(prev[key] ?? {}),
        ...patch,
      },
    }));
  };

  const updateSegment = (segment: AudioPanelSegment, patch: Partial<ImportedAudioSegment | NarrationSegment>) => {
    if (segment.kind === 'narration') {
      onUpdateNarrationSegment(segment.id, patch as Partial<NarrationSegment>);
      return;
    }
    onUpdateImportedSegment(segment.id, patch as Partial<ImportedAudioSegment>);
  };

  const escapeSelectorValue = (value: string) => {
    if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function') return CSS.escape(value);
    return value.replace(/["\\]/g, '\\$&');
  };

  const resetTimelineVisualDraft = (segment: AudioPanelSegment) => {
    const selector = segment.kind === 'narration'
      ? `.narration-track-segment[data-narration-segment-id="${escapeSelectorValue(segment.id)}"]`
      : `.audio-track-segment[data-audio-segment-id="${escapeSelectorValue(segment.id)}"]`;
    const element = document.querySelector<HTMLElement>(selector);
    if (!element) return;
    element.querySelectorAll<HTMLElement>('[data-sgt-preview-created="true"]').forEach((child) => {
      child.remove();
    });
    delete element.dataset.sgtPreviewBaseWidthPct;
    delete element.dataset.sgtPreviewBaseLeftPct;
  };

  const applyTimelineVisualDraft = (segment: AudioPanelSegment, patch: AudioPanelDraft) => {
    const selector = segment.kind === 'narration'
      ? `.narration-track-segment[data-narration-segment-id="${escapeSelectorValue(segment.id)}"]`
      : `.audio-track-segment[data-audio-segment-id="${escapeSelectorValue(segment.id)}"]`;
    const element = document.querySelector<HTMLElement>(selector);
    if (!element) return;

    const timelineDuration = readFiniteNumber(element.dataset.timelineDuration, 0);
    if (!Number.isFinite(timelineDuration) || timelineDuration <= 0) return;

    const previewSegment: AudioPanelSegment = {
      ...segment,
      duration: readFiniteNumber(element.dataset.duration, segment.duration),
      startTime: readFiniteNumber(element.dataset.startTime, segment.startTime),
      inPoint: readFiniteNumber(element.dataset.inPoint, segment.inPoint),
      outPoint: readFiniteNumber(element.dataset.outPoint, segment.outPoint),
      playbackRate: readFiniteNumber(element.dataset.playbackRate, segment.playbackRate ?? 1),
      ...patch,
    };
    const leftPct = Math.min(100, Math.max(0, (previewSegment.startTime / timelineDuration) * 100));
    const nextWidthPct = Math.min(100, Math.max(0.001, (getTimelineDuration(previewSegment) / timelineDuration) * 100));
    element.dataset.sgtPreviewBaseLeftPct ||= element.style.left || `${leftPct}%`;
    element.dataset.sgtPreviewBaseWidthPct ||= element.style.width || `${nextWidthPct}%`;
    element.style.left = `${leftPct}%`;
    element.style.width = `${nextWidthPct}%`;

    const rate = clampRate(previewSegment.playbackRate ?? 1);
    let speedBadge = element.querySelector<HTMLElement>(
      segment.kind === 'narration' ? '.narration-track-segment-speed' : '.audio-track-segment-speed',
    );
    if (!speedBadge) {
      const content = element.querySelector<HTMLElement>(
        segment.kind === 'narration' ? '.narration-track-segment-content' : '.audio-track-segment-content',
      );
      if (content) {
        speedBadge = document.createElement('span');
        speedBadge.className = segment.kind === 'narration'
          ? 'narration-track-segment-speed ml-auto rounded bg-[var(--secondary-color)]/30 px-1 text-[9px] font-semibold leading-3'
          : 'audio-track-segment-speed ml-auto rounded bg-[var(--primary-color)]/30 px-1 text-[9px] font-semibold leading-3';
        speedBadge.dataset.sgtPreviewCreated = 'true';
        content.appendChild(speedBadge);
      }
    }
    if (speedBadge) {
      speedBadge.textContent = `${rate.toFixed(2)}×`;
      speedBadge.style.display = Math.abs(rate - 1) > 0.001 ? '' : 'none';
    }
  };

  const flushPreviewDrafts = () => {
    previewFrameRef.current = null;
    const pending = pendingPreviewRef.current;
    pendingPreviewRef.current = {};
    Object.values(pending).forEach(({ segment, patch }) => applyTimelineVisualDraft(segment, patch));
  };

  const schedulePreviewDraft = (segment: AudioPanelSegment, patch: AudioPanelDraft) => {
    const key = getDraftKey(segment);
    const pending = pendingPreviewRef.current[key];
    pendingPreviewRef.current[key] = {
      segment,
      patch: {
        ...(pending?.patch ?? {}),
        ...patch,
      },
    };
    if (previewFrameRef.current !== null) return;
    previewFrameRef.current = window.requestAnimationFrame(flushPreviewDrafts);
  };

  useEffect(() => () => {
    if (previewFrameRef.current !== null) {
      window.cancelAnimationFrame(previewFrameRef.current);
      previewFrameRef.current = null;
    }
    pendingPreviewRef.current = {};
  }, []);

  const commitSegmentDraft = (segment: AudioPanelSegment) => {
    if (previewFrameRef.current !== null) {
      window.cancelAnimationFrame(previewFrameRef.current);
      flushPreviewDrafts();
    }
    const key = getDraftKey(segment);
    const draft = drafts[key];
    if (draft && Object.keys(draft).length > 0) {
      updateSegment(segment, draft);
      setDrafts((prev) => {
        const { [key]: _removed, ...rest } = prev;
        return rest;
      });
      resetTimelineVisualDraft(segment);
      commitSegment(segment);
    }
  };

  const commitSegment = (segment: AudioPanelSegment) => {
    if (segment.kind === 'narration') onCommitNarrationSegments?.();
    else onCommitImportedSegments?.();
  };

  const commitSelected = (targets: AudioPanelSegment[]) => {
    if (targets.some((segment) => segment.kind === 'imported')) onCommitImportedSegments?.();
    if (targets.some((segment) => segment.kind === 'narration')) onCommitNarrationSegments?.();
  };

  const finishPanelInteraction = (commit: () => void) => {
    commit();
  };

  const handleRateChange = (segment: AudioPanelSegment, rate: number) => {
    const patch = { playbackRate: clampRate(rate) };
    setSegmentDraft(segment, patch);
    if (segment.kind === 'narration') {
      updateSegment(segment, patch);
      return;
    }
    schedulePreviewDraft(segment, patch);
  };

  const handleTrimChange = (
    seg: AudioPanelSegment,
    next: Partial<Pick<ImportedAudioSegment, 'inPoint' | 'outPoint'>>,
  ) => {
    const inPoint = next.inPoint ?? seg.inPoint;
    const outPoint = next.outPoint ?? seg.outPoint;
    const safeIn = Math.min(Math.max(inPoint, 0), Math.max(seg.duration - 0.05, 0));
    const safeOut = Math.min(Math.max(outPoint, safeIn + 0.05), seg.duration);
    const patch = { inPoint: safeIn, outPoint: safeOut };
    setSegmentDraft(seg, patch);
    if (seg.kind === 'narration') {
      updateSegment(seg, patch);
      return;
    }
    schedulePreviewDraft(seg, patch);
  };

  const handleApplyRateToAll = (rate: number) => {
    const nextRate = clampRate(rate);
    setBulkRate(nextRate);
    selected.forEach((segment) => {
      const patch = { playbackRate: nextRate };
      setSegmentDraft(segment, patch);
      if (segment.kind === 'narration') {
        updateSegment(segment, patch);
      } else {
        schedulePreviewDraft(segment, patch);
      }
    });
  };

  const commitApplyRateToAll = () => {
    const nextRate = clampRate(bulkRate);
    selected.forEach((segment) => updateSegment(segment, { playbackRate: nextRate }));
    commitSelected(selected);
  };

  const handleAutoArrange = () => {
    const allowedOverlap = 0.3;
    const subtitleById = new Map(subtitleSegments.map((subtitle) => [subtitle.id, subtitle]));
    const getOverlapDebt = (
      start: number,
      duration: number,
      intervals: Array<{ start: number; end: number }>,
    ) => {
      const end = start + duration;
      let debt = 0;
      for (const interval of intervals) {
        const overlap = Math.min(end, interval.end) - Math.max(start, interval.start);
        if (overlap > allowedOverlap) debt += overlap - allowedOverlap;
      }
      return debt;
    };

    const arrangeImported = () => {
      const targets = selected
        .filter((segment) => segment.kind === 'imported')
        .sort((left, right) => left.startTime - right.startTime || left.id.localeCompare(right.id));
      if (targets.length <= 1) return;

      const selectedIds = new Set(targets.map((segment) => segment.id));
      const blockers = importedSegments
        .filter((segment) => !selectedIds.has(segment.id))
        .map((segment) => ({
          start: segment.startTime,
          end: segment.startTime + getTimelineDuration({ ...segment, kind: 'imported' }),
        }))
        .sort((left, right) => left.start - right.start);

      let cursor = 0;
      for (const segment of targets) {
        const segmentDuration = getTimelineDuration(segment);
        const desiredStart = Math.max(segment.startTime, cursor);
        let nextStart = Math.max(0, desiredStart);
        for (const blocker of blockers) {
          if (getOverlapDebt(nextStart, segmentDuration, [blocker]) <= 0) break;
          nextStart = Math.max(nextStart, blocker.end - allowedOverlap);
        }
        updateSegment(segment, { startTime: nextStart });
        cursor = nextStart + segmentDuration - allowedOverlap;
      }
    };

    const arrangeNarration = () => {
      const targets = selected
        .filter((segment) => segment.kind === 'narration')
        .sort((left, right) => {
          const leftSubtitle = left.sourceSubtitleId
            ? subtitleById.get(left.sourceSubtitleId)
            : undefined;
          const rightSubtitle = right.sourceSubtitleId
            ? subtitleById.get(right.sourceSubtitleId)
            : undefined;
          return (leftSubtitle?.startTime ?? left.startTime) -
            (rightSubtitle?.startTime ?? right.startTime) ||
            left.id.localeCompare(right.id);
        });
      if (targets.length <= 1) return;

      const selectedIds = new Set(targets.map((segment) => segment.id));
      const fixedIntervals = narrationSegments
        .filter((segment) => !selectedIds.has(segment.id))
        .map((segment) => ({
          start: segment.startTime,
          end: segment.startTime + getTimelineDuration({ ...segment, kind: 'narration' }),
        }))
        .sort((left, right) => left.start - right.start);

      const placed: Array<{ start: number; end: number }> = [];
      targets.forEach((segment, index) => {
        const segmentDuration = getTimelineDuration(segment);
        const anchorSubtitle = segment.sourceSubtitleId
          ? subtitleById.get(segment.sourceSubtitleId)
          : undefined;
        const anchorStart = anchorSubtitle?.startTime ?? segment.startTime;
        const anchorEnd = anchorSubtitle?.endTime;
        const nextAnchor = targets
          .slice(index + 1)
          .map((candidate) =>
            candidate.sourceSubtitleId
              ? subtitleById.get(candidate.sourceSubtitleId)?.startTime
              : undefined,
          )
          .find((value): value is number => typeof value === 'number');
        const leftSlack = Math.min(1.2, Math.max(0.45, segmentDuration * 0.35));
        const rightSlack = Math.min(1.8, Math.max(0.75, segmentDuration * 0.5));
        const candidates = new Set<number>([
          anchorStart,
          segment.startTime,
          Math.max(0, anchorStart - leftSlack),
          anchorStart + rightSlack,
          ...(anchorEnd !== undefined ? [Math.max(0, anchorEnd - segmentDuration)] : []),
          ...(nextAnchor !== undefined ? [Math.max(0, nextAnchor - segmentDuration + allowedOverlap)] : []),
        ]);

        for (let offset = -leftSlack; offset <= rightSlack + 0.0001; offset += 0.05) {
          candidates.add(Math.max(0, anchorStart + offset));
        }
        [...fixedIntervals, ...placed].forEach((interval) => {
          candidates.add(Math.max(0, interval.start - segmentDuration + allowedOverlap));
          candidates.add(Math.max(0, interval.end - allowedOverlap));
        });

        let bestStart = Math.max(0, anchorStart);
        let bestScore = Number.POSITIVE_INFINITY;
        for (const candidate of candidates) {
          const start = Math.max(0, candidate);
          const end = start + segmentDuration;
          const overlapDebt = getOverlapDebt(start, segmentDuration, [...fixedIntervals, ...placed]);
          const anchorDistance = Math.abs(start - anchorStart);
          const earlyDebt = Math.max(0, anchorStart - start - leftSlack);
          const lateDebt = Math.max(0, start - anchorStart - rightSlack);
          const nextCrowdDebt = nextAnchor !== undefined
            ? Math.max(0, end - nextAnchor - allowedOverlap)
            : 0;
          const subtitleTailDebt = anchorEnd !== undefined
            ? Math.max(0, end - anchorEnd - rightSlack)
            : 0;
          const score =
            overlapDebt * overlapDebt * 500 +
            nextCrowdDebt * nextCrowdDebt * 18 +
            subtitleTailDebt * subtitleTailDebt * 4 +
            anchorDistance * anchorDistance * 2 +
            earlyDebt * earlyDebt * 20 +
            lateDebt * lateDebt * 12;
          if (score < bestScore || (score === bestScore && start < bestStart)) {
            bestScore = score;
            bestStart = start;
          }
        }

        placed.push({ start: bestStart, end: bestStart + segmentDuration });
        placed.sort((left, right) => left.start - right.start);
        updateSegment(segment, { startTime: bestStart });
      });
    };

    beginBatch?.();
    arrangeImported();
    arrangeNarration();
    commitBatch?.();
    commitSelected(selected);
  };

  const renderSpeedControl = (
    value: number,
    onChange: (rate: number) => void,
    onCommit: () => void,
    label: string,
  ) => (
    <div className="audio-panel-speed-row flex items-center gap-2 rounded-lg border border-outline/30 bg-surface-container-high/40 p-2">
      <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
        {label}
      </span>
      <input
        type="range"
        min={RATE_MIN}
        max={RATE_MAX}
        step={0.05}
        value={value}
        onChange={(event) => onChange(parseFloat(event.target.value))}
        onPointerUp={onCommit}
        onPointerCancel={onCommit}
        onKeyUp={onCommit}
        onBlur={onCommit}
        className="audio-panel-speed-slider flex-1"
      />
      <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
        {value.toFixed(2)}×
      </span>
    </div>
  );

  const renderTrimRange = (segment: AudioPanelSegment) => {
    const minGap = 0.05;
    const inPct = Math.max(0, Math.min(100, (segment.inPoint / Math.max(segment.duration, 0.001)) * 100));
    const outPct = Math.max(0, Math.min(100, (segment.outPoint / Math.max(segment.duration, 0.001)) * 100));
    return (
      <div className="audio-panel-trim-range rounded-lg border border-outline/30 bg-surface-container-high/40 p-2">
        <div className="audio-panel-trim-header mb-2 flex items-center justify-between text-[10px] font-medium text-on-surface-variant">
          <span>{t.audioPanelTrimIn}</span>
          <span className="tabular-nums text-on-surface">
            {formatSec(segment.inPoint)}s - {formatSec(segment.outPoint)}s
          </span>
          <span>{t.audioPanelTrimOut}</span>
        </div>
        <div className="audio-panel-trim-slider relative h-7">
          <div className="audio-panel-trim-rail absolute left-0 right-0 rounded-full bg-[var(--ui-surface-1)]" />
          <div
            className="audio-panel-trim-active absolute rounded-full bg-[var(--primary-color)]"
            style={{ left: `${inPct}%`, right: `${100 - outPct}%` }}
          />
          <input
            type="range"
            min={0}
            max={segment.duration}
            step={0.01}
            value={segment.inPoint}
            onPointerUp={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onPointerCancel={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onKeyUp={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onBlur={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onChange={(event) =>
              handleTrimChange(segment, {
                inPoint: Math.min(parseFloat(event.target.value), segment.outPoint - minGap),
              })
            }
            className="audio-panel-trim-start-slider absolute left-0 right-0 w-full appearance-none bg-transparent"
          />
          <input
            type="range"
            min={0}
            max={segment.duration}
            step={0.01}
            value={segment.outPoint}
            onPointerUp={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onPointerCancel={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onKeyUp={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onBlur={() => finishPanelInteraction(() => commitSegmentDraft(segment))}
            onChange={(event) =>
              handleTrimChange(segment, {
                outPoint: Math.max(parseFloat(event.target.value), segment.inPoint + minGap),
              })
            }
            className="audio-panel-trim-end-slider absolute left-0 right-0 w-full appearance-none bg-transparent"
          />
        </div>
      </div>
    );
  };

  return (
    <PanelCard className="audio-panel">
      <div className="audio-panel-body space-y-3">
        <p className="audio-panel-hint text-[11px] leading-4 text-on-surface-variant">
          {t.audioPanelHint}
        </p>

        {selected.length === 0 ? (
          <div className="audio-panel-empty rounded-lg border border-outline/30 bg-surface-container-high/50 p-3 text-[11px] text-on-surface-variant">
            {t.audioPanelEmpty}
          </div>
        ) : (
          <>
            <div className="audio-panel-header flex items-center gap-1.5 text-[11px] font-semibold text-on-surface">
              <AudioLines className="h-3.5 w-3.5 text-[var(--primary-color)]" />
              {t.audioPanelSelectedCount.replace('{count}', String(headerCount))}
            </div>

            {selected.length > 1 ? (
              <div className="audio-panel-multi-editor space-y-2">
                <div className={`audio-panel-action-row grid gap-2 ${selectedNarrationWithSubtitleCount > 0 ? 'grid-cols-2' : 'grid-cols-1'}`}>
                  <button
                    type="button"
                    className="audio-panel-auto-arrange-button flex w-full items-center justify-center gap-2 rounded-lg border border-outline/30 bg-surface-container-high/50 px-3 py-2 text-[11px] font-semibold text-on-surface transition-colors hover:border-[var(--primary-color)] hover:bg-[color:color-mix(in_srgb,var(--primary-color)_12%,transparent)]"
                    onClick={handleAutoArrange}
                  >
                    <Rows3 className="h-3.5 w-3.5 text-[var(--primary-color)]" />
                    {t.audioPanelAutoArrange}
                  </button>
                  {selectedNarrationWithSubtitleCount > 0 && (
                    <button
                      type="button"
                      className="audio-panel-align-subtitles-button flex w-full items-center justify-center gap-2 rounded-lg border border-outline/30 bg-surface-container-high/50 px-3 py-2 text-[11px] font-semibold text-on-surface transition-colors hover:border-[var(--primary-color)] hover:bg-[color:color-mix(in_srgb,var(--primary-color)_12%,transparent)]"
                      onClick={onAlignSubtitlesToNarration}
                    >
                      <Captions className="h-3.5 w-3.5 text-[var(--primary-color)]" />
                      {t.audioPanelAlignSubtitlesToAudio}
                    </button>
                  )}
                </div>
                {renderSpeedControl(
                  bulkRate,
                  handleApplyRateToAll,
                  () => finishPanelInteraction(commitApplyRateToAll),
                  t.audioPanelBulkSpeed,
                )}
              </div>
            ) : (
              <div className="audio-panel-single-editor w-full space-y-2 text-[11px] font-semibold text-on-surface">
                <div className="audio-panel-selected-name min-w-0 rounded-lg border border-outline/30 bg-surface-container-high/40 p-2">
                  <span className="block truncate text-[11px] font-medium text-on-surface">
                    {selected[0].name}
                  </span>
                  <span className="block text-[9px] font-medium uppercase tracking-wide text-on-surface-variant">
                    {selected[0].kind === 'narration' ? t.tabNarration : t.tabAudio}
                  </span>
                </div>
                {renderSpeedControl(
                  getEffectiveSegment(selected[0]).playbackRate ?? 1,
                  (rate) => handleRateChange(selected[0], rate),
                  () => finishPanelInteraction(() => commitSegmentDraft(selected[0])),
                  t.audioPanelSpeed,
                )}
                {renderTrimRange(getEffectiveSegment(selected[0]))}
              </div>
            )}
          </>
        )}
      </div>
    </PanelCard>
  );
}
