import type { OrbitalArcsBackgroundPreset } from '@/lib/backgroundPresets';
import { clamp01, hexToLinear, linearToSrgb, mix, smoothstep } from './gradientMath';

function hashNoise(x: number, y: number): number {
  const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
  return noiseSeed - Math.floor(noiseSeed);
}

function sampleOrbitalArc(
  point: [number, number],
  center: [number, number],
  radius: number,
  thickness: number,
  intensity: number
): { field: number; band: number; core: number } {
  const ringDistance = Math.abs(Math.hypot(point[0] - center[0], point[1] - center[1]) - radius);
  const edgeSoftness = Math.max(thickness * 0.62, 0.025);
  const field = (1 - smoothstep(thickness * 1.4, thickness * 6.8, ringDistance)) * intensity;
  const band = (1 - smoothstep(thickness, thickness + edgeSoftness, ringDistance)) * intensity;
  const core = (1 - smoothstep(thickness * 0.18, thickness * 0.72, ringDistance)) * intensity;
  return { field, band, core };
}

function sampleOrbitalSweepWeight(
  point: [number, number],
  center: [number, number],
  canvasCenter: [number, number]
): number {
  const pointDx = point[0] - center[0];
  const pointDy = point[1] - center[1];
  const focusDx = canvasCenter[0] - center[0];
  const focusDy = canvasCenter[1] - center[1];
  const pointLen = Math.max(Math.hypot(pointDx, pointDy), 1e-5);
  const focusLen = Math.max(Math.hypot(focusDx, focusDy), 1e-5);
  const facing =
    ((pointDx / pointLen) * (focusDx / focusLen)) +
    ((pointDy / pointLen) * (focusDy / focusLen));
  return mix(0.3, 1, smoothstep(0.08, 0.96, (facing * 0.5) + 0.5));
}

export function fillOrbitalArcsBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: OrbitalArcsBackgroundPreset
): void {
  const baseStartColor = hexToLinear(preset.colors.baseStart);
  const baseEndColor = hexToLinear(preset.colors.baseEnd);
  const arcAColor = hexToLinear(preset.colors.arcA);
  const arcBColor = hexToLinear(preset.colors.arcB);
  const arcCColor = hexToLinear(preset.colors.arcC);
  const aspect = width / Math.max(1, height);
  const arcACenter: [number, number] = [preset.arcACenter[0] * aspect, preset.arcACenter[1]];
  const arcBCenter: [number, number] = [preset.arcBCenter[0] * aspect, preset.arcBCenter[1]];
  const arcCCenter: [number, number] = [preset.arcCCenter[0] * aspect, preset.arcCCenter[1]];
  const canvasCenter: [number, number] = [aspect * 0.5, 0.52];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const baseMix = clamp01((ux * 0.68) + ((1 - uy) * 0.32));
      const ambient = clamp01(((1 - uy) * 0.28) + ((1 - ux) * 0.08) + 0.2);
      const edgeBias = mix(
        preset.centerCalm,
        1,
        smoothstep(0.16, 0.88, Math.hypot(centeredX, centeredY))
      );

      let litR = mix(baseStartColor[0], baseEndColor[0], baseMix) * mix(0.82, 0.96, ambient);
      let litG = mix(baseStartColor[1], baseEndColor[1], baseMix) * mix(0.82, 0.96, ambient);
      let litB = mix(baseStartColor[2], baseEndColor[2], baseMix) * mix(0.82, 0.96, ambient);

      const arcA = sampleOrbitalArc(
        point,
        arcACenter,
        preset.arcARadius,
        preset.arcAThickness,
        preset.arcAIntensity
      );
      const arcB = sampleOrbitalArc(
        point,
        arcBCenter,
        preset.arcBRadius,
        preset.arcBThickness,
        preset.arcBIntensity
      );
      const arcC = sampleOrbitalArc(
        point,
        arcCCenter,
        preset.arcCRadius,
        preset.arcCThickness,
        preset.arcCIntensity
      );
      const sweepA = sampleOrbitalSweepWeight(point, arcACenter, canvasCenter);
      const sweepB = sampleOrbitalSweepWeight(point, arcBCenter, canvasCenter);
      const sweepC = sampleOrbitalSweepWeight(point, arcCCenter, canvasCenter);

      const arcFieldA = ((arcA.field * 0.025) + (arcA.band * 1.18)) * sweepA;
      const arcFieldB = ((arcB.field * 0.03) + (arcB.band * 1.2)) * sweepB;
      const arcFieldC = ((arcC.field * 0.025) + (arcC.band * 1.16)) * sweepC;
      litR += ((arcAColor[0] * arcFieldA) + (arcBColor[0] * arcFieldB) + (arcCColor[0] * arcFieldC)) * edgeBias;
      litG += ((arcAColor[1] * arcFieldA) + (arcBColor[1] * arcFieldB) + (arcCColor[1] * arcFieldC)) * edgeBias;
      litB += ((arcAColor[2] * arcFieldA) + (arcBColor[2] * arcFieldB) + (arcCColor[2] * arcFieldC)) * edgeBias;

      const glowLift = (
        (arcA.core * 0.22 * sweepA) +
        (arcB.core * 0.24 * sweepB) +
        (arcC.core * 0.2 * sweepC) +
        ((arcA.field + arcB.field + arcC.field) * 0.004)
      ) * edgeBias;
      const overlap =
        Math.max(((arcA.band * sweepA) + (arcB.band * sweepB) + (arcC.band * sweepC)) - 0.86, 0) *
        preset.overlapGlow *
        edgeBias;
      const glowR = mix(((arcAColor[0] * 0.26) + (arcBColor[0] * 0.38) + (arcCColor[0] * 0.36)), 1, 0.02);
      const glowG = mix(((arcAColor[1] * 0.26) + (arcBColor[1] * 0.38) + (arcCColor[1] * 0.36)), 1, 0.02);
      const glowB = mix(((arcAColor[2] * 0.26) + (arcBColor[2] * 0.38) + (arcCColor[2] * 0.36)), 1, 0.02);
      litR += glowR * (glowLift + overlap);
      litG += glowG * (glowLift + overlap);
      litB += glowB * (glowLift + overlap);

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
