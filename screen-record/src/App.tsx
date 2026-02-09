import { useState, useEffect, useCallback, useRef } from "react";
import { Wand2, MousePointer2 } from "lucide-react";
import "./App.css";
import { Button } from "@/components/ui/button";
import { videoRenderer } from '@/lib/videoRenderer';
import { BackgroundConfig, MousePosition, VideoSegment } from '@/types/video';
import { projectManager } from '@/lib/projectManager';
import { TimelineArea } from '@/components/timeline';
import { useUndoRedo } from '@/hooks/useUndoRedo';
import { useFfmpegSetup, useHotkeys, useKeyviz, useMonitors } from '@/hooks/useAppHooks';
import {
  useVideoPlayback, useRecording, useProjects, useExport,
  useZoomKeyframes, useTextOverlays, useAutoZoom, useCursorHiding
} from '@/hooks/useVideoState';

import { Header } from '@/components/Header';
import { Placeholder, CropOverlay, PlaybackControls, CanvasResizeOverlay } from '@/components/VideoPreview';
import { SidePanel, ActivePanel } from '@/components/SidePanel';
import {
  ProcessingOverlay, ExportDialog,
  MonitorSelectDialog, HotkeyDialog, FfmpegSetupDialog
} from '@/components/Dialogs';
import { ProjectsView } from '@/components/ProjectsView';
import { SettingsContext, useSettingsProvider } from '@/hooks/useSettings';

const ipc = (msg: string) => (window as any).ipc.postMessage(msg);

function ResizeBorders() {
  const resize = (dir: string) => (e: React.MouseEvent) => { e.preventDefault(); ipc(`resize_${dir}`); };
  return (
    <>
      {/* Edges: left / right full-height, bottom full-width (top handled by Header) */}
      <div className="resize-border-left fixed top-0 left-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('w')} />
      <div className="resize-border-right fixed top-0 right-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('e')} />
      <div className="resize-border-bottom fixed bottom-0 left-[14px] right-[14px] h-[6px] z-50 cursor-ns-resize" onMouseDown={resize('s')} />
      {/* Corners */}
      <div className="resize-corner-sw fixed bottom-0 left-0 w-[14px] h-[14px] z-50 cursor-nesw-resize" onMouseDown={resize('sw')} />
      <div className="resize-corner-se fixed bottom-0 right-0 w-[14px] h-[14px] z-50 cursor-nwse-resize" onMouseDown={resize('se')} />
    </>
  );
}

function App() {
  const settings = useSettingsProvider();
  const { t } = settings;
  // Core state
  const { state: segment, setState: setSegment, undo, redo, canUndo, canRedo, beginBatch, commitBatch } = useUndoRedo<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>('zoom');
  const [isCropping, setIsCropping] = useState(false);
  const [recentUploads, setRecentUploads] = useState<string[]>([]);
  const [backgroundConfig, setBackgroundConfig] = useState<BackgroundConfig>({
    scale: 90, borderRadius: 48, backgroundType: 'gradient2', shadow: 100, volume: 1, cursorScale: 5, cursorMovementDelay: 0.03
  });

  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const mousePositionsRef = useRef<MousePosition[]>([]);
  const wheelBatchActiveRef = useRef(false);
  const wheelBatchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoreImageRef = useRef<string | null>(null);

  // Utility hooks
  const { needsSetup, ffmpegInstallStatus, handleCancelInstall } = useFfmpegSetup();
  const { hotkeys, showHotkeyDialog, handleRemoveHotkey, openHotkeyDialog, closeHotkeyDialog } = useHotkeys();
  const { keyvizStatus, toggleKeyviz } = useKeyviz();
  const { monitors, showMonitorSelect, setShowMonitorSelect, getMonitors } = useMonitors();

  // Video playback — mousePositionsRef is shared so useVideoPlayback always reads latest
  const playback = useVideoPlayback({ segment, backgroundConfig, mousePositionsRef, isCropping });
  const {
    currentTime, setCurrentTime, duration, isPlaying, isVideoReady, setIsVideoReady,
    thumbnails, setThumbnails, currentVideo, setCurrentVideo, currentAudio, setCurrentAudio,
    videoRef, audioRef, canvasRef, tempCanvasRef, videoControllerRef,
    renderFrame, togglePlayPause, seek, flushSeek, generateThumbnail, generateThumbnails
  } = playback;

  // Recording
  const recording = useRecording({
    videoControllerRef, videoRef, canvasRef, tempCanvasRef, backgroundConfig,
    setSegment, setCurrentVideo, setCurrentAudio, setIsVideoReady, setThumbnails,
    setDuration: () => {}, setCurrentTime, generateThumbnails, generateThumbnail,
    renderFrame, currentVideo, currentAudio
  });
  const {
    isRecording, recordingDuration, isLoadingVideo, loadingProgress,
    mousePositions, setMousePositions, audioFilePath, error, setError,
    startNewRecording, handleStopRecording
  } = recording;

  // Sync mouse positions to shared ref synchronously during render (NOT in
  // an effect) so useVideoPlayback's effects always read the latest positions.
  // Previously this was a useEffect which ran AFTER useVideoPlayback's effects,
  // causing stale/empty mouse positions on project load → frozen cursor.
  mousePositionsRef.current = mousePositions;

  // Projects
  const projects = useProjects({
    videoControllerRef, setCurrentVideo, setCurrentAudio, setSegment,
    setBackgroundConfig, setMousePositions, setThumbnails, currentVideo, currentAudio
  });

  // Export
  const exportHook = useExport({
    videoRef, canvasRef, tempCanvasRef, audioRef, segment, backgroundConfig,
    mousePositions, audioFilePath, currentVideo
  });

  // Zoom keyframes
  const zoomKeyframes = useZoomKeyframes({
    segment, setSegment, videoRef, currentTime, isVideoReady, renderFrame, activePanel, setActivePanel
  });
  const { editingKeyframeId, setEditingKeyframeId, zoomFactor, setZoomFactor,
    handleAddKeyframe, handleDeleteKeyframe, throttledUpdateZoom } = zoomKeyframes;

  // Text overlays
  const textOverlays = useTextOverlays({ segment, setSegment, currentTime, duration, setActivePanel });
  const { editingTextId, setEditingTextId, handleAddText, handleDeleteText, handleTextDragMove } = textOverlays;

  // Auto zoom
  const { handleAutoZoom } = useAutoZoom({
    segment, setSegment, videoRef, mousePositions, duration,
    currentProjectId: projects.currentProjectId, backgroundConfig,
    loadProjects: projects.loadProjects, setActivePanel
  });

  // Cursor hiding
  const cursorHiding = useCursorHiding({
    segment, setSegment, mousePositions, currentTime, duration
  });
  const { editingPointerId, setEditingPointerId, handleSmartPointerHiding,
    handleAddPointerSegment, handleDeletePointerSegment } = cursorHiding;
  const isOverlayMode = projects.showProjectsDialog || isCropping;

  // Handlers
  const handleToggleCrop = useCallback(() => {
    if (isCropping) {
      setIsCropping(false);
      setActivePanel('zoom');
      setZoomFactor(1.0);
      setEditingKeyframeId(null);
    } else {
      setIsCropping(true);
      if (isPlaying) togglePlayPause();
    }
  }, [isCropping, isPlaying, togglePlayPause, setZoomFactor, setEditingKeyframeId]);

  const handlePreviewMouseDown = useCallback((e: React.MouseEvent) => {
    if (!currentVideo || isCropping || activePanel === 'text') return;
    e.preventDefault();
    e.stopPropagation();
    if (isPlaying) togglePlayPause();

    const startX = e.clientX;
    const startY = e.clientY;
    const lastState = videoRenderer.getLastCalculatedState();
    if (!lastState) return;

    const { positionX: startPosX, positionY: startPosY, zoomFactor: z } = lastState;
    const rect = e.currentTarget.getBoundingClientRect();
    beginBatch();

    const handleMouseMove = (me: MouseEvent) => {
      const dx = me.clientX - startX, dy = me.clientY - startY;
      handleAddKeyframe({
        zoomFactor: z,
        positionX: Math.max(0, Math.min(1, startPosX - (dx / rect.width) / z)),
        positionY: Math.max(0, Math.min(1, startPosY - (dy / rect.height) / z))
      });
      setActivePanel('zoom');
    };

    const handleMouseUp = () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
      commitBatch();
    };
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  }, [currentVideo, isCropping, activePanel, isPlaying, togglePlayPause, handleAddKeyframe, beginBatch, commitBatch]);

  const handleBackgroundUpload = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (event) => {
        const imageUrl = event.target?.result as string;
        setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }));
        setRecentUploads(prev => [imageUrl, ...prev].slice(0, 3));
      };
      reader.readAsDataURL(file);
    }
  }, []);

  const handleToggleProjects = useCallback(() => {
    if (projects.showProjectsDialog) {
      window.dispatchEvent(new CustomEvent('sr-close-projects'));
    } else {
      if (canvasRef.current && currentVideo) {
        try { restoreImageRef.current = canvasRef.current.toDataURL('image/jpeg', 0.8); }
        catch { restoreImageRef.current = null; }
      } else {
        restoreImageRef.current = null;
      }
      projects.setShowProjectsDialog(true);
    }
  }, [projects.showProjectsDialog, currentVideo]);

  const handleStartRecording = useCallback(async () => {
    if (isRecording) return;
    try {
      const monitorList = await getMonitors();
      if (monitorList.length > 1) setShowMonitorSelect(true);
      else await startNewRecording('0');
    } catch (err) { setError(err as string); }
  }, [isRecording, getMonitors, setShowMonitorSelect, startNewRecording, setError]);

  const onStopRecording = useCallback(async () => {
    const result = await handleStopRecording();
    if (result) {
      const { mouseData, initialSegment, videoUrl } = result;
      const response = await fetch(videoUrl);
      const videoBlob = await response.blob();
      const thumbnail = await videoControllerRef.current?.generateThumbnail({
        segment: initialSegment, backgroundConfig, mousePositions: mouseData
      }) || generateThumbnail();
      const project = await projectManager.saveProject({
        name: `Recording ${new Date().toLocaleString()}`,
        videoBlob, segment: initialSegment, backgroundConfig, mousePositions: mouseData, thumbnail,
        duration: initialSegment.trimEnd
      });
      projects.setCurrentProjectId(project.id);
      await projects.loadProjects();
    }
  }, [handleStopRecording, backgroundConfig, generateThumbnail, projects]);

  // Effects
  useEffect(() => {
    const handleToggle = () => {
      if (showHotkeyDialog) return;
      if (isRecording) onStopRecording();
      else handleStartRecording();
    };
    window.addEventListener('toggle-recording', handleToggle);
    return () => window.removeEventListener('toggle-recording', handleToggle);
  }, [isRecording, showHotkeyDialog, onStopRecording, handleStartRecording]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      const isInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target as HTMLElement).isContentEditable;
      if (e.code === 'Space' && !isInput) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (isCropping) return; // Block play/pause during crop mode
        // Blur focused buttons so keyup doesn't re-trigger their click
        if (tag === 'BUTTON' || tag === 'A') (e.target as HTMLElement).blur();
        togglePlayPause();
      }
      if ((e.code === 'Delete' || e.code === 'Backspace') && !isInput) {
        if (editingPointerId) {
          handleDeletePointerSegment();
        } else if (editingTextId && !editingKeyframeId) {
          handleDeleteText();
        } else if (editingKeyframeId !== null && segment?.zoomKeyframes[editingKeyframeId]) {
          setSegment({ ...segment, zoomKeyframes: segment.zoomKeyframes.filter((_, i) => i !== editingKeyframeId) });
          setEditingKeyframeId(null);
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyZ') {
        e.preventDefault();
        e.shiftKey ? (canRedo && redo()) : (canUndo && undo());
      }
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyY') { e.preventDefault(); canRedo && redo(); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [editingKeyframeId, editingTextId, editingPointerId, handleDeleteText, handleDeletePointerSegment, segment, canUndo, canRedo, undo, redo, setSegment, setEditingKeyframeId, togglePlayPause, isCropping]);

  // Wheel zoom
  useEffect(() => {
    const container = previewContainerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (!currentVideo || isCropping) return;
      e.preventDefault();
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;

      if (!wheelBatchActiveRef.current) {
        beginBatch();
        wheelBatchActiveRef.current = true;
      }
      if (wheelBatchTimerRef.current) clearTimeout(wheelBatchTimerRef.current);
      wheelBatchTimerRef.current = setTimeout(() => {
        commitBatch();
        wheelBatchActiveRef.current = false;
        wheelBatchTimerRef.current = null;
      }, 400);

      const newZoom = Math.max(1.0, Math.min(12.0, lastState.zoomFactor - e.deltaY * 0.002 * lastState.zoomFactor));
      handleAddKeyframe({ zoomFactor: newZoom, positionX: lastState.positionX, positionY: lastState.positionY });
      setActivePanel('zoom');
    };

    container.addEventListener('wheel', handleWheel, { passive: false });
    return () => container.removeEventListener('wheel', handleWheel);
  }, [currentVideo, isCropping, handleAddKeyframe, beginBatch, commitBatch]);

  // Initialize segment
  useEffect(() => {
    if (duration > 0 && !segment) {
      const initialSegment: VideoSegment = {
          trimStart: 0,
          trimEnd: duration,
          trimSegments: [{
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: duration,
          }],
          zoomKeyframes: [],
          textSegments: [],
        };
        setSegment(initialSegment);
      setTimeout(() => {
        if (videoRef.current && canvasRef.current && videoRef.current.readyState >= 2) {
          videoRenderer.drawFrame({
            video: videoRef.current, canvas: canvasRef.current, tempCanvas: tempCanvasRef.current,
            segment: initialSegment, backgroundConfig, mousePositions, currentTime: 0
          });
        }
      }, 0);
    }
  }, [duration, segment, backgroundConfig, mousePositions, setSegment, videoRef, canvasRef, tempCanvasRef]);

  // Auto-save
  useEffect(() => {
    if (!projects.currentProjectId || !currentVideo || !segment) return;
    const timer = setTimeout(async () => {
      try {
        const response = await fetch(currentVideo);
        const videoBlob = await response.blob();
        const thumbnail = await videoControllerRef.current?.generateThumbnail({
          segment, backgroundConfig, mousePositions
        }) || generateThumbnail();
        await projectManager.updateProject(projects.currentProjectId!, {
          name: projects.projects.find(p => p.id === projects.currentProjectId)?.name || "Auto Saved",
          videoBlob, segment, backgroundConfig, mousePositions, thumbnail,
          duration: videoControllerRef.current?.duration || duration
        });
        await projects.loadProjects();
      } catch {}
    }, 2000);
    return () => clearTimeout(timer);
  }, [segment, backgroundConfig, mousePositions, projects.currentProjectId, currentVideo, generateThumbnail, projects]);

  // Text drag listeners
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !segment) return;
    const onDown = (e: MouseEvent) => {
      const hitId = videoRenderer.handleMouseDown(e, segment, canvas);
      if (hitId) {
        e.stopPropagation();
        e.preventDefault();
        setEditingTextId(hitId);
        setActivePanel('text');
      }
    };
    const onMove = (e: MouseEvent) => videoRenderer.handleMouseMove(e, segment, canvas, handleTextDragMove);
    const onUp = () => videoRenderer.handleMouseUp();
    canvas.addEventListener('mousedown', onDown);
    canvas.addEventListener('mousemove', onMove);
    canvas.addEventListener('mouseup', onUp);
    canvas.addEventListener('mouseleave', onUp);
    return () => {
      canvas.removeEventListener('mousedown', onDown);
      canvas.removeEventListener('mousemove', onMove);
      canvas.removeEventListener('mouseup', onUp);
      canvas.removeEventListener('mouseleave', onUp);
    };
  }, [segment, handleTextDragMove, canvasRef, setEditingTextId, setActivePanel]);

  return (
    <SettingsContext.Provider value={settings}>
    <div className="app-container min-h-screen bg-[var(--surface)]">
      <ResizeBorders />
      <Header
        isRecording={isRecording} recordingDuration={recordingDuration} currentVideo={currentVideo}
        isProcessing={exportHook.isProcessing} hotkeys={hotkeys} keyvizStatus={keyvizStatus}
        onRemoveHotkey={handleRemoveHotkey} onOpenHotkeyDialog={openHotkeyDialog}
        onToggleKeyviz={toggleKeyviz} onExport={exportHook.handleExport}
        onOpenProjects={handleToggleProjects}
        hideExport={isOverlayMode}
      />

      <main className="app-main flex flex-col px-3 py-3 overflow-hidden" style={{ height: 'calc(100vh - 44px)' }}>
        {error && <p className="error-message text-[var(--tertiary-color)] mb-2 flex-shrink-0">{error}</p>}

        <div className="content-layout flex gap-3 flex-1 min-h-0">
          {/* Video Preview */}
          <div className="video-preview-container flex-1 min-w-0 rounded-xl overflow-hidden bg-[var(--surface-dim)]/80 backdrop-blur-2xl flex items-center justify-center shadow-[0_8px_32px_rgba(0,0,0,0.3)]">
            <div className="preview-inner relative w-full h-full flex justify-center items-center">
              <div
                ref={previewContainerRef}
                className="preview-canvas relative flex items-center justify-center cursor-crosshair group w-full h-full"
                onMouseDown={handlePreviewMouseDown}
              >
                <canvas ref={canvasRef} className="preview-canvas-element absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 max-w-full max-h-full" />
                <canvas ref={tempCanvasRef} className="hidden" />
                <video ref={videoRef} className="hidden" playsInline preload="auto" />
                <audio ref={audioRef} className="hidden" />

                {(!currentVideo || isLoadingVideo) && (
                  <Placeholder isLoadingVideo={isLoadingVideo} loadingProgress={loadingProgress}
                    isRecording={isRecording} recordingDuration={recordingDuration} />
                )}

                {isCropping && currentVideo && segment && (
                  <CropOverlay segment={segment}
                    previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
                    videoRef={videoRef as React.RefObject<HTMLVideoElement>}
                    onUpdateSegment={setSegment}
                    beginBatch={beginBatch} commitBatch={commitBatch} />
                )}

                {!isCropping && currentVideo && backgroundConfig.canvasMode === 'custom' && backgroundConfig.canvasWidth && backgroundConfig.canvasHeight && (
                  <CanvasResizeOverlay
                    previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
                    backgroundConfig={backgroundConfig}
                    setBackgroundConfig={setBackgroundConfig}
                    beginBatch={beginBatch}
                    commitBatch={commitBatch}
                  />
                )}
              </div>

              {currentVideo && !isLoadingVideo && !projects.showProjectsDialog && (
                <PlaybackControls isPlaying={isPlaying} isProcessing={exportHook.isProcessing}
                  isVideoReady={isVideoReady} isCropping={isCropping} currentTime={currentTime}
                  duration={duration} onTogglePlayPause={togglePlayPause} onToggleCrop={handleToggleCrop}
                  autoZoomButton={
                    <Button onClick={handleAutoZoom}
                      disabled={exportHook.isProcessing || !currentVideo || !mousePositions.length}
                      className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                        !currentVideo || exportHook.isProcessing || !mousePositions.length
                          ? 'bg-white/10 text-white/30 cursor-not-allowed'
                          : segment?.smoothMotionPath?.length
                            ? 'bg-[var(--success-color)] hover:bg-[var(--success-color)]/85 text-white'
                            : 'bg-[var(--glass-bg)] hover:bg-white/10 text-[var(--on-surface)]'
                      }`}>
                      <Wand2 className="w-3 h-3 mr-1" />{t.autoZoom}
                    </Button>
                  }
                  smartPointerButton={
                    <Button onClick={handleSmartPointerHiding}
                      disabled={exportHook.isProcessing || !currentVideo || !mousePositions.length}
                      className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                        !currentVideo || exportHook.isProcessing || !mousePositions.length
                          ? 'bg-white/10 text-white/30 cursor-not-allowed'
                          : (() => {
                              const segs = segment?.cursorVisibilitySegments;
                              const isDefault = !segs || segs.length === 0 || (
                                segs.length === 1 &&
                                Math.abs(segs[0].startTime - 0) < 0.01 &&
                                Math.abs(segs[0].endTime - duration) < 0.01
                              );
                              return isDefault
                                ? 'bg-[var(--glass-bg)] hover:bg-white/10 text-[var(--on-surface)]'
                                : 'bg-[var(--success-color)] hover:bg-[var(--success-color)]/85 text-white';
                            })()
                      }`}>
                      <MousePointer2 className="w-3 h-3 mr-1" />{t.smartPointer}
                    </Button>
                  } />
              )}

              {/* Projects view — lives inside the preview area for native FLIP animation */}
              {projects.showProjectsDialog && (
                <ProjectsView
                  projects={projects.projects}
                  onLoadProject={projects.handleLoadProject}
                  onProjectsChange={projects.loadProjects}
                  onClose={() => projects.setShowProjectsDialog(false)}
                  currentProjectId={projects.currentProjectId}
                  restoreImage={restoreImageRef.current}
                />
              )}
            </div>
          </div>

          {/* Side Panel */}
          <div className={`side-panel-container w-72 flex-shrink-0 min-h-0 relative ${isOverlayMode ? 'overflow-hidden' : 'overflow-y-auto thin-scrollbar'}`}>
            <SidePanel
              activePanel={activePanel} setActivePanel={setActivePanel} segment={segment}
              editingKeyframeId={editingKeyframeId} zoomFactor={zoomFactor} setZoomFactor={setZoomFactor}
              onDeleteKeyframe={handleDeleteKeyframe} onUpdateZoom={throttledUpdateZoom}
              backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig}
              recentUploads={recentUploads} onBackgroundUpload={handleBackgroundUpload}
              editingTextId={editingTextId} onUpdateSegment={setSegment}
              beginBatch={beginBatch} commitBatch={commitBatch}
              canvasRef={canvasRef}
            />
            {isOverlayMode && <div className="panel-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />}
          </div>
        </div>

        {/* Timeline */}
        <div className={`timeline-container mt-3 flex-shrink-0 relative ${isOverlayMode ? 'overflow-hidden' : ''}`}>
          <TimelineArea
            duration={duration} currentTime={currentTime} segment={segment} thumbnails={thumbnails}
            timelineRef={timelineRef} videoRef={videoRef} editingKeyframeId={editingKeyframeId}
            editingTextId={editingTextId} setCurrentTime={setCurrentTime}
            setEditingKeyframeId={setEditingKeyframeId} setEditingTextId={setEditingTextId}
            setEditingPointerId={setEditingPointerId}
            setActivePanel={setActivePanel} setSegment={setSegment} onSeek={seek} onSeekEnd={flushSeek}
            onAddText={handleAddText} onAddPointerSegment={handleAddPointerSegment}
            isPlaying={isPlaying} beginBatch={beginBatch} commitBatch={commitBatch}
          />
          {isOverlayMode && <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />}
        </div>
      </main>

      {/* Dialogs */}
      <ProcessingOverlay show={exportHook.isProcessing} exportProgress={0} onCancel={exportHook.cancelExport} />
      <MonitorSelectDialog show={showMonitorSelect} onClose={() => setShowMonitorSelect(false)}
        monitors={monitors} onSelectMonitor={startNewRecording} />
      {currentVideo && !isVideoReady && !projects.showProjectsDialog && (
        <div className="video-loading-overlay absolute inset-0 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="loading-message text-[var(--on-surface)]">{t.preparingVideoOverlay}</div>
        </div>
      )}
      <ExportDialog show={exportHook.showExportDialog} onClose={() => exportHook.setShowExportDialog(false)}
        onExport={exportHook.startExport} exportOptions={exportHook.exportOptions}
        setExportOptions={exportHook.setExportOptions} segment={segment}
        videoRef={videoRef} backgroundConfig={backgroundConfig} />
      <HotkeyDialog show={showHotkeyDialog} onClose={closeHotkeyDialog} />
      <FfmpegSetupDialog show={needsSetup} ffmpegInstallStatus={ffmpegInstallStatus}
        onCancelInstall={handleCancelInstall} />
    </div>
    </SettingsContext.Provider>
  );
}

export default App;
