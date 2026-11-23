import type {
  AudioSubtitleClipTransform,
  SubtitleGenerationIndicator,
} from '@/lib/subtitleGenerationPlan';
import type { Translations } from '@/i18n';
import type { PersistOptions } from '@/hooks/useSequenceComposition';
import type { Project, ProjectComposition, VideoSegment } from '@/types/video';
import type { BaseAsyncJobStatus } from '@/hooks/asyncJobTypes';
import type { SubtitleSourceOwner } from './subtitleSourceOwnership';

export type SubtitleMethod =
  | 'groq-whisper-accurate'
  | 'groq-whisper-large-v3-turbo'
  | 'gemini-3-1-flash-lite'
  | 'gemini-3-flash-preview';

export interface SubtitleClipResultSegment {
  startTime: number;
  endTime: number;
  text: string;
  splitGroupId?: string;
  splitGroupIndex?: number;
  splitGroupCount?: number;
  splitGroupText?: string;
  splitGroupStartTime?: number;
  splitGroupEndTime?: number;
}

export interface SubtitleClipResult {
  clipId: string;
  isPartial: boolean;
  segments: SubtitleClipResultSegment[];
}

export interface SubtitleSkippedClip {
  clipId: string;
  reason: string;
}

export interface SubtitleMethodCapability {
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}

export interface SubtitleGenerationCapabilities {
  methods: SubtitleMethodCapability[];
}

export interface SubtitleJobStatus extends BaseAsyncJobStatus {
  activeClipId?: string | null;
  totalClips: number;
  completedClips: number;
  resultsRevision: number;
  results: SubtitleClipResult[];
  skipped: SubtitleSkippedClip[];
}

export type SubtitleJobViewStatus = Omit<SubtitleJobStatus, 'results'>;

export interface ResumableSubtitleGenerationRun {
  projectEpoch: number;
  jobId: string;
  projectKey: string | null;
  jobContext: SubtitleJobContext | null;
  status: SubtitleJobViewStatus | null;
  lastKnownResultsRevision: number;
  lastAppliedResultsKey: string;
  lastStatusViewKey: string;
  staleResults?: boolean;
  pendingResults?: SubtitleClipResult[];
}

export interface SubtitleJobContext {
  projectKey: string | null;
  replacementRangesByClip: Record<
    string,
    Array<{ startTime: number; endTime: number }>
  >;
  indicator: SubtitleGenerationIndicator;
  sourceTypeForNative: 'video' | 'mic' | 'audio';
  clipTransformsByClip: Record<string, AudioSubtitleClipTransform>;
  sourceOwnersByClip: Record<string, SubtitleSourceOwner>;
}

export interface UseSubtitleGenerationParams {
  t: Translations;
  projectResetKey?: string | null;
  segment: VideoSegment | null;
  segmentRef: React.MutableRefObject<VideoSegment | null>;
  currentProjectDataRef: React.MutableRefObject<Project | null>;
  currentProjectIdRef: React.MutableRefObject<string | null>;
  currentRawVideoPathRef: React.MutableRefObject<string>;
  currentRawMicAudioPathRef: React.MutableRefObject<string>;
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
  activeClipId: string | null | undefined;
  loadedClipIdRef: React.MutableRefObject<string | null>;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  setActivePanel: (
    panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles',
  ) => void;
  persistProject?: (opts?: PersistOptions) => Promise<void>;
}
