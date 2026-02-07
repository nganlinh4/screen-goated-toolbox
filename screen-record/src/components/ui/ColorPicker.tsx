import { useState, useRef, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';

const PRESETS = [
  '#ffffff', '#e0e0e0', '#9e9e9e', '#616161', '#212121', '#000000',
  '#f44336', '#ff9800', '#ffeb3b', '#4caf50', '#2196f3', '#9c27b0',
  '#ef9a9a', '#ffcc80', '#fff59d', '#a5d6a7', '#90caf9', '#ce93d8',
];

function hsvToHex(h: number, s: number, v: number): string {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  let r = 0, g = 0, b = 0;
  if (h < 60) { r = c; g = x; }
  else if (h < 120) { r = x; g = c; }
  else if (h < 180) { g = c; b = x; }
  else if (h < 240) { g = x; b = c; }
  else if (h < 300) { r = x; b = c; }
  else { r = c; b = x; }
  const hex = (n: number) => Math.round((n + m) * 255).toString(16).padStart(2, '0');
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

function hexToHsv(hex: string): [number, number, number] {
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const d = max - min;
  const v = max;
  const s = max === 0 ? 0 : d / max;
  let h = 0;
  if (d > 0) {
    if (max === r) h = 60 * (((g - b) / d + 6) % 6);
    else if (max === g) h = 60 * ((b - r) / d + 2);
    else h = 60 * ((r - g) / d + 4);
  }
  return [h, s, v];
}

interface ColorPickerProps {
  value: string;
  onChange: (color: string) => void;
  onOpen?: () => void;
  onClose?: () => void;
}

export function ColorPicker({ value, onChange, onOpen, onClose }: ColorPickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [hexInput, setHexInput] = useState(value);
  const [hsv, setHsv] = useState<[number, number, number]>(() => {
    try { return hexToHsv(value); } catch { return [0, 0, 1]; }
  });
  const [pos, setPos] = useState({ top: 0, left: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const svRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);
  const hsvRef = useRef(hsv);
  hsvRef.current = hsv;

  useEffect(() => {
    setHexInput(value);
    try { setHsv(hexToHsv(value)); } catch {}
  }, [value]);

  const emitColor = useCallback((h: number, s: number, v: number) => {
    const hex = hsvToHex(h, s, v);
    const next: [number, number, number] = [h, s, v];
    hsvRef.current = next;
    setHsv(next);
    setHexInput(hex);
    onChange(hex);
  }, [onChange]);

  const open = useCallback(() => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (rect) {
      const popW = 200, popH = 280;
      let top = rect.bottom + 4;
      let left = rect.left;
      if (top + popH > window.innerHeight) top = rect.top - popH - 4;
      if (left + popW > window.innerWidth) left = window.innerWidth - popW - 8;
      setPos({ top, left });
    }
    setIsOpen(true);
    onOpen?.();
  }, [onOpen]);

  const close = useCallback(() => {
    setIsOpen(false);
    onClose?.();
  }, [onClose]);

  useEffect(() => {
    if (!isOpen) return;
    const handle = (e: MouseEvent) => {
      if (triggerRef.current?.contains(e.target as Node) || popoverRef.current?.contains(e.target as Node)) return;
      close();
    };
    document.addEventListener('mousedown', handle);
    return () => document.removeEventListener('mousedown', handle);
  }, [isOpen, close]);

  const handleSVDrag = useCallback((startE: React.MouseEvent) => {
    startE.preventDefault();
    const rect = svRef.current!.getBoundingClientRect();
    const update = (cx: number, cy: number) => {
      const s = Math.max(0, Math.min(1, (cx - rect.left) / rect.width));
      const v = Math.max(0, Math.min(1, 1 - (cy - rect.top) / rect.height));
      emitColor(hsvRef.current[0], s, v);
    };
    update(startE.clientX, startE.clientY);
    const onMove = (e: MouseEvent) => update(e.clientX, e.clientY);
    const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, [emitColor]);

  const handleHueDrag = useCallback((startE: React.MouseEvent) => {
    startE.preventDefault();
    const rect = hueRef.current!.getBoundingClientRect();
    const update = (cx: number) => {
      const h = Math.max(0, Math.min(360, ((cx - rect.left) / rect.width) * 360));
      emitColor(h, hsvRef.current[1], hsvRef.current[2]);
    };
    update(startE.clientX);
    const onMove = (e: MouseEvent) => update(e.clientX);
    const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, [emitColor]);

  return (
    <>
      <button
        ref={triggerRef}
        onClick={() => isOpen ? close() : open()}
        className="w-7 h-6 rounded border border-[var(--glass-border)] cursor-pointer transition-shadow hover:shadow-[0_0_0_2px_var(--primary-color)/30]"
        style={{ backgroundColor: value }}
      />
      {isOpen && createPortal(
        <div
          ref={popoverRef}
          className="fixed z-[9999] bg-[var(--surface-dim)] backdrop-blur-xl border border-[var(--glass-border)] rounded-xl p-2.5 shadow-[0_8px_32px_rgba(0,0,0,0.4)]"
          style={{ top: pos.top, left: pos.left, width: 200 }}
        >
          {/* SV square */}
          <div
            ref={svRef}
            className="w-full h-[120px] rounded-lg cursor-crosshair relative mb-2 border border-[var(--glass-border)]"
            style={{
              background: `linear-gradient(to bottom, transparent, #000), linear-gradient(to right, #fff, ${hsvToHex(hsv[0], 1, 1)})`,
            }}
            onMouseDown={handleSVDrag}
          >
            <div
              className="absolute w-3 h-3 rounded-full border-2 border-white shadow-[0_0_4px_rgba(0,0,0,0.5)] -translate-x-1/2 -translate-y-1/2 pointer-events-none"
              style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%` }}
            />
          </div>

          {/* Hue bar */}
          <div
            ref={hueRef}
            className="w-full h-3 rounded-full cursor-pointer relative mb-2.5 border border-[var(--glass-border)]"
            style={{ background: 'linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)' }}
            onMouseDown={handleHueDrag}
          >
            <div
              className="absolute w-3.5 h-3.5 rounded-full border-2 border-white shadow-[0_0_4px_rgba(0,0,0,0.5)] -translate-x-1/2 -translate-y-1/2 pointer-events-none"
              style={{ left: `${(hsv[0] / 360) * 100}%`, top: '50%' }}
            />
          </div>

          {/* Presets */}
          <div className="grid grid-cols-6 gap-1.5 mb-2">
            {PRESETS.map(color => (
              <button
                key={color}
                onClick={() => {
                  onChange(color);
                  setHexInput(color);
                  try { setHsv(hexToHsv(color)); } catch {}
                }}
                className={`w-6 h-6 rounded-md transition-all hover:scale-110 ${
                  value.toLowerCase() === color.toLowerCase()
                    ? 'ring-2 ring-[var(--primary-color)] ring-offset-1 ring-offset-[var(--surface-dim)]'
                    : 'ring-1 ring-white/10 hover:ring-white/30'
                }`}
                style={{ backgroundColor: color }}
              />
            ))}
          </div>

          {/* Hex input */}
          <input
            type="text"
            value={hexInput}
            onChange={(e) => {
              const v = e.target.value;
              setHexInput(v);
              if (/^#[0-9a-fA-F]{6}$/.test(v)) {
                onChange(v);
                try { setHsv(hexToHsv(v)); } catch {}
              }
            }}
            onKeyDown={(e) => { if (e.key === 'Enter') close(); }}
            className="w-full bg-[var(--glass-bg)] border border-[var(--glass-border)] rounded-lg px-2.5 py-1.5 text-xs text-[var(--on-surface)] font-mono focus:border-[var(--primary-color)]/50 focus:ring-1 focus:ring-[var(--primary-color)]/30 transition-colors"
            placeholder="#000000"
          />
        </div>,
        document.body
      )}
    </>
  );
}
