import { useState, useEffect, useCallback, useRef } from "react";
import { Wand2 } from "lucide-react";
import "./App.css";
import { Button } from "@/components/ui/button";
import { videoRenderer } from '@/lib/videoRenderer';
import { BackgroundConfig, VideoSegment } from '@/types/video';
import { projectManager } from '@/lib/projectManager';
import { Timeline } from '@/components/Timeline';
import { useUndoRedo } from '@/hooks/useUndoRedo';
import { useFfmpegSetup, useHotkeys, useKeyviz, useMonitors } from '@/hooks/useAppHooks';
import {
  useVideoPlayback, useRecording, useProjects, useExport,
  useZoomKeyframes, useTextOverlays, useAutoZoom
} from '@/hooks/useVideoState';

import { Header } from '@/components/Header';
import { Placeholder, CropOverlay, PlaybackControls } from '@/components/VideoPreview';
import { SidePanel, ActivePanel } from '@/components/SidePanel';
import {
  ProcessingOverlay, ExportDialog, ProjectsDialog,
  MonitorSelectDialog, HotkeyDialog, FfmpegSetupDialog
} from '@/components/Dialogs';

function App() {
  // Core state
  const { state: segment, setState: setSegment, undo, redo, canUndo, canRedo } = useUndoRedo<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>('zoom');
  const [isCropping, setIsCropping] = useState(false);
  const [recentUploads, setRecentUploads] = useState<string[]>([]);
  const [backgroundConfig, setBackgroundConfig] = useState<BackgroundConfig>({
    scale: 90, borderRadius: 48, backgroundType: 'gradient2', shadow: 100, volume: 1, cursorScale: 5
  });

  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);

  // Utility hooks
  const { needsSetup, ffmpegInstallStatus, handleCancelInstall } = useFfmpegSetup();
  const { hotkeys, showHotkeyDialog, handleRemoveHotkey, openHotkeyDialog, closeHotkeyDialog } = useHotkeys();
  const { keyvizStatus, toggleKeyviz } = useKeyviz();
  const { monitors, showMonitorSelect, setShowMonitorSelect, getMonitors } = useMonitors();

  // Video playback
  const playback = useVideoPlayback({ segment, backgroundConfig, mousePositions: [], isCropping });
  const {
    currentTime, setCurrentTime, duration, isPlaying, isVideoReady, setIsVideoReady,
    thumbnails, setThumbnails, currentVideo, setCurrentVideo, currentAudio, setCurrentAudio,
    videoRef, audioRef, canvasRef, tempCanvasRef, videoControllerRef,
    renderFrame, togglePlayPause, seek, generateThumbnail, generateThumbnails
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
  const { editingTextId, setEditingTextId, handleAddText, handleTextDragMove } = textOverlays;

  // Auto zoom
  const { handleAutoZoom } = useAutoZoom({
    segment, setSegment, videoRef, mousePositions, duration,
    currentProjectId: projects.currentProjectId, backgroundConfig,
    loadProjects: projects.loadProjects, setActivePanel
  });

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
    };
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  }, [currentVideo, isCropping, activePanel, isPlaying, togglePlayPause, handleAddKeyframe]);

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
      const { mouseData, initialSegment } = result;
      // Auto-save project
      if (currentVideo) {
        const response = await fetch(currentVideo);
        const videoBlob = await response.blob();
        const thumbnail = generateThumbnail();
        const project = await projectManager.saveProject({
          name: `Recording ${new Date().toLocaleString()}`,
          videoBlob, segment: initialSegment, backgroundConfig, mousePositions: mouseData, thumbnail
        });
        projects.setCurrentProjectId(project.id);
        await projects.loadProjects();
      }
    }
  }, [handleStopRecording, currentVideo, backgroundConfig, generateThumbnail, projects]);

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
      const isInput = ['INPUT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName);
      if (e.code === 'Space' && !isInput) { e.preventDefault(); togglePlayPause(); }
      if ((e.code === 'Delete' || e.code === 'Backspace') && editingKeyframeId !== null && !isInput) {
        if (segment?.zoomKeyframes[editingKeyframeId]) {
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
  }, [editingKeyframeId, segment, canUndo, canRedo, undo, redo, setSegment, setEditingKeyframeId, togglePlayPause]);

  // Wheel zoom
  useEffect(() => {
    const container = previewContainerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (!currentVideo || isCropping) return;
      e.preventDefault();
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;
      const newZoom = Math.max(1.0, Math.min(12.0, lastState.zoomFactor - e.deltaY * 0.002 * lastState.zoomFactor));
      handleAddKeyframe({ zoomFactor: newZoom, positionX: lastState.positionX, positionY: lastState.positionY });
      setActivePanel('zoom');
    };

    container.addEventListener('wheel', handleWheel, { passive: false });
    return () => container.removeEventListener('wheel', handleWheel);
  }, [currentVideo, isCropping, handleAddKeyframe]);

  // Initialize segment
  useEffect(() => {
    if (duration > 0 && !segment) {
      const initialSegment: VideoSegment = { trimStart: 0, trimEnd: duration, zoomKeyframes: [], textSegments: [] };
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
        await projectManager.updateProject(projects.currentProjectId!, {
          name: projects.projects.find(p => p.id === projects.currentProjectId)?.name || "Auto Saved",
          videoBlob, segment, backgroundConfig, mousePositions, thumbnail: generateThumbnail()
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
    const onDown = (e: MouseEvent) => videoRenderer.handleMouseDown(e, segment, canvas);
    const onMove = (e: MouseEvent) => videoRenderer.handleMouseMove(e, segment, canvas, handleTextDragMove);
    const onUp = () => videoRenderer.handleMouseUp(canvas);
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
  }, [segment, handleTextDragMove, canvasRef]);

  return (
    <div className="min-h-screen bg-[#1a1a1b]">
      <Header
        isRecording={isRecording} recordingDuration={recordingDuration} currentVideo={currentVideo}
        isProcessing={exportHook.isProcessing} hotkeys={hotkeys} keyvizStatus={keyvizStatus}
        onRemoveHotkey={handleRemoveHotkey} onOpenHotkeyDialog={openHotkeyDialog}
        onToggleKeyviz={toggleKeyviz} onExport={exportHook.handleExport}
        onOpenProjects={() => projects.setShowProjectsDialog(true)}
      />

      <main className="max-w-6xl mx-auto px-4 py-6">
        {error && <p className="text-red-500 mb-4">{error}</p>}

        <div className="space-y-6">
          <div className="grid grid-cols-4 gap-6 items-start">
            {/* Video Preview */}
            <div className="col-span-3 rounded-lg overflow-hidden bg-black/20 flex items-center justify-center">
              <div className="relative w-full flex justify-center max-h-[70vh]">
                <div
                  ref={previewContainerRef}
                  className={`relative flex items-center justify-center cursor-crosshair group ${!currentVideo ? 'w-full aspect-video' : ''}`}
                  onMouseDown={handlePreviewMouseDown}
                >
                  <canvas ref={canvasRef} className="max-w-full max-h-[70vh] object-contain" />
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
                      onUpdateSegment={setSegment} />
                  )}
                </div>

                {currentVideo && !isLoadingVideo && (
                  <PlaybackControls isPlaying={isPlaying} isProcessing={exportHook.isProcessing}
                    isVideoReady={isVideoReady} isCropping={isCropping} currentTime={currentTime}
                    duration={duration} onTogglePlayPause={togglePlayPause} onToggleCrop={handleToggleCrop} />
                )}
              </div>
            </div>

            {/* Side Panel */}
            <SidePanel
              activePanel={activePanel} setActivePanel={setActivePanel} segment={segment}
              editingKeyframeId={editingKeyframeId} zoomFactor={zoomFactor} setZoomFactor={setZoomFactor}
              onDeleteKeyframe={handleDeleteKeyframe} onUpdateZoom={throttledUpdateZoom}
              backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig}
              recentUploads={recentUploads} onBackgroundUpload={handleBackgroundUpload}
              editingTextId={editingTextId} onAddText={handleAddText} onUpdateSegment={setSegment}
            />
          </div>

          {/* Timeline */}
          <div className="bg-[#1a1a1b] rounded-lg border border-[#343536] p-6">
            <div className="space-y-2 mb-8">
              <div className="flex justify-between items-center">
                <h2 className="text-lg font-semibold text-[#d7dadc]">Timeline</h2>
                <Button onClick={handleAutoZoom}
                  disabled={exportHook.isProcessing || !currentVideo || !mousePositions.length}
                  className={`flex items-center px-4 py-2 h-9 text-sm font-medium ${
                    !currentVideo || exportHook.isProcessing || !mousePositions.length
                      ? 'bg-gray-600/50 text-gray-400 cursor-not-allowed'
                      : 'bg-green-600 hover:bg-green-700 text-white'
                  }`}>
                  <Wand2 className="w-4 h-4 mr-2" />Auto-Smart Zoom
                </Button>
              </div>
            </div>

            <Timeline duration={duration} currentTime={currentTime} segment={segment} thumbnails={thumbnails}
              timelineRef={timelineRef} videoRef={videoRef} editingKeyframeId={editingKeyframeId}
              editingTextId={editingTextId} setCurrentTime={setCurrentTime}
              setEditingKeyframeId={setEditingKeyframeId} setEditingTextId={setEditingTextId}
              setActivePanel={setActivePanel} setSegment={setSegment} onSeek={seek} />
          </div>
        </div>
      </main>

      {/* Dialogs */}
      <ProcessingOverlay show={exportHook.isProcessing} exportProgress={exportHook.exportProgress} />
      <MonitorSelectDialog show={showMonitorSelect} onClose={() => setShowMonitorSelect(false)}
        monitors={monitors} onSelectMonitor={startNewRecording} />
      {currentVideo && !isVideoReady && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/50">
          <div className="text-white">Preparing video...</div>
        </div>
      )}
      <ExportDialog show={exportHook.showExportDialog} onClose={() => exportHook.setShowExportDialog(false)}
        onExport={exportHook.startExport} exportOptions={exportHook.exportOptions}
        setExportOptions={exportHook.setExportOptions} segment={segment} />
      <ProjectsDialog show={projects.showProjectsDialog} onClose={() => projects.setShowProjectsDialog(false)}
        projects={projects.projects} onLoadProject={projects.handleLoadProject}
        onProjectsChange={projects.loadProjects} />
      <HotkeyDialog show={showHotkeyDialog} onClose={closeHotkeyDialog} />
      <FfmpegSetupDialog show={needsSetup} ffmpegInstallStatus={ffmpegInstallStatus}
        onCancelInstall={handleCancelInstall} />
    </div>
  );
}

export default App;
