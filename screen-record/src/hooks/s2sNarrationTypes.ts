import type { Translations } from '@/i18n';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { NarrationSegment, ProjectComposition, SubtitleSegment, VideoSegment } from '@/types/video';
import type { PopulateS2sSubtitleTracksOptions } from './s2sNarrationSubtitles';
import type { SubtitleSourceSnapshot } from './subtitleSourceOwnership';

export interface UseS2sNarrationParams {
  getNarrationSourceSnapshot: () => SubtitleSourceSnapshot;
  projectId: string | null;
  currentProjectIdRef: React.MutableRefObject<string | null>;
  backendMode?: 's2s' | 'gemini-translate';
  t: Translations;
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  activeClipId?: string | null;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  sourceType: SubtitleSource;
  selectedRange?: TrackSelectionRange | null;
  targetLanguage: string;
  geminiModel: string;
  geminiVoice: string;
  geminiSpeed: string;
  parallelRequests: number;
  groupTextBudget: number;
  targetSegmentSec: number;
  onApplyNarrationSegments: (segments: NarrationSegment[], replaceSubtitleIds: string[]) => void | Promise<void>;
  onPopulateEmptySubtitles: (
    sourceSegments: SubtitleSegment[], targetSegments: SubtitleSegment[], targetLanguage: string,
    options?: PopulateS2sSubtitleTracksOptions,
  ) => VideoSegment | void;
  onFinalize: () => void | Promise<void>;
}
