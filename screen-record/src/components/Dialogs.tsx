import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, Loader2, AlertCircle, Trash2 } from 'lucide-react';
import { ExportOptions, Project, VideoSegment } from '@/types/video';
import { EXPORT_PRESETS, DIMENSION_PRESETS } from '@/lib/videoExporter';
import { projectManager } from '@/lib/projectManager';
import { formatTime } from '@/utils/helpers';
import { MonitorInfo, Hotkey, FfmpegInstallStatus } from '@/hooks/useAppHooks';

// Re-export types for backwards compatibility
export type { MonitorInfo, Hotkey, FfmpegInstallStatus };

// ============================================================================
// ProcessingOverlay
// ============================================================================
interface ProcessingOverlayProps {
  show: boolean;
  exportProgress: number;
}

export function ProcessingOverlay({ show, exportProgress }: ProcessingOverlayProps) {
  if (!show) return null;
  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#1a1a1b] p-6 rounded-lg border border-[#343536]">
        <p className="text-lg text-[#d7dadc]">
          {exportProgress > 0 ? `Exporting video... ${Math.round(exportProgress)}%` : 'Processing video...'}
        </p>
      </div>
    </div>
  );
}

// ============================================================================
// ExportDialog
// ============================================================================
interface ExportDialogProps {
  show: boolean;
  onClose: () => void;
  onExport: () => void;
  exportOptions: ExportOptions;
  setExportOptions: React.Dispatch<React.SetStateAction<ExportOptions>>;
  segment: VideoSegment | null;
}

export function ExportDialog({ show, onClose, onExport, exportOptions, setExportOptions, segment }: ExportDialogProps) {
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#1a1a1b] p-6 rounded-lg border border-[#343536] max-w-md w-full mx-4">
        <h3 className="text-lg font-semibold text-[#d7dadc] mb-4">Export Options</h3>

        <div className="space-y-4 mb-6">
          <div>
            <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Quality</label>
            <select
              value={exportOptions.quality}
              onChange={(e) => setExportOptions(prev => ({ ...prev, quality: e.target.value as ExportOptions['quality'] }))}
              className="w-full bg-[#272729] border border-[#343536] rounded-md px-3 py-2 text-[#d7dadc]"
            >
              {Object.entries(EXPORT_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Dimensions</label>
            <select
              value={exportOptions.dimensions}
              onChange={(e) => setExportOptions(prev => ({ ...prev, dimensions: e.target.value as ExportOptions['dimensions'] }))}
              className="w-full bg-[#272729] border border-[#343536] rounded-md px-3 py-2 text-[#d7dadc]"
            >
              {Object.entries(DIMENSION_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-sm font-medium text-[#d7dadc] mb-2 block">Speed</label>
            <div className="bg-[#272729] rounded-md p-3">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-1.5">
                  <span className="text-sm text-[#d7dadc] tabular-nums">
                    {formatTime(segment ? (segment.trimEnd - segment.trimStart) / exportOptions.speed : 0)}
                  </span>
                  {segment && exportOptions.speed !== 1 && (
                    <span className={`text-xs ${exportOptions.speed > 1 ? 'text-red-400/90' : 'text-green-400/90'}`}>
                      {exportOptions.speed > 1 ? '↓' : '↑'}
                      {formatTime(Math.abs((segment.trimEnd - segment.trimStart) - ((segment.trimEnd - segment.trimStart) / exportOptions.speed)))}
                    </span>
                  )}
                </div>
                <span className="text-sm font-medium text-[#d7dadc] tabular-nums">{Math.round(exportOptions.speed * 100)}%</span>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-xs text-[#818384] min-w-[36px]">Slower</span>
                <input
                  type="range"
                  min="50"
                  max="200"
                  step="10"
                  value={exportOptions.speed * 100}
                  onChange={(e) => setExportOptions(prev => ({ ...prev, speed: Number(e.target.value) / 100 }))}
                  className="flex-1 h-1 accent-[#0079d3] rounded-full"
                />
                <span className="text-xs text-[#818384] min-w-[36px]">Faster</span>
              </div>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-3">
          <Button variant="outline" onClick={onClose} className="bg-transparent border-[#343536] text-[#d7dadc] hover:bg-[#272729] hover:text-[#d7dadc]">Cancel</Button>
          <Button onClick={onExport} className="bg-[#0079d3] hover:bg-[#0079d3]/90 text-white">Export Video</Button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// ProjectsDialog
// ============================================================================
interface ProjectsDialogProps {
  show: boolean;
  onClose: () => void;
  projects: Omit<Project, 'videoBlob'>[];
  onLoadProject: (projectId: string) => void;
  onProjectsChange: () => void;
}

export function ProjectsDialog({ show, onClose, projects, onLoadProject, onProjectsChange }: ProjectsDialogProps) {
  const [editingProjectNameId, setEditingProjectNameId] = useState<string | null>(null);
  const [projectRenameValue, setProjectRenameValue] = useState("");

  if (!show) return null;

  const handleRenameProject = async (id: string) => {
    if (!projectRenameValue.trim()) return;
    const fullProject = await projectManager.loadProject(id);
    if (fullProject) {
      await projectManager.updateProject(id, { ...fullProject, name: projectRenameValue.trim() });
      onProjectsChange();
    }
    setEditingProjectNameId(null);
  };

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#1a1a1b] p-6 rounded-lg border border-[#343536] max-w-2xl w-full mx-4">
        <div className="flex justify-between items-center mb-6">
          <div className="flex items-center gap-4">
            <h3 className="text-lg font-semibold text-[#d7dadc]">Recent Projects</h3>
            <div className="flex items-center gap-2 ml-4">
              <span className="text-xs text-[#818384]">Limit:</span>
              <input
                type="range" min="10" max="100" value={projectManager.getLimit()}
                onChange={(e) => { projectManager.setLimit(parseInt(e.target.value)); onProjectsChange(); }}
                className="w-24 h-1 bg-[#272729] rounded-lg appearance-none cursor-pointer accent-[#0079d3]"
              />
              <span className="text-xs text-[#d7dadc]">{projectManager.getLimit()}</span>
            </div>
          </div>
          <Button variant="ghost" onClick={onClose} className="text-[#818384] hover:text-[#d7dadc]">✕</Button>
        </div>

        {projects.length === 0 ? (
          <div className="text-center py-8 text-[#818384]">No saved projects yet</div>
        ) : (
          <div className="space-y-2 max-h-[60vh] overflow-y-auto">
            {projects.map((project) => (
              <div key={project.id} className="flex items-center justify-between p-3 rounded-lg border border-[#343536] hover:bg-[#272729] transition-colors gap-4">
                <div className="w-24 h-14 bg-black rounded overflow-hidden flex-shrink-0 border border-[#343536]">
                  {project.thumbnail ? (
                    <img src={project.thumbnail} className="w-full h-full object-cover" alt="Preview" />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center text-[#343536]"><Video className="w-6 h-6" /></div>
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  {editingProjectNameId === project.id ? (
                    <input
                      autoFocus
                      className="bg-[#1a1a1b] border border-[#0079d3] rounded px-2 py-1 text-[#d7dadc] w-full"
                      value={projectRenameValue}
                      onChange={(e) => setProjectRenameValue(e.target.value)}
                      onBlur={() => handleRenameProject(project.id)}
                      onKeyDown={(e) => e.key === 'Enter' && handleRenameProject(project.id)}
                    />
                  ) : (
                    <h4
                      className="text-[#d7dadc] font-medium truncate hover:text-[#0079d3] cursor-pointer"
                      onClick={() => { setEditingProjectNameId(project.id); setProjectRenameValue(project.name); }}
                    >
                      {project.name}
                    </h4>
                  )}
                  <p className="text-sm text-[#818384]">Last modified: {new Date(project.lastModified).toLocaleDateString()}</p>
                </div>
                <div className="flex gap-2">
                  <Button onClick={() => onLoadProject(project.id)} className="bg-[#0079d3] hover:bg-[#0079d3]/90 text-white">Load Project</Button>
                  <Button variant="ghost" onClick={async () => { await projectManager.deleteProject(project.id); onProjectsChange(); }} className="text-red-400 hover:text-red-300 hover:bg-red-900/20">
                    <Trash2 className="w-4 h-4" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// MonitorSelectDialog
// ============================================================================
interface MonitorSelectDialogProps {
  show: boolean;
  onClose: () => void;
  monitors: MonitorInfo[];
  onSelectMonitor: (monitorId: string) => void;
}

export function MonitorSelectDialog({ show, onClose, monitors, onSelectMonitor }: MonitorSelectDialogProps) {
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#1a1a1b] p-6 rounded-lg border border-[#343536] max-w-md w-full mx-4">
        <h3 className="text-lg font-semibold text-[#d7dadc] mb-4">Select Monitor</h3>
        <div className="space-y-3 mb-6">
          {monitors.map((monitor) => (
            <button
              key={monitor.id}
              onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
              className="w-full p-4 rounded-lg border border-[#343536] hover:bg-[#272729] transition-colors text-left"
            >
              <div className="font-medium text-[#d7dadc]">{monitor.name}</div>
              <div className="text-sm text-[#818384] mt-1">{monitor.width}x{monitor.height} at ({monitor.x}, {monitor.y})</div>
            </button>
          ))}
        </div>
        <div className="flex justify-end">
          <Button onClick={onClose} variant="outline" className="bg-transparent border-[#343536] text-[#d7dadc] hover:bg-[#272729] hover:text-[#d7dadc]">Cancel</Button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// HotkeyDialog
// ============================================================================
interface HotkeyDialogProps {
  show: boolean;
  onClose: () => void;
}

export function HotkeyDialog({ show, onClose }: HotkeyDialogProps) {
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50">
      <div className="bg-[#1a1a1b] p-6 rounded-lg border border-[#343536] max-w-sm w-full mx-4 text-center">
        <Keyboard className="w-12 h-12 text-[#0079d3] mx-auto mb-4" />
        <h3 className="text-lg font-semibold text-[#d7dadc] mb-2">Press Keys...</h3>
        <p className="text-[#818384] mb-6">Press the combination of keys you want to use.</p>
        <div className="flex justify-center gap-3">
          <Button variant="ghost" onClick={onClose} className="text-[#d7dadc] hover:bg-[#272729]">Cancel</Button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// FfmpegSetupDialog
// ============================================================================
interface FfmpegSetupDialogProps {
  show: boolean;
  ffmpegInstallStatus: FfmpegInstallStatus;
  onCancelInstall: () => void;
}

export function FfmpegSetupDialog({ show, ffmpegInstallStatus, onCancelInstall }: FfmpegSetupDialogProps) {
  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/95 flex items-center justify-center z-[100] backdrop-blur-md">
      <div className="bg-[#1a1a1b] p-8 rounded-2xl border border-[#343536] max-w-md w-full mx-4 shadow-2xl relative overflow-hidden">
        <div className="absolute -top-24 -left-24 w-48 h-48 bg-[#0079d3]/10 rounded-full blur-3xl opacity-50" />
        <div className="absolute -bottom-24 -right-24 w-48 h-48 bg-[#9C17FF]/10 rounded-full blur-3xl opacity-50" />

        <div className="relative z-10 text-center">
          <div className="mb-6 inline-flex p-4 rounded-full bg-[#0079d3]/10 text-[#0079d3]">
            {ffmpegInstallStatus.type === 'Error' ? (
              <AlertCircle className="w-10 h-10 text-red-500" />
            ) : ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting' ? (
              <Loader2 className="w-10 h-10 animate-spin" />
            ) : (
              <Video className="w-10 h-10" />
            )}
          </div>

          <h3 className="text-2xl font-bold text-white mb-2">
            {ffmpegInstallStatus.type === 'Downloading' ? 'Downloading Dependencies' :
              ffmpegInstallStatus.type === 'Extracting' ? 'Setting Up...' :
                ffmpegInstallStatus.type === 'Error' ? 'Installation Failed' :
                  ffmpegInstallStatus.type === 'Cancelled' ? 'Installation Cancelled' : 'Preparing Screen Recorder'}
          </h3>

          <p className="text-[#818384] mb-8 text-sm leading-relaxed">
            {ffmpegInstallStatus.type === 'Downloading' ? 'FFmpeg and ffprobe are required for screen recording. We are downloading them for you.' :
              ffmpegInstallStatus.type === 'Extracting' ? 'Almost ready! Extracting binaries to your system.' :
                ffmpegInstallStatus.type === 'Error' ? ffmpegInstallStatus.message :
                  ffmpegInstallStatus.type === 'Cancelled' ? 'The installation was stopped.' : 'Please wait while we check your system.'}
          </p>

          {(ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting') && (
            <div className="space-y-4 mb-8">
              <div className="h-2 w-full bg-[#272729] rounded-full overflow-hidden">
                <div
                  className="h-full bg-gradient-to-r from-[#0079d3] to-[#9C17FF] transition-all duration-300 ease-out"
                  style={{ width: `${ffmpegInstallStatus.type === 'Downloading' ? ffmpegInstallStatus.progress : 95}%` }}
                />
              </div>
              {ffmpegInstallStatus.type === 'Downloading' && (
                <div className="flex justify-between text-xs font-medium">
                  <span className="text-[#0079d3]">
                    {Math.round(ffmpegInstallStatus.progress)}% downloaded
                    {ffmpegInstallStatus.totalSize > 0 && ` of ${(ffmpegInstallStatus.totalSize / (1024 * 1024)).toFixed(1)} MB`}
                  </span>
                  <span className="text-[#818384]">FFmpeg Essentials</span>
                </div>
              )}
            </div>
          )}

          <div className="flex flex-col gap-3">
            {ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled' ? (
              <Button onClick={() => window.location.reload()} className="w-full bg-[#0079d3] hover:bg-[#0079d3]/90 text-white font-semibold py-6 rounded-xl">
                Try Again
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={onCancelInstall}
                disabled={ffmpegInstallStatus.type === 'Idle' || ffmpegInstallStatus.type === 'Extracting'}
                className="w-full text-[#818384] hover:text-white hover:bg-white/5 py-6 rounded-xl border border-[#343536]"
              >
                Cancel Installation
              </Button>
            )}
            {(ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled') && (
              <Button variant="ghost" onClick={() => (window as any).ipc.postMessage('close_window')} className="w-full text-red-400 hover:text-red-300 hover:bg-red-500/10 py-6 rounded-xl">
                Close App
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
