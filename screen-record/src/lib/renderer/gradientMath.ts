export function clamp01(v: number): number {
  return Math.max(0, Math.min(1, v));
}

export function mix(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = clamp01((x - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
}

export function linearToSrgb(c: number): number {
  if (c <= 0.0031308) return c * 12.92;
  return 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
}

export function hexToLinear(hex: string): [number, number, number] {
  const raw = hex.replace('#', '');
  const r = parseInt(raw.slice(0, 2), 16) / 255;
  const g = parseInt(raw.slice(2, 4), 16) / 255;
  const b = parseInt(raw.slice(4, 6), 16) / 255;
  const toLinear = (c: number) => c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  return [toLinear(r), toLinear(g), toLinear(b)];
}
