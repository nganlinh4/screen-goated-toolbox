import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useMediaEngine } from '@/hooks/useMediaEngine';
import { useVideoPlayback } from '@/hooks/useVideoState';
import { getCompositionClipRenderSegment, getCompositionSequenceRenderSegment } from '@/lib/compositionSubtitles';
import { createSubtitleTrackStateFromSegments } from '@/lib/subtitleTracks';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import { DEFAULT_BACKGROUND_CONFIG } from '@/lib/appUtils';
import { DEFAULT_WEBCAM_CONFIG } from '@/lib/webcam';
import type { ProjectComposition, SubtitleSegment, VideoSegment } from '@/types/video';

vi.mock('@/hooks/useVideoState', () => ({
  useVideoPlayback: vi.fn(() => ({})),
  useRecording: vi.fn(() => ({ mousePositions: [] })),
}));

function cue(id: string, startTime: number, endTime: number): SubtitleSegment {
  return { id, startTime, endTime, text: id, style: defaultSubtitleStyle(),
    sourceGroup: { kind: 'audio', audioSegmentId: 'audio-owner' },
  };
}
function composition(cues: SubtitleSegment[]): ProjectComposition {
  const segment = { trimStart: 0, trimEnd: 1,
    trimSegments: [{ id: 'first-range', startTime: 0, endTime: 1 }],
    ...createSubtitleTrackStateFromSegments([]),
  } as VideoSegment;
  const clip = { id: 'first', role: 'root' as const, name: 'First', duration: 1, segment,
    backgroundConfig: DEFAULT_BACKGROUND_CONFIG, mousePositions: [],
  };
  return { mode: 'separate', selectedClipId: 'second', focusedClipId: 'second',
    clips: [clip, { ...clip, id: 'second', duration: 21, segment: { ...segment,
      trimStart: 8, trimEnd: 21, trimSegments: [
        { id: 'left', startTime: 8, endTime: 9 }, { id: 'right', startTime: 20, endTime: 21 },
      ],
    } }],
    globalSegment: { ...segment, trimEnd: 3, ...createSubtitleTrackStateFromSegments(cues) },
  };
}

describe('sequence-owned subtitle projection', () => {
  it('uses the same trim-aware view for live playback without changing editor state', () => {
    const source = composition([cue('audio', 1.25, 2.75)]);
    const local = source.clips[1].segment;
    const before = JSON.stringify(source);
    const owner = { current: 'second' as string | null };
    const setSegment = vi.fn();
    const { result, rerender } = renderHook(() => useMediaEngine({
      segment: local, composition: source, backgroundConfig: DEFAULT_BACKGROUND_CONFIG,
      webcamConfig: DEFAULT_WEBCAM_CONFIG, isCropping: false, isCanvasResizeDragging: false,
      isTimelineOnly: false, setSegment,
    }));
    act(() => result.current.bindSubtitleClipOwner(owner));
    rerender();
    const playbackSegment = vi.mocked(useVideoPlayback).mock.lastCall?.[0].segment;
    expect(playbackSegment?.subtitleSegments).toEqual([
      expect.objectContaining({ id: 'audio', startTime: 8.25, endTime: 20.75 }),
    ]);
    expect(playbackSegment).toEqual(getCompositionClipRenderSegment(source, 'second', local));
    expect(setSegment).not.toHaveBeenCalled();
    expect(JSON.stringify(source)).toBe(before);
    owner.current = 'retired-clip';
    rerender();
    expect(vi.mocked(useVideoPlayback).mock.lastCall?.[0].segment).toBe(local);
  });

  it('clips crossing cues at clip boundaries and projects split-group bounds', () => {
    const crossing = { ...cue('crossing', 0.75, 1.25), splitGroupStartTime: 0.75, splitGroupEndTime: 1.25 };
    const source = composition([crossing, cue('past-end', 2.75, 3.25), cue('outside', 3, 4)]);
    const first = getCompositionClipRenderSegment(source, 'first', source.clips[0].segment);
    const second = getCompositionClipRenderSegment(source, 'second', source.clips[1].segment);
    expect(first.subtitleSegments).toEqual([
      expect.objectContaining({ id: 'crossing', startTime: 0.75, endTime: 1, splitGroupEndTime: 1 }),
    ]);
    expect(second.subtitleSegments).toEqual([
      expect.objectContaining({ id: 'crossing', startTime: 8, endTime: 8.25, splitGroupStartTime: 8 }),
      expect.objectContaining({ id: 'past-end', startTime: 20.75, endTime: 21 }),
    ]);
    expect(getCompositionSequenceRenderSegment(source)?.subtitleSegments).toEqual([
      expect.objectContaining({ id: 'crossing', startTime: 0.75, endTime: 1.25 }),
      expect.objectContaining({ id: 'past-end', startTime: 2.75, endTime: 3 }),
    ]);
  });

  it('keeps local video captions and ignores their stale sequence mirrors', () => {
    const source = composition([cue('audio', 0.5, 0.75)]);
    const video = { ...cue('video', 0.1, 0.3), sourceGroup: { kind: 'video' as const } };
    source.clips[0].segment = { ...source.clips[0].segment, ...createSubtitleTrackStateFromSegments([video]) };
    source.globalSegment = { ...source.globalSegment!, ...createSubtitleTrackStateFromSegments([
      cue('audio', 0.5, 0.75), { ...video, text: 'stale mirror' },
    ]) };
    const projected = getCompositionClipRenderSegment(source, 'first', source.clips[0].segment);
    expect(projected.subtitleSegments?.map((entry) => entry.text)).toEqual(['video', 'audio']);
  });
});
