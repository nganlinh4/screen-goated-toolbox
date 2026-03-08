import type { MeltedGlassBackgroundPreset } from '@/lib/backgroundPresets';
import { clamp01, hexToLinear, linearToSrgb, mix, smoothstep } from './gradientMath';

function hashNoise(x: number, y: number): number {
  const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
  return noiseSeed - Math.floor(noiseSeed);
}

function samplePoolField(
  point: [number, number],
  center: [number, number],
  radius: number,
  weight: number
): { value: number; gradX: number; gradY: number } {
  const dx = point[0] - center[0];
  const dy = point[1] - center[1];
  const radiusSq = Math.max(radius * radius, 1e-5);
  const exponent = -((dx * dx) + (dy * dy)) / radiusSq;
  const value = Math.exp(exponent) * weight;
  const gradientScale = (-2 / radiusSq) * value;
  return {
    value,
    gradX: dx * gradientScale,
    gradY: dy * gradientScale,
  };
}

export function fillMeltedGlassBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: MeltedGlassBackgroundPreset
): void {
  const baseStartColor = hexToLinear(preset.colors.baseStart);
  const baseEndColor = hexToLinear(preset.colors.baseEnd);
  const poolAColor = hexToLinear(preset.colors.poolA);
  const poolBColor = hexToLinear(preset.colors.poolB);
  const poolCColor = hexToLinear(preset.colors.poolC);
  const aspect = width / Math.max(1, height);
  const poolACenter: [number, number] = [preset.poolACenter[0] * aspect, preset.poolACenter[1]];
  const poolBCenter: [number, number] = [preset.poolBCenter[0] * aspect, preset.poolBCenter[1]];
  const poolCCenter: [number, number] = [preset.poolCCenter[0] * aspect, preset.poolCCenter[1]];
  const lightDirX = -0.58;
  const lightDirY = -0.81;

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const baseMix = clamp01((ux * 0.64) + ((1 - uy) * 0.36));
      const ambient = clamp01(((1 - uy) * 0.26) + ((1 - ux) * 0.12) + 0.18);
      const edgeBias = mix(
        preset.centerCalm,
        1,
        smoothstep(0.16, 0.88, Math.hypot(centeredX, centeredY))
      );

      let litR = mix(baseStartColor[0], baseEndColor[0], baseMix) * mix(0.84, 0.98, ambient);
      let litG = mix(baseStartColor[1], baseEndColor[1], baseMix) * mix(0.84, 0.98, ambient);
      let litB = mix(baseStartColor[2], baseEndColor[2], baseMix) * mix(0.84, 0.98, ambient);

      const poolA = samplePoolField(point, poolACenter, preset.poolARadius, preset.poolAWeight);
      const poolB = samplePoolField(point, poolBCenter, preset.poolBRadius, preset.poolBWeight);
      const poolC = samplePoolField(point, poolCCenter, preset.poolCRadius, preset.poolCWeight);
      const field = poolA.value + poolB.value + poolC.value;
      const mask = smoothstep(preset.threshold - preset.softness, preset.threshold + preset.softness, field) * edgeBias;
      const inner = smoothstep(preset.threshold, preset.threshold + (preset.softness * 2.2), field) * edgeBias;
      const rim =
        (
          smoothstep(preset.threshold - preset.softness * 0.9, preset.threshold, field) -
          smoothstep(preset.threshold + preset.softness * 0.4, preset.threshold + preset.softness * 1.8, field)
        ) *
        edgeBias;

      const colorWeight = Math.max(poolA.value + poolB.value + poolC.value, 1e-5);
      const liquidR = ((poolAColor[0] * poolA.value) + (poolBColor[0] * poolB.value) + (poolCColor[0] * poolC.value)) / colorWeight;
      const liquidG = ((poolAColor[1] * poolA.value) + (poolBColor[1] * poolB.value) + (poolCColor[1] * poolC.value)) / colorWeight;
      const liquidB = ((poolAColor[2] * poolA.value) + (poolBColor[2] * poolB.value) + (poolCColor[2] * poolC.value)) / colorWeight;

      const gradX = poolA.gradX + poolB.gradX + poolC.gradX;
      const gradY = poolA.gradY + poolB.gradY + poolC.gradY;
      const gradLength = Math.max(Math.hypot(gradX, gradY), 1e-5);
      const normalX = -gradX / gradLength;
      const normalY = -gradY / gradLength;
      const highlight = Math.pow(clamp01((normalX * lightDirX) + (normalY * lightDirY)), 2.2) * inner * preset.highlightStrength;
      const rimLight = rim * preset.rimStrength;

      const liquidShade = mix(0.88, 1.06, clamp01((normalX * -0.34) + (normalY * -0.66) + 0.5));
      const finalLiquidR = (liquidR * liquidShade) + (mix(liquidR, 1, 0.18) * (rimLight + highlight));
      const finalLiquidG = (liquidG * liquidShade) + (mix(liquidG, 1, 0.18) * (rimLight + highlight));
      const finalLiquidB = (liquidB * liquidShade) + (mix(liquidB, 1, 0.18) * (rimLight + highlight));

      litR = mix(litR, finalLiquidR, mask);
      litG = mix(litG, finalLiquidG, mask);
      litB = mix(litB, finalLiquidB, mask);

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(centeredX, centeredY)
      ) * preset.vignetteStrength;
      litR = mix(litR, litR * 0.84, vignette);
      litG = mix(litG, litG * 0.84, vignette);
      litB = mix(litB, litB * 0.84, vignette);

      if (preset.noiseIntensity > 0) {
        const noise = (hashNoise(x, y) - 0.5) * (preset.noiseIntensity / 255.0);
        litR += noise;
        litG += noise;
        litB += noise;
      }

      data[idx++] = Math.round(clamp01(linearToSrgb(litR)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litG)) * 255);
      data[idx++] = Math.round(clamp01(linearToSrgb(litB)) * 255);
      data[idx++] = 255;
    }
  }
}
