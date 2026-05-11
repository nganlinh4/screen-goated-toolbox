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
import { isScreenRecordTestHarnessEnabled } from "./browserIpcMock";
import {
  createSyntheticProjectFixture,
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
  loadSyntheticProject: (profile?: SyntheticProjectProfile) => ScreenRecordEditorStateSnapshot;
  getEditorState: () => ScreenRecordEditorStateSnapshot;
  setCurrentTime: (time: number) => void;
  startPerfProbe: () => void;
  stopPerfProbe: () => FrontendFrameProbeSummary;
  startAction: (label: string) => void;
  endAction: (label: string) => void;
  resetPerf: () => void;
  getPerfSnapshot: () => FrontendPerfSnapshot;
  getDomStats: () => ScreenRecordDomStats;
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
  setCurrentTime: (time: number) => void;
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
  testWindow.__SGT_TEST__ = {
    loadSyntheticProject: (profile = "small") => {
      const project = createSyntheticProjectFixture({ profile });
      options.loadProject(project);
      return summarizeProject(project);
    },
    getEditorState: () => summarizeState(options),
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
  };
  return () => {};
}
