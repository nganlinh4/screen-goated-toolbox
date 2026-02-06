import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Video, Keyboard, Loader2, AlertCircle, Trash2, Play } from 'lucide-react';
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
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-6 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)]">
        <p className="text-lg text-[var(--on-surface)]">
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
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-6 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)] max-w-md w-full mx-4">
        <h3 className="text-lg font-semibold text-[var(--on-surface)] mb-4">Export Options</h3>

        <div className="space-y-4 mb-6">
          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Quality</label>
            <select
              value={exportOptions.quality}
              onChange={(e) => setExportOptions(prev => ({ ...prev, quality: e.target.value as ExportOptions['quality'] }))}
              className="w-full bg-white/[0.04] border border-white/[0.08] rounded-lg px-3 py-2 text-[var(--on-surface)]"
            >
              {Object.entries(EXPORT_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Dimensions</label>
            <select
              value={exportOptions.dimensions}
              onChange={(e) => setExportOptions(prev => ({ ...prev, dimensions: e.target.value as ExportOptions['dimensions'] }))}
              className="w-full bg-white/[0.04] border border-white/[0.08] rounded-lg px-3 py-2 text-[var(--on-surface)]"
            >
              {Object.entries(DIMENSION_PRESETS).map(([key, preset]) => (
                <option key={key} value={key}>{preset.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="text-xs text-[var(--on-surface-variant)] mb-2 block">Speed</label>
            <div className="bg-white/[0.04] rounded-lg p-3">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-1.5">
                  <span className="text-sm text-[var(--on-surface)] tabular-nums">
                    {formatTime(segment ? (segment.trimEnd - segment.trimStart) / exportOptions.speed : 0)}
                  </span>
                  {segment && exportOptions.speed !== 1 && (
                    <span className={`text-xs ${exportOptions.speed > 1 ? 'text-red-400/90' : 'text-green-400/90'}`}>
                      {exportOptions.speed > 1 ? '↓' : '↑'}
                      {formatTime(Math.abs((segment.trimEnd - segment.trimStart) - ((segment.trimEnd - segment.trimStart) / exportOptions.speed)))}
                    </span>
                  )}
                </div>
                <span className="text-sm font-medium text-[var(--on-surface)] tabular-nums">{Math.round(exportOptions.speed * 100)}%</span>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-xs text-[var(--outline)] min-w-[36px]">Slower</span>
                <input
                  type="range"
                  min="50"
                  max="200"
                  step="10"
                  value={exportOptions.speed * 100}
                  onChange={(e) => setExportOptions(prev => ({ ...prev, speed: Number(e.target.value) / 100 }))}
                  className="flex-1 h-1 rounded"
                />
                <span className="text-xs text-[var(--outline)] min-w-[36px]">Faster</span>
              </div>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-3">
          <Button variant="outline" onClick={onClose} className="bg-transparent border-white/[0.08] text-[var(--on-surface)] hover:bg-white/[0.06] hover:text-[var(--on-surface)] rounded-lg">Cancel</Button>
          <Button onClick={onExport} className="bg-[var(--primary-color)] hover:bg-[var(--primary-color)]/85 text-white rounded-lg">Export Video</Button>
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
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-5 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)] max-w-3xl w-full mx-4">
        <div className="flex justify-between items-center mb-4">
          <h3 className="text-sm font-medium text-[var(--on-surface)]">Projects</h3>
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <span className="text-[10px] text-[var(--outline)]">Max</span>
              <input
                type="range" min="10" max="100" value={projectManager.getLimit()}
                onChange={(e) => { projectManager.setLimit(parseInt(e.target.value)); onProjectsChange(); }}
                className="w-16"
              />
              <span className="text-[10px] text-[var(--on-surface)] tabular-nums w-5">{projectManager.getLimit()}</span>
            </div>
            <button onClick={onClose} className="text-[var(--outline)] hover:text-[var(--on-surface)] transition-colors text-lg leading-none">&times;</button>
          </div>
        </div>

        {projects.length === 0 ? (
          <div className="text-center py-12 text-xs text-[var(--outline)]">No projects yet</div>
        ) : (
          <div className="grid grid-cols-3 gap-3 max-h-[65vh] overflow-y-auto pr-1">
            {projects.map((project) => (
              <div key={project.id} className="group relative bg-white/[0.02] border border-white/[0.06] rounded-lg overflow-hidden hover:border-white/[0.12] transition-all">
                <div
                  className="aspect-video bg-black/40 relative cursor-pointer overflow-hidden"
                  onClick={() => onLoadProject(project.id)}
                >
                  {project.thumbnail ? (
                    <img src={project.thumbnail} className="w-full h-full object-cover" alt="" />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center">
                      <Video className="w-6 h-6 text-white/[0.08]" />
                    </div>
                  )}
                  <div className="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors flex items-center justify-center">
                    <Play className="w-8 h-8 text-white opacity-0 group-hover:opacity-80 transition-opacity drop-shadow-lg" />
                  </div>
                </div>
                <div className="p-2 flex items-start justify-between gap-1">
                  <div className="min-w-0 flex-1">
                    {editingProjectNameId === project.id ? (
                      <input
                        autoFocus
                        className="bg-transparent border-b border-[var(--primary-color)] text-[var(--on-surface)] text-xs w-full outline-none py-0.5"
                        value={projectRenameValue}
                        onChange={(e) => setProjectRenameValue(e.target.value)}
                        onBlur={() => handleRenameProject(project.id)}
                        onKeyDown={(e) => e.key === 'Enter' && handleRenameProject(project.id)}
                      />
                    ) : (
                      <p
                        className="text-xs text-[var(--on-surface)] truncate cursor-pointer hover:text-[var(--primary-color)] transition-colors"
                        onClick={() => { setEditingProjectNameId(project.id); setProjectRenameValue(project.name); }}
                      >
                        {project.name}
                      </p>
                    )}
                    <p className="text-[10px] text-[var(--outline)] mt-0.5">{new Date(project.lastModified).toLocaleDateString()}</p>
                  </div>
                  <button
                    onClick={async () => { await projectManager.deleteProject(project.id); onProjectsChange(); }}
                    className="text-[var(--outline)] hover:text-red-400 transition-colors opacity-0 group-hover:opacity-100 p-0.5 flex-shrink-0"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
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
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-6 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)] max-w-md w-full mx-4">
        <h3 className="text-lg font-semibold text-[var(--on-surface)] mb-4">Select Monitor</h3>
        <div className="space-y-2 mb-6">
          {monitors.map((monitor) => (
            <button
              key={monitor.id}
              onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
              className="w-full p-4 rounded-lg border border-white/[0.06] hover:bg-white/[0.04] transition-colors text-left"
            >
              <div className="font-medium text-[var(--on-surface)]">{monitor.name}</div>
              <div className="text-sm text-[var(--outline)] mt-1">{monitor.width}x{monitor.height} at ({monitor.x}, {monitor.y})</div>
            </button>
          ))}
        </div>
        <div className="flex justify-end">
          <Button onClick={onClose} variant="outline" className="bg-transparent border-white/[0.08] text-[var(--on-surface)] hover:bg-white/[0.06] hover:text-[var(--on-surface)] rounded-lg">Cancel</Button>
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
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-6 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)] max-w-sm w-full mx-4 text-center">
        <Keyboard className="w-10 h-10 text-[var(--primary-color)] mx-auto mb-4" />
        <h3 className="text-lg font-semibold text-[var(--on-surface)] mb-2">Press Keys...</h3>
        <p className="text-[var(--outline)] mb-6 text-sm">Press the combination of keys you want to use.</p>
        <div className="flex justify-center gap-3">
          <Button variant="ghost" onClick={onClose} className="text-[var(--on-surface)] hover:bg-white/[0.06] rounded-lg">Cancel</Button>
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
    <div className="fixed inset-0 bg-black/95 backdrop-blur-sm flex items-center justify-center z-[100]">
      <div className="bg-[var(--surface-dim)]/95 backdrop-blur-xl p-6 rounded-xl border border-white/[0.08] shadow-[0_8px_32px_rgba(0,0,0,0.4)] max-w-md w-full mx-4 relative">
        <div className="text-center">
          <div className="mb-4 inline-flex p-3 rounded-xl bg-[var(--primary-color)]/10 text-[var(--primary-color)]">
            {ffmpegInstallStatus.type === 'Error' ? (
              <AlertCircle className="w-8 h-8 text-red-500" />
            ) : ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting' ? (
              <Loader2 className="w-8 h-8 animate-spin" />
            ) : (
              <Video className="w-8 h-8" />
            )}
          </div>

          <h3 className="text-lg font-semibold text-white mb-2">
            {ffmpegInstallStatus.type === 'Downloading' ? 'Downloading Dependencies' :
              ffmpegInstallStatus.type === 'Extracting' ? 'Setting Up...' :
                ffmpegInstallStatus.type === 'Error' ? 'Installation Failed' :
                  ffmpegInstallStatus.type === 'Cancelled' ? 'Installation Cancelled' : 'Preparing Screen Recorder'}
          </h3>

          <p className="text-[var(--outline)] mb-6 text-sm leading-relaxed">
            {ffmpegInstallStatus.type === 'Downloading' ? 'FFmpeg and ffprobe are required for screen recording. We are downloading them for you.' :
              ffmpegInstallStatus.type === 'Extracting' ? 'Almost ready! Extracting binaries to your system.' :
                ffmpegInstallStatus.type === 'Error' ? ffmpegInstallStatus.message :
                  ffmpegInstallStatus.type === 'Cancelled' ? 'The installation was stopped.' : 'Please wait while we check your system.'}
          </p>

          {(ffmpegInstallStatus.type === 'Downloading' || ffmpegInstallStatus.type === 'Extracting') && (
            <div className="space-y-3 mb-6">
              <div className="h-1.5 w-full bg-white/[0.06] rounded-full overflow-hidden">
                <div
                  className="h-full bg-[var(--primary-color)] transition-all duration-300 ease-out"
                  style={{ width: `${ffmpegInstallStatus.type === 'Downloading' ? ffmpegInstallStatus.progress : 95}%` }}
                />
              </div>
              {ffmpegInstallStatus.type === 'Downloading' && (
                <div className="flex justify-between text-xs font-medium">
                  <span className="text-[var(--primary-color)]">
                    {Math.round(ffmpegInstallStatus.progress)}% downloaded
                    {ffmpegInstallStatus.totalSize > 0 && ` of ${(ffmpegInstallStatus.totalSize / (1024 * 1024)).toFixed(1)} MB`}
                  </span>
                  <span className="text-[var(--outline)]">FFmpeg Essentials</span>
                </div>
              )}
            </div>
          )}

          <div className="flex flex-col gap-3">
            {ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled' ? (
              <Button onClick={() => window.location.reload()} className="w-full bg-[var(--primary-color)] hover:bg-[var(--primary-color)]/85 text-white font-semibold py-5 rounded-lg">
                Try Again
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={onCancelInstall}
                disabled={ffmpegInstallStatus.type === 'Idle' || ffmpegInstallStatus.type === 'Extracting'}
                className="w-full text-[var(--outline)] hover:text-white hover:bg-white/[0.04] py-5 rounded-lg border border-white/[0.08]"
              >
                Cancel Installation
              </Button>
            )}
            {(ffmpegInstallStatus.type === 'Error' || ffmpegInstallStatus.type === 'Cancelled') && (
              <Button variant="ghost" onClick={() => (window as any).ipc.postMessage('close_window')} className="w-full text-red-400 hover:text-red-300 hover:bg-red-500/10 py-5 rounded-lg">
                Close App
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
