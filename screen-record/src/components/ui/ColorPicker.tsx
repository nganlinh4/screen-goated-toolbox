import { useState, useRef, useEffect, useCallback } from 'react';
import * as Popover from '@radix-ui/react-popover';
import { motion, AnimatePresence } from 'framer-motion';

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
    <Popover.Root
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (open) onOpen?.();
        else onClose?.();
      }}
    >
      <Popover.Trigger asChild>
        <button
          className="color-picker-trigger ui-chip-button w-7 h-6 rounded cursor-pointer active:scale-95"
          style={{ backgroundColor: value }}
        />
      </Popover.Trigger>
      <AnimatePresence>
        {isOpen && (
          <Popover.Portal forceMount>
            <Popover.Content
              sideOffset={4}
              collisionPadding={8}
              asChild
              onOpenAutoFocus={(e) => e.preventDefault()}
            >
              <motion.div
                className="color-picker-popover material-surface-elevated relative z-[9999] w-[200px] rounded-xl p-2.5"
                initial={{ opacity: 0, scale: 0.95, y: -4 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.95, y: -4 }}
                transition={{ type: 'spring', stiffness: 500, damping: 30 }}
              >
                {/* SV square */}
                <div
                  ref={svRef}
                  className="color-sv-square ui-surface w-full h-[120px] rounded-lg cursor-crosshair relative mb-2"
                  style={{
                    background: `linear-gradient(to bottom, transparent, #000), linear-gradient(to right, #fff, ${hsvToHex(hsv[0], 1, 1)})`,
                  }}
                  onMouseDown={handleSVDrag}
                >
                  <div
                    className="sv-cursor absolute w-3 h-3 rounded-full border-2 border-white shadow-[0_0_4px_rgba(0,0,0,0.5)] -translate-x-1/2 -translate-y-1/2 pointer-events-none"
                    style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%` }}
                  />
                </div>

                {/* Hue bar */}
                <div
                  ref={hueRef}
                  className="color-hue-bar ui-surface w-full h-3 rounded-full cursor-pointer relative mb-2.5"
                  style={{ background: 'linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)' }}
                  onMouseDown={handleHueDrag}
                >
                  <div
                    className="hue-cursor absolute w-3.5 h-3.5 rounded-full border-2 border-white shadow-[0_0_4px_rgba(0,0,0,0.5)] -translate-x-1/2 -translate-y-1/2 pointer-events-none"
                    style={{ left: `${(hsv[0] / 360) * 100}%`, top: '50%' }}
                  />
                </div>

                {/* Presets */}
                <div className="color-presets-grid grid grid-cols-6 gap-1.5 mb-2">
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
                  onKeyDown={(e) => { if (e.key === 'Enter') setIsOpen(false); }}
                  className="hex-input ui-input w-full rounded-lg px-2.5 py-1.5 text-xs text-[var(--on-surface)] font-mono"
                  placeholder="#000000"
                />
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        )}
      </AnimatePresence>
    </Popover.Root>
  );
}
