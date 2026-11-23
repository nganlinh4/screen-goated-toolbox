import React, { lazy, Suspense } from "react";
import { act, render, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useVideoPlayback } from "@/hooks/useVideoPlayback";
import { PreviewCanvas, type PreviewCanvasProps } from "@/components/PreviewCanvas";
import { createVideoController } from "@/lib/videoController";
import { notifyPreviewMediaMounted } from "@/lib/previewMediaLifecycle";

vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: {} }));
vi.mock("@/lib/videoController", () => ({ createVideoController: vi.fn() }));
vi.mock("@/lib/thumbnailGenerator", () => ({ thumbnailGenerator: {} }));
vi.mock("@/hooks/useSettings", () => ({ useSettings: () => ({ t: {} }) }));
vi.mock("@/components/ImportedAudioPlayers", () => ({ ImportedAudioPlayers: () => null }));

it("attaches the playback controller when a deferred editor mounts after its parent", async () => {
  const destroy = vi.fn();
  vi.mocked(createVideoController).mockReturnValue({ destroy } as unknown as ReturnType<typeof createVideoController>);
  let resolve!: (value: { default: React.ComponentType<{ playback: ReturnType<typeof useVideoPlayback> }> }) => void;
  const Deferred = lazy(() => new Promise<{ default: React.ComponentType<{ playback: ReturnType<typeof useVideoPlayback> }> }>((done) => { resolve = done; }));
  let renders = 0;
  let playback!: ReturnType<typeof useVideoPlayback>;
  function Parent() {
    renders += 1;
    playback = useVideoPlayback({ segment: null, backgroundConfig: {} as PreviewCanvasProps["backgroundConfig"], mousePositionsRef: { current: [] }, isCropping: false, isTimelineOnly: false });
    return <Suspense fallback={<div className="deferred-editor-fallback">Loading editor</div>}><Deferred playback={playback} /></Suspense>;
  }
  const view = render(<Parent />);
  expect(createVideoController).not.toHaveBeenCalled();
  await act(async () => resolve({ default: ({ playback: media }) => <PreviewCanvas {...{
    ...media,
    previewContainerRef: { current: null },
    previousPreloadVideoRef: { current: null },
    previousPreloadAudioRef: { current: null },
    nextPreloadVideoRef: { current: null },
    nextPreloadAudioRef: { current: null },
    isDraggingKeystrokeOverlayRef: { current: false },
    isResizingKeystrokeOverlayRef: { current: false },
    backgroundConfig: {},
    isTimelineOnly: false,
  } as unknown as PreviewCanvasProps} /> }));
  await waitFor(() => expect(playback.videoControllerRef.current).toBeDefined());
  expect(renders).toBe(1);
  expect(createVideoController).toHaveBeenCalledTimes(1);
  notifyPreviewMediaMounted(document.createElement("video"), document.createElement("canvas"));
  expect(createVideoController).toHaveBeenCalledTimes(1);
  view.unmount();
  expect(destroy).toHaveBeenCalledTimes(1);
});
