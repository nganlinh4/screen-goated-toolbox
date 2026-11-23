import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useStopRecording, type UseStopRecordingParams } from "@/hooks/useStopRecording";
import { invoke } from "@/lib/ipc";
import { projectManager } from "@/lib/projectManager";
import { recordingHandoff } from "@/lib/recordingHandoff";
import { beginAudioImportWrite, finishAudioImportWrite, markAudioImportCommitted } from '@/lib/audioImportWriteFence';
import { useImportedAudioImport } from '@/hooks/useImportedAudioImport';
import { createAudioPlaceholderVideo, importAudioPathToManagedMediaFile } from '@/lib/mediaServer';
import { claimResumableStart, releaseResumableStart } from '@/hooks/resumableJobRegistry';
import { clearSubtitleShutdownBlocked, cancelSubtitleShutdownOverride, armSubtitleShutdownOverride, currentSubtitleShutdownOverride, subtitleShutdownOverrideArmed } from '@/hooks/subtitleShutdown';
import type { Project, VideoSegment } from "@/types/video";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn(async () => null), logToHost: vi.fn() }));
vi.mock('@/lib/mediaServer', () => ({ createAudioPlaceholderVideo: vi.fn(), importAudioPathToManagedMediaFile: vi.fn(), importAudioToManagedMediaFile: vi.fn() }));
vi.mock("@/hooks/useSettings", () => ({ useSettings: () => ({ t: { recorderShutdownFailed: "Save failed", audioRecoveryShutdownBlocked: "Keep the window open and retry audio recovery", subtitleShutdownBlocked: 'Subtitle work is unfinished' } }) }));
vi.mock("@/lib/projectManager", () => ({ projectManager: { saveProject: vi.fn() } }));
vi.mock("@/lib/projectComposition", () => ({ ensureProjectComposition: vi.fn(() => ({})) }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function params(saveAck: () => Promise<void>): UseStopRecordingParams {
  const noop = vi.fn();
  return {
    handleStopRecording: vi.fn(async () => ({
      mouseData: [],
      initialSegment: { trimEnd: 1 } as VideoSegment,
      videoUrl: "blob:recording",
      webcamVideoUrl: null,
      recordingMode: "withoutCursor",
      rawVideoPath: "C:/recordings/original.mp4",
      rawMicAudioPath: null,
      rawWebcamVideoPath: null,
      capturedFps: 30,
      recordingId: "a".repeat(32),
      markSaved: saveAck,
    })),
    backgroundConfig: {} as UseStopRecordingParams["backgroundConfig"],
    generateThumbnail: () => null,
    projects: { setCurrentProjectId: noop, loadProjects: vi.fn(async () => undefined) },
    rawAutoCopyEnabled: false, rawSaveDir: "", flashRawSavedButton: noop,
    setShowRawVideoDialog: noop, setShowExportSuccessDialog: noop, requestCloseProjects: noop,
    setComposition: noop, setCurrentProjectData: noop, setLoadedClipId: noop,
    setLastCaptureFps: noop, setCurrentRecordingMode: noop, setCurrentRawVideoPath: noop,
    setCurrentRawMicAudioPath: noop, setCurrentRawWebcamVideoPath: noop,
    setLastRawSavedPath: noop, setIsRawActionBusy: noop, setWebcamConfig: noop, setError: noop,
  };
}

const observedRequests = new Set<number>();
const observePrepare = (event: Event) => {
  const requestId = (event as CustomEvent<{ requestId?: number }>).detail?.requestId;
  if (requestId !== undefined) observedRequests.add(requestId);
};
const cancelRequest = (requestId: number) => window.dispatchEvent(new CustomEvent('sr-cancel-shutdown', { detail: { requestId } }));

beforeEach(() => {
  window.addEventListener('sr-prepare-shutdown', observePrepare);
  vi.mocked(invoke).mockClear();
  vi.mocked(projectManager.saveProject).mockReset();
});
afterEach(async () => {
  for (const id of observedRequests) cancelRequest(id);
  observedRequests.clear();
  window.removeEventListener('sr-prepare-shutdown', observePrepare);
  cancelSubtitleShutdownOverride();
  clearSubtitleShutdownBlocked();
  recordingHandoff.cancelPrepare();
  await recordingHandoff.stop(async () => true);
});

it('blocks shutdown while a subtitle start is pending, then permits retry when it retires', async () => {
  const options = params(async () => undefined);
  renderHook(() => useStopRecording(options));
  const claim = claimResumableStart('subtitle-generation');
  expect(claim).not.toBeNull();
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 36 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 36, saved: false }));
  act(() => window.dispatchEvent(new Event('sr-shutdown-failed')));
  expect(options.setError).toHaveBeenCalledWith('Subtitle work is unfinished');
  releaseResumableStart('subtitle-generation', claim!);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 37 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 37, saved: true }));
});

it('ignores a delayed cancellation for an earlier prepare after a new acknowledgement', async () => {
  renderHook(() => useStopRecording(params(async () => undefined)));
  const claim = claimResumableStart('subtitle-generation')!;
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 51 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 51, saved: false }));
  releaseResumableStart('subtitle-generation', claim);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 52 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 52, saved: true }));
  act(() => { cancelRequest(51); });
  expect(() => beginAudioImportWrite('after-stale-cancel')).toThrow('recorder is closing');
  expect(recordingHandoff.beginStart()).toBe(false);
  act(() => { cancelRequest(52); });
  const fence = beginAudioImportWrite('after-current-cancel');
  finishAudioImportWrite(fence, false);
});

it('uses a confirmed subtitle override only once and keeps audio admission closed', async () => {
  renderHook(() => useStopRecording(params(async () => undefined)));
  const claim = claimResumableStart('subtitle-generation')!;
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 38 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 38, saved: false }));
  act(() => window.dispatchEvent(new Event('sr-shutdown-failed')));
  expect(armSubtitleShutdownOverride()).toBe(true);
  expect(subtitleShutdownOverrideArmed()).toBe(true);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 390 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 390, saved: false }));
  expect(subtitleShutdownOverrideArmed()).toBe(true);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 39, subtitleOverrideId: currentSubtitleShutdownOverride() } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 39, saved: true }));
  expect(subtitleShutdownOverrideArmed()).toBe(false);
  expect(() => beginAudioImportWrite('after-forced-ack')).toThrow('recorder is closing');
  act(() => { cancelRequest(39); });
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 40 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 40, saved: false }));
  releaseResumableStart('subtitle-generation', claim);
});

it('clears only the matching override when native close fails before prepare', () => {
  renderHook(() => useStopRecording(params(async () => undefined)));
  const claim = claimResumableStart('subtitle-generation')!;
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 41 } })));
  act(() => window.dispatchEvent(new Event('sr-shutdown-failed')));
  expect(armSubtitleShutdownOverride()).toBe(true);
  const first = currentSubtitleShutdownOverride();
  act(() => window.dispatchEvent(new CustomEvent('sr-shutdown-failed', { detail: { subtitleOverrideId: first } })));
  expect(subtitleShutdownOverrideArmed()).toBe(false);
  expect(armSubtitleShutdownOverride()).toBe(true);
  const second = currentSubtitleShutdownOverride();
  act(() => window.dispatchEvent(new CustomEvent('sr-shutdown-failed', { detail: { subtitleOverrideId: first } })));
  expect(subtitleShutdownOverrideArmed()).toBe(true);
  act(() => window.dispatchEvent(new CustomEvent('sr-shutdown-failed', { detail: { subtitleOverrideId: second } })));
  expect(subtitleShutdownOverrideArmed()).toBe(false);
  releaseResumableStart('subtitle-generation', claim);
});

it('keeps audio admission closed after a successful acknowledgement and rerender', async () => {
  const initial = params(async () => undefined);
  const hook = renderHook(({ options }) => useStopRecording(options), { initialProps: { options: initial } });
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 31 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 31, saved: true }));
  hook.rerender({ options: { ...initial, backgroundConfig: {} as UseStopRecordingParams['backgroundConfig'] } });
  expect(() => beginAudioImportWrite('after-ack')).toThrow('recorder is closing');
  hook.unmount();
  expect(() => beginAudioImportWrite('after-unmount')).toThrow('recorder is closing');
  act(() => { cancelRequest(31); });
  const fence = beginAudioImportWrite('after-cancel');
  finishAudioImportWrite(fence, false);
});

it('rejects shutdown while audio is being imported before project attachment', async () => {
  const pending = deferred<{ path: string; duration: number }>();
  vi.mocked(importAudioPathToManagedMediaFile).mockReturnValue(pending.promise);
  const options = params(async () => undefined);
  renderHook(() => useStopRecording(options));
  const audio = renderHook(() => useImportedAudioImport({
    getCurrentProjectId: () => 'project-a', onAttachToCurrentProject: vi.fn(), onCreateAudioProject: vi.fn(),
  }));
  let operation!: Promise<void>;
  act(() => { operation = audio.result.current.importAudioPath('C:\\track.wav'); });
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 32 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 32, saved: false }));
  pending.resolve({ path: 'C:\\managed\\track.wav', duration: 1 });
  await act(async () => operation);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 33 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 33, saved: true }));
});

it('rejects shutdown while a new audio project is being saved', async () => {
  const save = deferred<Project>();
  vi.mocked(importAudioPathToManagedMediaFile).mockResolvedValue({ path: 'C:\\managed\\track.wav', duration: 1 });
  vi.mocked(createAudioPlaceholderVideo).mockResolvedValue({ path: 'C:\\managed\\placeholder.mp4' } as Awaited<ReturnType<typeof createAudioPlaceholderVideo>>);
  vi.mocked(projectManager.saveProject).mockReturnValue(save.promise);
  renderHook(() => useStopRecording(params(async () => undefined)));
  const audio = renderHook(() => useImportedAudioImport({
    getCurrentProjectId: () => null, onAttachToCurrentProject: vi.fn(), onCreateAudioProject: vi.fn(async () => false),
  }));
  let operation!: Promise<void>;
  act(() => { operation = audio.result.current.importAudioPath('C:\\track.wav'); });
  await waitFor(() => expect(projectManager.saveProject).toHaveBeenCalledOnce());
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 34 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 34, saved: false }));
  save.resolve({ id: 'new-audio-project' } as Project);
  await act(async () => operation);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 35 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 35, saved: true }));
});

it("acknowledges an idle recorder without trying to save", async () => {
  const options = params(async () => undefined);
  renderHook(() => useStopRecording(options));
  act(() => window.dispatchEvent(new CustomEvent("sr-prepare-shutdown", { detail: { requestId: 3 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("shutdown_ack", { requestId: 3, saved: true }));
  expect(options.handleStopRecording).not.toHaveBeenCalled();
  expect(projectManager.saveProject).not.toHaveBeenCalled();
});

it('rejects native shutdown while audio recovery is pending and allows retry after recovery', async () => {
  const options = params(async () => undefined);
  renderHook(() => useStopRecording(options));
  const fence = beginAudioImportWrite('shutdown-audio-project');
  markAudioImportCommitted(fence.projectId, fence);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 10 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 10, saved: false }));
  expect(options.setError).toHaveBeenCalledWith('Keep the window open and retry audio recovery');
  act(() => window.dispatchEvent(new Event('sr-shutdown-failed')));
  expect(options.setError).toHaveBeenLastCalledWith('Keep the window open and retry audio recovery');
  finishAudioImportWrite(fence, true);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 11 } })));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shutdown_ack', { requestId: 11, saved: true }));
});

it('prevents a new audio transaction during native shutdown and releases admission on cancel', async () => {
  const options = params(async () => undefined);
  renderHook(() => useStopRecording(options));
  expect(recordingHandoff.beginStart()).toBe(true);
  act(() => window.dispatchEvent(new CustomEvent('sr-prepare-shutdown', { detail: { requestId: 12 } })));
  expect(() => beginAudioImportWrite('during-shutdown')).toThrow('recorder is closing');
  act(() => { cancelRequest(12); });
  const fence = beginAudioImportWrite('after-cancel');
  finishAudioImportWrite(fence, false);
  recordingHandoff.finishStart(false);
  await act(async () => { await Promise.resolve(); });
  expect(invoke).not.toHaveBeenCalledWith('shutdown_ack', expect.objectContaining({ requestId: 12 }));
});

it("offers retry after a stop timeout and clears it only after the late save succeeds", async () => {
  const options = params(async () => undefined);
  const successfulStop = options.handleStopRecording;
  let attempts = 0;
  options.handleStopRecording = vi.fn(async () => ++attempts === 1 ? null : successfulStop());
  vi.mocked(projectManager.saveProject).mockResolvedValue({ id: "late-project" } as Project);
  const required = vi.fn();
  const cleared = vi.fn();
  window.addEventListener("sr-recording-retry-required", required);
  window.addEventListener("sr-recording-retry-cleared", cleared);
  const { result } = renderHook(() => useStopRecording(options));
  expect(recordingHandoff.beginStart()).toBe(true);
  recordingHandoff.finishStart(true);
  await act(async () => { expect(await result.current.onStopRecording()).toBe(false); });
  expect(required).toHaveBeenCalledOnce();
  expect(cleared).not.toHaveBeenCalled();
  act(() => window.dispatchEvent(new Event("sr-recover-recording")));
  await waitFor(() => expect(cleared).toHaveBeenCalledOnce());
  expect(attempts).toBe(2);
  expect(projectManager.saveProject).toHaveBeenCalledOnce();
  window.removeEventListener("sr-recording-retry-required", required);
  window.removeEventListener("sr-recording-retry-cleared", cleared);
});

it("waits for project persistence and handles a timed-out prepare without deleting the original", async () => {
  const save = deferred<Project>();
  const ack = deferred<void>();
  vi.mocked(projectManager.saveProject).mockReturnValue(save.promise);
  const options = params(() => ack.promise);
  renderHook(() => useStopRecording(options));
  expect(recordingHandoff.beginStart()).toBe(true);
  recordingHandoff.finishStart(true);
  act(() => window.dispatchEvent(new CustomEvent("sr-prepare-shutdown", { detail: { requestId: 1 } })));
  await waitFor(() => expect(projectManager.saveProject).toHaveBeenCalledOnce());
  expect(invoke).not.toHaveBeenCalledWith("shutdown_ack", expect.anything());
  act(() => { cancelRequest(1); });
  act(() => window.dispatchEvent(new CustomEvent("sr-prepare-shutdown", { detail: { requestId: 2 } })));
  save.resolve({ id: "saved-project" } as Project);
  await act(async () => Promise.resolve());
  expect(invoke).not.toHaveBeenCalledWith("shutdown_ack", expect.anything());
  ack.resolve();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("shutdown_ack", { requestId: 2, saved: true }));
  expect(projectManager.saveProject).toHaveBeenCalledOnce();
  expect(projectManager.saveProject).toHaveBeenCalledWith(expect.objectContaining({ rawVideoPath: "C:/recordings/original.mp4" }), "a".repeat(32));
  expect(vi.mocked(invoke).mock.calls.every(([command]) => !command.includes("delete"))).toBe(true);
});
