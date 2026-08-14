import {
  getFrontendPerfSnapshot,
  resetFrontendPerfDiagnostics,
  startFrontendPerfSpan,
  startFrontendFrameProbe,
  endFrontendPerfSpan,
  stopFrontendFrameProbe,
  type FrontendFrameProbeSummary,
  type FrontendPerfSnapshot,
} from "@/lib/frontendPerfDiagnostics";
import type { Project, ProjectComposition, VideoSegment } from "@/types/video";
import { idbCreateProjectBundle } from "@/lib/projectStorage";
import { isScreenRecordTestHarnessEnabled } from "./browserIpcMock";
import {
  createSyntheticProjectFixture,
  type SyntheticProjectOptions,
  type SyntheticProjectProfile,
} from "./syntheticProject";

export interface ScreenRecordEditorStateSnapshot {
  projectId: string | null;
  duration: number;
  subtitleCount: number;
  narrationCount: number;
  audioCount: number;
}

export interface ScreenRecordDomStats {
  subtitleBlocks: number;
  audioBlocks: number;
  narrationBlocks: number;
  textBlocks: number;
  waveformLayers: number;
  totalTimelineBlocks: number;
}

export interface ScreenRecordTestHarness {
  loadSyntheticProject: (profile?: SyntheticProjectProfile) => Promise<ScreenRecordEditorStateSnapshot>;
  loadSyntheticProjectWithOptions: (options?: SyntheticProjectOptions) => Promise<ScreenRecordEditorStateSnapshot>;
  getEditorState: () => ScreenRecordEditorStateSnapshot;
  getNarrationAudioPaths: () => string[];
  setCurrentVideoSource: (url: string | null) => void;
  setCurrentTime: (time: number) => void;
  startPerfProbe: () => void;
  stopPerfProbe: () => FrontendFrameProbeSummary;
  startAction: (label: string) => void;
  endAction: (label: string) => void;
  resetPerf: () => void;
  getPerfSnapshot: () => FrontendPerfSnapshot;
  getDomStats: () => ScreenRecordDomStats;
  startExport: () => Promise<unknown>;
}

type TestWindow = Window & {
  __SGT_TEST__?: ScreenRecordTestHarness;
};

export interface InstallAppTestHarnessOptions {
  loadProject: (project: Project) => void;
  getProjectId: () => string | null;
  getDuration: () => number;
  getSegment: () => VideoSegment | null;
  getComposition: () => ProjectComposition | null;
  setCurrentVideoSource: (url: string | null) => void;
  setCurrentTime: (time: number) => void;
  startExport: () => Promise<unknown>;
}

function summarizeState(options: InstallAppTestHarnessOptions): ScreenRecordEditorStateSnapshot {
  const segment = options.getSegment();
  const composition = options.getComposition();
  return {
    projectId: options.getProjectId(),
    duration: options.getDuration(),
    subtitleCount:
      segment?.subtitleSegments?.length ??
      segment?.subtitleTracks?.reduce((sum, track) => sum + track.segments.length, 0) ??
      0,
    narrationCount: composition?.narrationSegments?.length ?? 0,
    audioCount: composition?.audioSegments?.length ?? 0,
  };
}

function summarizeProject(project: Project): ScreenRecordEditorStateSnapshot {
  const segment = project.segment;
  const composition = project.composition ?? null;
  return {
    projectId: project.id,
    duration: project.duration ?? segment.trimEnd,
    subtitleCount:
      segment.subtitleSegments?.length ??
      segment.subtitleTracks?.reduce((sum, track) => sum + track.segments.length, 0) ??
      0,
    narrationCount: composition?.narrationSegments?.length ?? 0,
    audioCount: composition?.audioSegments?.length ?? 0,
  };
}

export function installScreenRecordAppTestHarness(options: InstallAppTestHarnessOptions) {
  if (!isScreenRecordTestHarnessEnabled()) return () => {};
  const testWindow = window as TestWindow;
  const persistAndLoadFixture = async (project: Project) => {
    await idbCreateProjectBundle(project);
    options.loadProject(project);
    return summarizeProject(project);
  };
  testWindow.__SGT_TEST__ = {
    loadSyntheticProject: async (profile = "small") => {
      const project = createSyntheticProjectFixture({ profile });
      return persistAndLoadFixture(project);
    },
    loadSyntheticProjectWithOptions: async (fixtureOptions = {}) => {
      const project = createSyntheticProjectFixture(fixtureOptions);
      return persistAndLoadFixture(project);
    },
    getEditorState: () => summarizeState(options),
    getNarrationAudioPaths: () =>
      (options.getComposition()?.narrationSegments ?? [])
        .map((segment) => segment.rawAudioPath)
        .filter((path): path is string => typeof path === "string" && path.length > 0),
    setCurrentVideoSource: options.setCurrentVideoSource,
    setCurrentTime: (time: number) => {
      const duration = Math.max(options.getDuration(), 0);
      const nextTime = Number.isFinite(time)
        ? Math.max(0, Math.min(duration, time))
        : 0;
      options.setCurrentTime(nextTime);
    },
    startPerfProbe: () => {
      resetFrontendPerfDiagnostics();
      startFrontendFrameProbe();
    },
    stopPerfProbe: stopFrontendFrameProbe,
    startAction: startFrontendPerfSpan,
    endAction: endFrontendPerfSpan,
    resetPerf: resetFrontendPerfDiagnostics,
    getPerfSnapshot: getFrontendPerfSnapshot,
    getDomStats: () => ({
      subtitleBlocks: document.querySelectorAll(".subtitle-segment").length,
      audioBlocks: document.querySelectorAll(".audio-track-segment").length,
      narrationBlocks: document.querySelectorAll(".narration-track-segment").length,
      textBlocks: document.querySelectorAll(".text-segment").length,
      waveformLayers: document.querySelectorAll(".audio-waveform-layer").length,
      totalTimelineBlocks: document.querySelectorAll(".timeline-block").length,
    }),
    startExport: options.startExport,
  };
  return () => {};
}
