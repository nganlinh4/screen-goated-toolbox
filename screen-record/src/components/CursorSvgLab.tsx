import { useEffect, useMemo, useRef, useState } from 'react';

type CursorAdjustment = {
  scale: number;
  offsetX: number;
  offsetY: number;
};

type CursorItem = {
  key: string;
  label: string;
  src: string;
};

type DragState = {
  key: string;
  startX: number;
  startY: number;
  startOffsetX: number;
  startOffsetY: number;
};

const STAGE_W = 220;
const STAGE_H = 170;
const CANVAS_W = 44;
const CANVAS_H = 43;
const LAB_DISPLAY_SCALE = 2.2;

const CURSOR_TYPES: Array<{ id: string; label: string }> = [
  { id: 'default', label: 'Default Arrow' },
  { id: 'text', label: 'Text Beam' },
  { id: 'pointer', label: 'Pointing Hand' },
  { id: 'openhand', label: 'Open Hand' },
  { id: 'closehand', label: 'Closed Hand' },
  { id: 'wait', label: 'Wait' },
  { id: 'appstarting', label: 'App Starting' },
  { id: 'crosshair', label: 'Crosshair' },
  { id: 'resize-ns', label: 'Resize N-S' },
  { id: 'resize-we', label: 'Resize W-E' },
  { id: 'resize-nwse', label: 'Resize NW-SE' },
  { id: 'resize-nesw', label: 'Resize NE-SW' },
];

const SCREENSTUDIO_ITEMS: CursorItem[] = CURSOR_TYPES.map((t) => ({
  key: `screenstudio-${t.id}`,
  label: `ScreenStudio • ${t.label}`,
  src: `/cursor-${t.id}-screenstudio.svg`,
}));

const MACOS26_ITEMS: CursorItem[] = CURSOR_TYPES.map((t) => ({
  key: `macos26-${t.id}`,
  label: `macOS 26 • ${t.label}`,
  src: `/cursor-${t.id}-macos26.svg`,
}));

const SGTCUTE_ITEMS: CursorItem[] = CURSOR_TYPES.map((t) => ({
  key: `sgtcute-${t.id}`,
  label: `SGT Cute • ${t.label}`,
  src: `/cursor-${t.id}-sgtcute.svg`,
}));

const SGTCOOL_ITEMS: CursorItem[] = CURSOR_TYPES.map((t) => ({
  key: `sgtcool-${t.id}`,
  label: `SGT Cool • ${t.label}`,
  src: `/cursor-${t.id}-sgtcool.svg`,
}));

const CURSOR_ITEMS: CursorItem[] = [
  ...SCREENSTUDIO_ITEMS,
  ...MACOS26_ITEMS,
  ...SGTCUTE_ITEMS,
  ...SGTCOOL_ITEMS,
];

function makeDefaultAdjustments(): Record<string, CursorAdjustment> {
  const out: Record<string, CursorAdjustment> = {};
  for (const item of CURSOR_ITEMS) {
    out[item.key] = { scale: 1, offsetX: 0, offsetY: 0 };
  }
  return out;
}

export default function CursorSvgLab() {
  const [adjust, setAdjust] = useState<Record<string, CursorAdjustment>>(makeDefaultAdjustments);
  const [baselineAdjust, setBaselineAdjust] = useState<Record<string, CursorAdjustment>>(makeDefaultAdjustments);
  const [focusedKey, setFocusedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState<'idle' | 'ok' | 'fail'>('idle');
  const [applying, setApplying] = useState<Record<string, boolean>>({});
  const [applyStatus, setApplyStatus] = useState<Record<string, 'idle' | 'ok' | 'fail'>>({});
  const [assetVersion, setAssetVersion] = useState(1);
  const dragRef = useRef<DragState | null>(null);

  const payload = useMemo(() => {
    const out: Record<string, CursorAdjustment & { hotspotX: number; hotspotY: number; src: string }> = {};
    for (const item of CURSOR_ITEMS) {
      const a = adjust[item.key];
      out[item.src.replace(/^\//, '')] = {
        scale: Number(a.scale.toFixed(4)),
        offsetX: Number(a.offsetX.toFixed(2)),
        offsetY: Number(a.offsetY.toFixed(2)),
        hotspotX: 0.5,
        hotspotY: 0.5,
        src: item.src,
      };
    }
    return out;
  }, [adjust]);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      const loaded: Record<string, CursorAdjustment> = {};
      await Promise.all(
        CURSOR_ITEMS.map(async (item) => {
          try {
            const res = await fetch(`${item.src}?v=cursor-lab-load-v10`);
            if (!res.ok) return;
            const text = await res.text();
            const parsed = parseCursorGeometry(text);
            if (!parsed) return;
            loaded[item.key] = {
              scale: Number.isFinite(parsed.scale) ? parsed.scale : 1,
              offsetX: Number.isFinite(parsed.offsetX) ? parsed.offsetX : 0,
              offsetY: Number.isFinite(parsed.offsetY) ? parsed.offsetY : 0,
            };
          } catch {
            // Keep default values on parse/load failures.
          }
        })
      );
      if (cancelled) return;
      if (Object.keys(loaded).length === 0) return;
      setAdjust((prev) => ({ ...prev, ...loaded }));
      setBaselineAdjust((prev) => ({ ...prev, ...loaded }));
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const copyJson = async () => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      setCopied('ok');
    } catch {
      setCopied('fail');
    }
    window.setTimeout(() => setCopied('idle'), 1200);
  };

  const resetOne = (key: string) => {
    setAdjust((prev) => ({ ...prev, [key]: { ...baselineAdjust[key] } }));
  };

  const applyOne = async (item: CursorItem) => {
    const a = adjust[item.key];
    const invoke = (window as unknown as { __TAURI__?: { core?: { invoke?: (cmd: string, args?: unknown) => Promise<unknown> } } })
      .__TAURI__?.core?.invoke;
    if (!invoke) {
      setApplyStatus((prev) => ({ ...prev, [item.key]: 'fail' }));
      return;
    }
    setApplying((prev) => ({ ...prev, [item.key]: true }));
    try {
      await invoke('apply_cursor_svg_adjustment', {
        src: item.src,
        scale: a.scale,
        offsetX: a.offsetX,
        offsetY: a.offsetY,
      });
      setApplyStatus((prev) => ({ ...prev, [item.key]: 'ok' }));
      setAssetVersion((v) => v + 1);
      window.setTimeout(() => {
        setApplyStatus((prev) => ({ ...prev, [item.key]: 'idle' }));
      }, 1000);
    } catch {
      setApplyStatus((prev) => ({ ...prev, [item.key]: 'fail' }));
      window.setTimeout(() => {
        setApplyStatus((prev) => ({ ...prev, [item.key]: 'idle' }));
      }, 1200);
    } finally {
      setApplying((prev) => ({ ...prev, [item.key]: false }));
    }
  };

  return (
    <div className="cursor-lab-page h-screen overflow-hidden bg-[var(--surface-dim)] text-[var(--on-surface)] p-4">
      <div className="cursor-lab-toolbar sticky top-0 z-20 bg-[var(--surface-dim)] py-2 mb-3 border-b border-[var(--glass-border)]">
        <div className="cursor-lab-toolbar-row flex items-center gap-2 flex-wrap">
          <a href="#" className="cursor-lab-back-link text-xs px-2 py-1 rounded border border-[var(--glass-border)] hover:bg-[var(--glass-bg)]">Back</a>
          <button
            onClick={copyJson}
            className="cursor-lab-copy-button text-xs px-2 py-1 rounded border border-[var(--primary-color)] text-[var(--primary-color)]"
          >
            Copy JSON
          </button>
          <span className="cursor-lab-copy-status text-xs text-[var(--on-surface-variant)]">
            {copied === 'ok' ? 'Copied' : copied === 'fail' ? 'Copy failed' : ''}
          </span>
          <span className="cursor-lab-help-text text-xs text-[var(--on-surface-variant)]">
            Drag: move cursor content (real SVG px), Slider: scale, Hotspot: fixed center
          </span>
        </div>
      </div>

      <div className="cursor-lab-grid h-[calc(100vh-108px)] overflow-auto thin-scrollbar grid grid-cols-[repeat(auto-fill,minmax(255px,1fr))] gap-3 pr-1">
        {CURSOR_ITEMS.map((item) => {
          const a = adjust[item.key];
          const b = baselineAdjust[item.key];
          const dispCanvasW = CANVAS_W * LAB_DISPLAY_SCALE;
          const dispCanvasH = CANVAS_H * LAB_DISPLAY_SCALE;
          const canvasLeft = STAGE_W / 2 - dispCanvasW / 2;
          const canvasTop = STAGE_H / 2 - dispCanvasH / 2;
          const previewScale = b.scale > 0 ? a.scale / b.scale : a.scale;
          const imgW = dispCanvasW * previewScale;
          const imgH = dispCanvasH * previewScale;
          const imgLeft =
            canvasLeft + (dispCanvasW - imgW) / 2 + (a.offsetX - b.offsetX) * LAB_DISPLAY_SCALE;
          const imgTop =
            canvasTop + (dispCanvasH - imgH) / 2 + (a.offsetY - b.offsetY) * LAB_DISPLAY_SCALE;
          const hotspotX = canvasLeft + dispCanvasW * 0.5;
          const hotspotY = canvasTop + dispCanvasH * 0.5;

          return (
            <div key={item.key} className="cursor-lab-card rounded-lg border border-[var(--glass-border)] bg-[var(--surface)] p-2">
              <div className="cursor-lab-title text-[10px] text-[var(--on-surface-variant)] truncate mb-1">{item.label}</div>
              <div className="cursor-lab-src text-[10px] text-[var(--on-surface-variant)]/80 truncate mb-1">{item.src}</div>
              <div
                className={`cursor-lab-stage relative overflow-hidden rounded-md border cursor-grab ${
                  focusedKey === item.key
                    ? 'border-[var(--primary-color)] ring-1 ring-[var(--primary-color)]/60'
                    : 'border-[var(--glass-border)]'
                }`}
                tabIndex={0}
                style={{
                  width: `${STAGE_W}px`,
                  height: `${STAGE_H}px`,
                  backgroundImage: 'linear-gradient(45deg,#1f1f1f 25%,transparent 25%),linear-gradient(-45deg,#1f1f1f 25%,transparent 25%),linear-gradient(45deg,transparent 75%,#1f1f1f 75%),linear-gradient(-45deg,transparent 75%,#1f1f1f 75%)',
                  backgroundSize: '24px 24px',
                  backgroundPosition: '0 0,0 12px,12px -12px,-12px 0',
                  backgroundColor: '#111111',
                }}
                onPointerDown={(e) => {
                  setFocusedKey(item.key);
                  e.currentTarget.focus();
                  dragRef.current = {
                    key: item.key,
                    startX: e.clientX,
                    startY: e.clientY,
                    startOffsetX: a.offsetX,
                    startOffsetY: a.offsetY,
                  };
                  const move = (me: PointerEvent) => {
                    const d = dragRef.current;
                    if (!d || d.key !== item.key) return;
                    setAdjust((prev) => ({
                      ...prev,
                      [item.key]: {
                      ...prev[item.key],
                        offsetX: d.startOffsetX + (me.clientX - d.startX) / LAB_DISPLAY_SCALE,
                        offsetY: d.startOffsetY + (me.clientY - d.startY) / LAB_DISPLAY_SCALE,
                      },
                    }));
                  };
                  const up = () => {
                    window.removeEventListener('pointermove', move);
                    window.removeEventListener('pointerup', up);
                    dragRef.current = null;
                  };
                  window.addEventListener('pointermove', move);
                  window.addEventListener('pointerup', up);
                }}
                onKeyDown={(e) => {
                  if (focusedKey !== item.key) return;
                  let dx = 0;
                  let dy = 0;
                  const step = e.shiftKey ? 1 : 0.25;
                  if (e.key === 'ArrowLeft') dx = -1;
                  else if (e.key === 'ArrowRight') dx = 1;
                  else if (e.key === 'ArrowUp') dy = -1;
                  else if (e.key === 'ArrowDown') dy = 1;
                  if (dx === 0 && dy === 0) return;
                  e.preventDefault();
                  e.stopPropagation();
                  setAdjust((prev) => ({
                    ...prev,
                    [item.key]: {
                      ...prev[item.key],
                      offsetX: prev[item.key].offsetX + dx * step,
                      offsetY: prev[item.key].offsetY + dy * step,
                    },
                  }));
                }}
              >
                <img
                  src={`${item.src}?v=cursor-lab-v9-apply-${assetVersion}`}
                  alt=""
                  className="cursor-lab-cursor-image absolute pointer-events-none select-none"
                  style={{ left: imgLeft, top: imgTop, width: imgW, height: imgH }}
                />
                <div
                  className="cursor-lab-canvas-frame absolute pointer-events-none border border-[#ffd166]"
                  style={{ left: canvasLeft, top: canvasTop, width: dispCanvasW, height: dispCanvasH }}
                />
                <div
                  className="cursor-lab-hotspot-center absolute pointer-events-none"
                  style={{ left: hotspotX, top: hotspotY, transform: 'translate(-50%,-50%)' }}
                >
                  <div className="w-2 h-2 rounded-full border border-red-500/95" />
                  <div className="w-px h-px rounded-full bg-red-500/90 absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2" />
                </div>
              </div>

              <div className="cursor-lab-controls mt-2 space-y-1.5">
                <div className="cursor-lab-scale-row flex items-center gap-2">
                  <span className="text-[10px] w-9 text-[var(--on-surface-variant)]">Scale</span>
                  <input
                    type="range"
                    min="0.2"
                    max="4"
                    step="0.01"
                    value={a.scale}
                    onChange={(e) => {
                      const next = Number(e.target.value);
                      setAdjust((prev) => ({ ...prev, [item.key]: { ...prev[item.key], scale: next } }));
                    }}
                    className="cursor-lab-scale-slider flex-1 min-w-0"
                  />
                  <span className="text-[10px] tabular-nums w-9 text-right">{a.scale.toFixed(2)}</span>
                </div>
                <div className="cursor-lab-meta-row flex items-center justify-between">
                  <span className="text-[10px] text-[var(--on-surface-variant)]">
                    offset {a.offsetX.toFixed(2)}, {a.offsetY.toFixed(2)}
                  </span>
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => applyOne(item)}
                      disabled={Boolean(applying[item.key])}
                      className="cursor-lab-apply-button text-[10px] px-2 py-0.5 rounded border border-[var(--primary-color)] text-[var(--primary-color)] hover:bg-[var(--glass-bg)] disabled:opacity-50"
                    >
                      {applying[item.key] ? 'Applying...' : 'Apply'}
                    </button>
                    <button
                      onClick={() => resetOne(item.key)}
                      className="cursor-lab-reset-button text-[10px] px-2 py-0.5 rounded border border-[var(--glass-border)] hover:bg-[var(--glass-bg)]"
                    >
                      Reset
                    </button>
                    <span className="text-[10px] text-[var(--on-surface-variant)] min-w-7">
                      {applyStatus[item.key] === 'ok' ? 'Done' : applyStatus[item.key] === 'fail' ? 'Err' : ''}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function parseCursorGeometry(svg: string): { scale: number; offsetX: number; offsetY: number } | null {
  const nested = svg.match(
    /<svg\s+x="([-0-9.]+)"\s+y="([-0-9.]+)"\s+width="([-0-9.]+)"\s+height="([-0-9.]+)"\s+viewBox="/
  );
  if (nested) {
    const x = Number(nested[1]);
    const y = Number(nested[2]);
    const width = Number(nested[3]);
    const height = Number(nested[4]);
    if (![x, y, width, height].every(Number.isFinite)) return null;
    const scaleX = width / 44;
    const scaleY = height / 43;
    const scale = Number(((scaleX + scaleY) * 0.5).toFixed(4));
    const offsetX = Number((x - (44 - width) * 0.5).toFixed(2));
    const offsetY = Number((y - (43 - height) * 0.5).toFixed(2));
    return { scale, offsetX, offsetY };
  }

  const group = svg.match(
    /<g\s+transform="translate\(([-0-9.]+)[\s,]+([-0-9.]+)\)\s*scale\(([-0-9.]+)\)"/
  );
  if (group) {
    const offsetX = Number(group[1]);
    const offsetY = Number(group[2]);
    const scale = Number(group[3]);
    if (![offsetX, offsetY, scale].every(Number.isFinite)) return null;
    return { scale, offsetX, offsetY };
  }

  return null;
}
