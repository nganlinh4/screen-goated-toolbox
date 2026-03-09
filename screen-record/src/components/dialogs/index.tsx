import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Keyboard, X } from 'lucide-react';
import { MonitorInfo, Hotkey, WindowInfo } from '@/hooks/useAppHooks';
import { useSettings } from '@/hooks/useSettings';
import { formatMonitorDialogSummary } from '@/utils/helpers';

// Re-export types for backwards compatibility
export type { MonitorInfo, Hotkey, WindowInfo };

// Re-export split dialog modules
export { ExportDialog } from './ExportDialog';
export { WindowSelectDialog } from './WindowSelectDialog';
export { MediaResultDialog, RawVideoDialog, ExportSuccessDialog } from './MediaResultDialog';

// ============================================================================
// ProcessingOverlay
// ============================================================================
interface ProcessingOverlayProps {
  show: boolean;
  exportProgress: number;
  onCancel?: () => void;
}

function formatEta(seconds: number): string {
  if (seconds < 1) return '';
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.ceil(seconds % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function ProcessingOverlay({ show, onCancel }: ProcessingOverlayProps) {
  const { t } = useSettings();
  const [percent, setPercent] = useState(0);
  const [eta, setEta] = useState(0);
  const [active, setActive] = useState(false);
  const [diagnosticsLine, setDiagnosticsLine] = useState('');
  const [phaseLine, setPhaseLine] = useState('');

  useEffect(() => {
    if (!show) {
      setPercent(0);
      setEta(0);
      setActive(false);
      setDiagnosticsLine('');
      setPhaseLine('');
      return;
    }
    // Listen for push progress updates from Rust via PostMessageW -> evaluate_script
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'sr-export-progress') {
        setActive(true);
        setPercent(e.data.percent);
        setEta(e.data.eta);
        const clipIndex = typeof e.data.clipIndex === 'number' ? e.data.clipIndex : null;
        const clipCount = typeof e.data.clipCount === 'number' ? e.data.clipCount : null;
        if (e.data.phase === 'render' && clipIndex && clipCount) {
          setPhaseLine(
            t.exportPhaseRenderClip
              .replace('{index}', String(clipIndex))
              .replace('{count}', String(clipCount))
          );
        } else if (e.data.phase === 'concat') {
          setPhaseLine(t.exportPhaseMergingClips);
        } else if (e.data.phase === 'gif') {
          setPhaseLine(t.exportPhaseCreatingGif);
        } else if (e.data.phase === 'prepare') {
          setPhaseLine(t.preparingExport);
        } else {
          setPhaseLine('');
        }
      } else if (e.data?.type === 'sr-export-diagnostics') {
        const d = e.data.diagnostics || {};
        const mode = d.turbo ? 'Turbo' : 'Standard';
        const codec = typeof d.codec === 'string' ? d.codec : '-';
        const backend = typeof d.backend === 'string' ? d.backend : '-';
        const sfe = d.sfe ? ' + SFE' : '';
        const deviation = typeof d.bitrateDeviationPercent === 'number'
          ? `${Math.abs(d.bitrateDeviationPercent).toFixed(1)}%`
          : '-';
        const fallback = d.fallbackUsed ? ' · fallback' : '';
        setDiagnosticsLine(`${mode} · ${codec}${sfe} · ${backend} · dev ${deviation}${fallback}`);
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, [show, t]);

  if (!show) return null;

  const pct = Math.round(percent);
  const etaStr = formatEta(eta);

  return (
    <div className="processing-overlay-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="processing-dialog bg-[var(--surface-dim)] p-5 rounded-xl border border-[var(--glass-border)] shadow-lg w-72">
        <p className="processing-title text-sm font-medium text-[var(--on-surface)] mb-3">
          {active ? t.exportingVideo : t.preparingExport}
        </p>
        <div className="progress-bar-track h-1.5 w-full bg-[var(--glass-bg-hover)] rounded-full overflow-hidden mb-2">
          <div
            className="progress-bar-fill h-full bg-[var(--primary-color)] rounded-full transition-all duration-300 ease-out"
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="progress-details flex justify-between text-[10px]">
          <span className="progress-percent text-[var(--on-surface-variant)] tabular-nums">{active ? `${pct}%` : ''}</span>
          <span className="progress-eta text-[var(--on-surface-variant)] tabular-nums">{etaStr ? `${etaStr} ${t.timeRemaining}` : ''}</span>
        </div>
        {phaseLine && (
          <div className="processing-phase mt-2 text-[10px] text-[var(--on-surface-variant)]">
            {phaseLine}
          </div>
        )}
        {diagnosticsLine && (
          <div className="processing-diagnostics mt-2 text-[10px] text-[var(--on-surface-variant)] tabular-nums">
            {diagnosticsLine}
          </div>
        )}
        {onCancel && (
          <button
            onClick={() => { console.log('[Cancel] Button clicked'); onCancel(); }}
            className="cancel-export-btn mt-3 w-full py-1.5 rounded-lg text-xs font-medium text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] bg-[var(--glass-bg)] hover:bg-[var(--glass-bg-hover)] border border-[var(--glass-border)] transition-colors"
          >
            {t.cancel}
          </button>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// ConfirmDialog
// ============================================================================
interface ConfirmDialogProps {
  show: boolean;
  title: string;
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ show, title, message, onConfirm, onCancel }: ConfirmDialogProps) {
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="confirm-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[110]">
      <div className="confirm-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-2xl max-w-sm w-full mx-4">
        <h3 className="text-sm font-semibold text-[var(--on-surface)] mb-2">{title}</h3>
        <p className="text-xs text-[var(--on-surface-variant)] mb-5">{message}</p>
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onCancel} className="h-8 text-xs bg-transparent border-[var(--glass-border)] text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)]">
            {t.cancel}
          </Button>
          <Button onClick={onConfirm} className="h-8 text-xs bg-red-600 hover:bg-red-700 text-white">
            {t.clearAll || 'Confirm'}
          </Button>
        </div>
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
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="monitor-select-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="monitor-select-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-md w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.selectMonitor}</h3>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="monitor-list space-y-1.5">
          {monitors.map((monitor) => (
            <button
              key={monitor.id}
              onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
              className="monitor-item w-full p-3 rounded-lg border border-[var(--glass-border)] hover:bg-[var(--glass-bg)] hover:border-[var(--outline)] transition-colors text-left"
            >
              <div className="monitor-name text-sm text-[var(--on-surface)]">{monitor.name}</div>
              <div className="monitor-specs text-xs text-[var(--outline)] mt-0.5">{formatMonitorDialogSummary(monitor)}</div>
            </button>
          ))}
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
  const { t } = useSettings();
  if (!show) return null;

  return (
    <div className="hotkey-dialog-backdrop fixed inset-0 bg-black/70 flex items-center justify-center z-[100]">
      <div className="hotkey-dialog bg-[var(--surface-dim)] p-5 rounded-lg border border-[var(--glass-border)] shadow-lg max-w-sm w-full mx-4">
        <div className="dialog-header flex items-center justify-between mb-4">
          <div className="dialog-header-icon flex items-center gap-2">
            <Keyboard className="w-4 h-4 text-[var(--on-surface-variant)]" />
            <h3 className="dialog-title text-sm font-medium text-[var(--on-surface)]">{t.pressKeys}</h3>
          </div>
          <button onClick={onClose} className="dialog-close-btn p-1 rounded text-[var(--outline)] hover:text-[var(--on-surface)] hover:bg-[var(--glass-bg-hover)] transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="hotkey-hint text-[var(--outline)] text-xs">{t.pressKeysHint}</p>
      </div>
    </div>
  );
}
