import type {
  AudioSubtitleClipTransform,
  SubtitleGenerationIndicator,
} from '@/lib/subtitleGenerationPlan';
import type { Translations } from '@/i18n';
import type { PersistOptions } from '@/hooks/useSequenceComposition';
import type { ProjectComposition, VideoSegment } from '@/types/video';

export type SubtitleMethod =
  | 'groq-whisper-accurate'
  | 'groq-whisper-large-v3-turbo'
  | 'gemini-3-1-flash-lite'
  | 'gemini-3-flash-preview'
  | 'qwen-local-0-6b'
  | 'qwen-local-1-7b'
  | 'parakeet-tdt-0-6b-v3';

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

export interface PrepareQwenLocalResult {
  available: boolean;
  startedDownloads: boolean;
  reason?: string | null;
}

export interface PrepareParakeetTdtResult {
  available: boolean;
  startedDownloads: boolean;
  reason?: string | null;
}

export interface SubtitleJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  messageKey?: string | null;
  messageParams?: Record<string, string> | null;
  progress: number;
  activeClipId?: string | null;
  totalClips: number;
  completedClips: number;
  resultsRevision: number;
  results: SubtitleClipResult[];
  skipped: SubtitleSkippedClip[];
  error?: string | null;
}

export type SubtitleJobViewStatus = Omit<SubtitleJobStatus, 'results'>;

export interface SubtitleJobContext {
  replacementRangesByClip: Record<
    string,
    Array<{ startTime: number; endTime: number }>
  >;
  indicator: SubtitleGenerationIndicator;
  sourceTypeForNative: 'video' | 'mic' | 'audio';
  clipTransformsByClip: Record<string, AudioSubtitleClipTransform>;
}

export interface UseSubtitleGenerationParams {
  t: Translations;
  projectResetKey?: string | null;
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
  activeClipId: string | null | undefined;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  setActivePanel: (
    panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles',
  ) => void;
  persistProject?: (opts?: PersistOptions) => Promise<void>;
}
