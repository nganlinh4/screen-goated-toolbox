import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Keyboard } from 'lucide-react';
import { MonitorInfo, Hotkey, WindowInfo } from '@/hooks/useAppHooks';
import { useSettings } from '@/hooks/useSettings';
import { formatMonitorDialogSummary } from '@/utils/helpers';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
} from '@/components/ui/Dialog';

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

  const pct = Math.round(percent);
  const etaStr = formatEta(eta);

  return (
    <Dialog open={show}>
      <DialogContent hideClose size="max-w-[18rem]" onPointerDownOutside={(e) => e.preventDefault()} onEscapeKeyDown={(e) => e.preventDefault()}>
        <DialogBody>
          <p className="processing-title text-sm font-medium text-[var(--on-surface)] mb-3">
            {active ? t.exportingVideo : t.preparingExport}
          </p>
          <div className="progress-bar-track h-1.5 w-full bg-[var(--ui-hover)] rounded-full overflow-hidden mb-2">
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
            <div className="processing-phase mt-2 text-[10px] text-[var(--on-surface-variant)]">{phaseLine}</div>
          )}
          {diagnosticsLine && (
            <div className="processing-diagnostics mt-2 text-[10px] text-[var(--on-surface-variant)] tabular-nums">{diagnosticsLine}</div>
          )}
          {onCancel && (
            <button
              onClick={onCancel}
              className="cancel-export-btn ui-chip-button mt-3 w-full rounded-lg py-1.5 text-xs font-medium"
            >
              {t.cancel}
            </button>
          )}
        </DialogBody>
      </DialogContent>
    </Dialog>
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

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onCancel(); }}>
      <DialogContent size="max-w-sm">
        <DialogBody>
          <h3 className="text-sm font-semibold text-[var(--on-surface)] mb-2">{title}</h3>
          <p className="text-xs text-[var(--on-surface-variant)] mb-5">{message}</p>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onCancel} className="h-8 rounded-lg text-xs">
              {t.cancel}
            </Button>
            <Button
              onClick={onConfirm}
              className="confirm-dialog-btn ui-action-button h-8 text-xs"
              data-tone="danger"
              data-active="true"
              data-emphasis="strong"
            >
              {t.clearAll || 'Confirm'}
            </Button>
          </div>
        </DialogBody>
      </DialogContent>
    </Dialog>
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

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="max-w-md">
        <DialogHeader>
          <DialogTitle>{t.selectMonitor}</DialogTitle>
        </DialogHeader>
        <DialogBody>
          <div className="monitor-list space-y-1.5">
            {monitors.map((monitor) => (
              <button
                key={monitor.id}
                onClick={() => { onClose(); onSelectMonitor(monitor.id); }}
                className="monitor-item ui-choice-tile w-full rounded-xl p-3 text-left"
              >
                <div className="monitor-name text-sm text-[var(--on-surface)]">{monitor.name}</div>
                <div className="monitor-specs text-xs text-[var(--outline)] mt-0.5">{formatMonitorDialogSummary(monitor)}</div>
              </button>
            ))}
          </div>
        </DialogBody>
      </DialogContent>
    </Dialog>
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

  return (
    <Dialog open={show} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent size="max-w-sm" onInteractOutside={(e) => e.preventDefault()}>
        <DialogHeader>
          <div className="dialog-header-icon flex items-center gap-2">
            <Keyboard className="w-4 h-4 text-[var(--on-surface-variant)]" />
            <DialogTitle>{t.pressKeys}</DialogTitle>
          </div>
        </DialogHeader>
        <DialogBody>
          <p className="hotkey-hint text-[var(--outline)] text-xs">{t.pressKeysHint}</p>
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}
