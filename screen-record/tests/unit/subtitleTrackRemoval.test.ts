import { describe, expect, it } from 'vitest';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getVisibleSubtitleSegments,
  removeSubtitleTrack,
} from '@/lib/subtitleTracks';
import type { SubtitleChainItem, SubtitleSegment, SubtitleTrack, VideoSegment } from '@/types/video';

const cue = (text: string) => ({ id: 's1', startTime: 0, endTime: 2, text, style: {} }) as unknown as SubtitleSegment;
const track = (id: string, kind: SubtitleTrack['kind'], text: string): SubtitleTrack =>
  ({ id, kind, targetLanguage: kind === 'translation' ? id : null, slotLabel: null, segments: [cue(text)] });

function customView(chain: SubtitleChainItem[]): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 4,
    subtitleTracks: [
      track(ORIGINAL_SUBTITLE_TRACK_ID, 'original', 'Hello'),
      track('ja', 'translation', 'Konnichiwa'),
      track('fr', 'translation', 'Bonjour'),
    ],
    activeSubtitleView: { kind: 'custom' },
    subtitleCustomChain: chain,
  } as unknown as VideoSegment;
}

const separator: SubtitleChainItem = { type: 'delimiter', value: ' / ' };
const shown = (segment: VideoSegment) => getVisibleSubtitleSegments(segment).map((item) => item.text);

describe('removing a subtitle track from the custom chain', () => {
  it('drops the separator that joined the removed track', () => {
    const segment = customView([{ type: 'track', trackId: ORIGINAL_SUBTITLE_TRACK_ID }, separator, { type: 'track', trackId: 'ja' }]);
    expect(shown(removeSubtitleTrack(segment, 'ja'))).toEqual(['Hello']);
  });

  it('falls back to the original text once every chained track is gone', () => {
    const segment = customView([{ type: 'track', trackId: 'ja' }, separator, { type: 'track', trackId: 'fr' }]);
    const next = removeSubtitleTrack(removeSubtitleTrack(segment, 'ja'), 'fr');
    expect(shown(next)).toEqual(['Hello']);
  });

  it('keeps decorative delimiters around the tracks that remain', () => {
    const segment = customView([
      { type: 'delimiter', value: '[' },
      { type: 'track', trackId: 'ja' },
      separator,
      { type: 'track', trackId: 'fr' },
      { type: 'delimiter', value: ']' },
    ]);
    expect(shown(removeSubtitleTrack(segment, 'ja'))).toEqual(['[Bonjour]']);
  });
});
