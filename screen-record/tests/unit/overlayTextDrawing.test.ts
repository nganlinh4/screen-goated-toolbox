import { describe, expect, it } from 'vitest';
import { drawTextOverlay } from '@/lib/renderer/overlayTextRenderer';
import type { TextSegment } from '@/types/video';

type Call = { op: 'fillText' | 'strokeText'; text: string; alpha: number; style: string };

/** Records text draws with the alpha and colour in effect; other canvas calls are no-ops. */
function recordingContext() {
  const calls: Call[] = [];
  const state: Record<string | symbol, unknown> = { globalAlpha: 1, fillStyle: '#000', strokeStyle: '#000', font: '', canvas: { style: {} } };
  const ctx = new Proxy(state, {
    get(target, key) {
      if (key === 'measureText') return (text: string) => ({ width: Array.from(text).length * 10, actualBoundingBoxAscent: 30, actualBoundingBoxDescent: 10 });
      if (key === 'fillText' || key === 'strokeText') {
        return (text: string) => calls.push({
          op: key,
          text,
          alpha: target.globalAlpha as number,
          style: String(key === 'fillText' ? target.fillStyle : target.strokeStyle),
        });
      }
      if (key in target) return target[key];
      return () => {};
    },
    set(target, key, value) {
      target[key] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
  return { ctx, calls };
}

const overlay = (text: string, style: Record<string, unknown>) => ({
  id: 't', startTime: 0, endTime: 5, text,
  style: { fontSize: 40, color: '#ffffff', x: 50, y: 50, ...style },
}) as unknown as TextSegment;

describe('text overlay drawing', () => {
  it('applies stroke opacity once', () => {
    const { ctx, calls } = recordingContext();
    drawTextOverlay(ctx, overlay('Hi', { stroke: { enabled: true, color: '#000000', width: 2, opacity: 0.5 } }), 1000, 500, 1, 1);
    const stroke = calls.find((call) => call.op === 'strokeText')!;
    const colourAlpha = /rgba\([^)]*,\s*([\d.]+)\)/.exec(stroke.style)?.[1];
    expect(stroke.alpha * (colourAlpha ? Number(colourAlpha) : 1)).toBeCloseTo(0.5);
  });

  it('draws an emoji whole when letter spacing is set', () => {
    const { ctx, calls } = recordingContext();
    drawTextOverlay(ctx, overlay('a👍', { letterSpacing: 1 }), 1000, 500, 1, 1);
    expect(calls.filter((call) => call.op === 'fillText').map((call) => call.text)).toEqual(['a', '👍']);
  });

  it('keeps a combined emoji sequence in one draw', () => {
    const { ctx, calls } = recordingContext();
    const family = '👨‍👩‍👧';
    drawTextOverlay(ctx, overlay(`${family}!`, { letterSpacing: 2 }), 1000, 500, 1, 1);
    expect(calls.filter((call) => call.op === 'fillText').map((call) => call.text)).toEqual([family, '!']);
  });
});
