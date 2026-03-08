import {
  type MatteCollageBackgroundPreset,
  type PrismFoldBackgroundPreset,
  type TopographicFlowBackgroundPreset,
  type WindowlightCausticsBackgroundPreset,
} from '@/lib/backgroundPresets';
import { clamp01, hexToLinear, linearToSrgb, mix, smoothstep } from './gradientMath';

const PRISM_FOLD_ROLE_POINTS: ReadonlyArray<[number, number]> = [
  [0.02, 0.02],
  [0.98, 0.02],
  [0.98, 0.48],
  [0.38, 0.98],
];

const PRISM_FOLD_ROLE_WEIGHTS = [1.0, 0.92, 0.84, 0.96] as const;

function scaleLineForAspect(
  line: [number, number, number, number],
  aspect: number
): [number, number, number, number] {
  return [line[0] * aspect, line[1], line[2] * aspect, line[3]];
}

function signedDistanceToLine(
  point: [number, number],
  line: [number, number, number, number]
): number {
  const dx = line[2] - line[0];
  const dy = line[3] - line[1];
  const invLen = 1 / Math.max(Math.hypot(dx, dy), 1e-6);
  return (((point[0] - line[0]) * -dy) + ((point[1] - line[1]) * dx)) * invLen;
}

function samplePrismPane(
  point: [number, number],
  line: [number, number, number, number],
  referencePoint: [number, number],
  softness: number
): { mask: number; glow: number } {
  const signedDistance = signedDistanceToLine(point, line);
  const referenceSide = signedDistanceToLine(referencePoint, line) >= 0 ? 1 : -1;
  const inside = signedDistance * referenceSide;
  const mask = smoothstep(-softness * 1.2, softness * 3.2, inside);
  const body = smoothstep(softness * 1.4, softness * 7.5, inside);
  const glow = body * (1 - smoothstep(softness * 7.5, softness * 15.0, inside));
  return { mask, glow };
}

function hashNoise(x: number, y: number): number {
  const noiseSeed = Math.sin((x * 12.9898) + (y * 78.233)) * 43758.5453;
  return noiseSeed - Math.floor(noiseSeed);
}

function rotatePoint(x: number, y: number, angle: number): [number, number] {
  const cosA = Math.cos(angle);
  const sinA = Math.sin(angle);
  return [(x * cosA) - (y * sinA), (x * sinA) + (y * cosA)];
}

function signedDistanceRoundedBox(
  point: [number, number],
  halfSize: [number, number],
  radius: number
): number {
  const qx = Math.abs(point[0]) - halfSize[0] + radius;
  const qy = Math.abs(point[1]) - halfSize[1] + radius;
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  const inside = Math.min(Math.max(qx, qy), 0);
  return outside + inside - radius;
}

function sampleMatteCollageLayer(
  point: [number, number],
  center: [number, number],
  radii: [number, number],
  angle: number
): { mask: number; shade: number } {
  const [relX, relY] = rotatePoint(point[0] - center[0], point[1] - center[1], -angle);
  const safeSize: [number, number] = [Math.max(radii[0], 1e-5), Math.max(radii[1], 1e-5)];
  const cornerRadius = Math.max(Math.min(safeSize[1] * 0.42, safeSize[0] * 0.18), 0.02);
  const distance = signedDistanceRoundedBox([relX, relY], safeSize, cornerRadius);
  const edgeSoftness = Math.max(Math.min(safeSize[1] * 0.2, 0.08), 0.028);
  const mask = 1 - smoothstep(0, edgeSoftness, distance);
  const light = clamp01(0.6 + ((-relX / safeSize[0]) * 0.08) + ((-relY / safeSize[1]) * 0.24));
  return { mask, shade: mix(0.92, 1.05, light) };
}

function sampleMatteCollageShadow(
  point: [number, number],
  center: [number, number],
  radii: [number, number],
  angle: number,
  blur: number
): number {
  const [relX, relY] = rotatePoint(point[0] - center[0], point[1] - center[1], -angle);
  const safeSize: [number, number] = [Math.max(radii[0], 1e-5), Math.max(radii[1], 1e-5)];
  const cornerRadius = Math.max(Math.min(safeSize[1] * 0.42, safeSize[0] * 0.18), 0.02);
  const distance = signedDistanceRoundedBox([relX, relY], safeSize, cornerRadius);
  return 1 - smoothstep(0, Math.max(blur, 1e-4), distance);
}

function getMatteCollageAspectScale(aspect: number): number {
  return Math.min(mix(1, aspect, 0.24), 1.28);
}

function sampleWindowlightBeam(
  point: [number, number],
  center: [number, number],
  angle: number,
  width: number,
  length: number,
  intensity: number,
  causticFreq: number,
  causticWarp: number,
  causticStrength: number
): number {
  const cosA = Math.cos(-angle);
  const sinA = Math.sin(-angle);
  const relX = ((point[0] - center[0]) * cosA) - ((point[1] - center[1]) * sinA);
  const relY = ((point[0] - center[0]) * sinA) + ((point[1] - center[1]) * cosA);
  const halfLength = length * 0.5;
  const halfWidth = width * 0.5;
  const radius = halfWidth * 0.82;
  const qx = Math.abs(relX) - halfLength + radius;
  const qy = Math.abs(relY) - halfWidth + radius;
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  const inside = Math.min(Math.max(qx, qy), 0);
  const distance = outside + inside - radius;
  const mask = 1 - smoothstep(0, Math.max(width * 0.9, 0.08), distance);
  const causticA = Math.sin((relX * Math.PI * causticFreq) + (Math.sin(relY * Math.PI * causticFreq * 0.78) * causticWarp));
  const causticB = Math.sin((relY * Math.PI * causticFreq * 0.52) - (relX * Math.PI * 0.35) + 1.3);
  const striation = clamp01(((causticA * 0.5 + 0.5) * 0.72) + ((causticB * 0.5 + 0.5) * 0.28));
  const edgeLift = 1 - smoothstep(halfWidth * 0.18, halfWidth * 1.35, Math.abs(relY));
  const endFade = 1 - smoothstep(halfLength * 0.22, halfLength * 1.04, Math.abs(relX));
  const textured = mix(
    1 - causticStrength,
    1 + (causticStrength * 0.42),
    clamp01((striation * 0.82) + (edgeLift * 0.18))
  );
  return mask * endFade * textured * intensity;
}

export function fillPrismFoldBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: PrismFoldBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const paneColors = [
    hexToLinear(preset.colors.paneA),
    hexToLinear(preset.colors.paneB),
    hexToLinear(preset.colors.paneC),
    hexToLinear(preset.colors.paneD),
  ] as const;
  const aspect = width / Math.max(1, height);
  const paneLines = [
    scaleLineForAspect(preset.paneALine, aspect),
    scaleLineForAspect(preset.paneBLine, aspect),
    scaleLineForAspect(preset.paneCLine, aspect),
    scaleLineForAspect(preset.paneDLine, aspect),
  ] as const;
  const referencePoints = PRISM_FOLD_ROLE_POINTS.map(([x, y]) => [x * aspect, y] as [number, number]);

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const ambient = clamp01(((1 - ux) * 0.52) + ((1 - uy) * 0.48));

      let litR = baseColor[0] * mix(0.84, 1.12, ambient);
      let litG = baseColor[1] * mix(0.84, 1.12, ambient);
      let litB = baseColor[2] * mix(0.84, 1.12, ambient);
      let paneAccumR = 0;
      let paneAccumG = 0;
      let paneAccumB = 0;
      let paneMaskSum = 0;

      for (let paneIndex = 0; paneIndex < paneLines.length; paneIndex++) {
        const { mask, glow } = samplePrismPane(
          point,
          paneLines[paneIndex],
          referencePoints[paneIndex],
          preset.softness
        );
        const roleWeight = PRISM_FOLD_ROLE_WEIGHTS[paneIndex];
        const paneMask = mask * roleWeight;
        const paneColor = paneColors[paneIndex];
        const paneContribution = (paneMask * preset.paneStrength) + (glow * preset.foldStrength * roleWeight);

        litR += paneColor[0] * paneContribution;
        litG += paneColor[1] * paneContribution;
        litB += paneColor[2] * paneContribution;

        paneAccumR += paneColor[0] * paneMask;
        paneAccumG += paneColor[1] * paneMask;
        paneAccumB += paneColor[2] * paneMask;
        paneMaskSum += paneMask;
      }

      const overlap = Math.max(paneMaskSum - 1, 0) * preset.overlapGain;
      if (overlap > 0) {
        const denom = Math.max(paneMaskSum, 1e-4);
        const avgR = paneAccumR / denom;
        const avgG = paneAccumG / denom;
        const avgB = paneAccumB / denom;
        litR += mix(avgR, 1, 0.35) * overlap;
        litG += mix(avgG, 1, 0.35) * overlap;
        litB += mix(avgB, 1, 0.35) * overlap;
      }

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot((ux - 0.5) * aspect, uy - 0.5)
      ) * preset.vignetteStrength;
      litR = mix(litR, litR * 0.82, vignette);
      litG = mix(litG, litG * 0.82, vignette);
      litB = mix(litB, litB * 0.82, vignette);

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

export function fillTopographicFlowBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: TopographicFlowBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const lineAColor = hexToLinear(preset.colors.lineA);
  const lineBColor = hexToLinear(preset.colors.lineB);
  const glowColor = hexToLinear(preset.colors.glow);
  const ink = hexToLinear(preset.colors.ink);
  const aspect = width / Math.max(1, height);
  const sourceA: [number, number] = [preset.sourceA[0] * aspect, preset.sourceA[1]];
  const sourceB: [number, number] = [preset.sourceB[0] * aspect, preset.sourceB[1]];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const point: [number, number] = [ux * aspect, uy];
      const distA = Math.hypot(point[0] - sourceA[0], point[1] - sourceA[1]);
      const distB = Math.hypot(point[0] - sourceB[0], point[1] - sourceB[1]);
      const warp =
        (Math.sin(((point[0] * 0.82) + (point[1] * 1.14)) * Math.PI * 2 * preset.warpFreq) * preset.warpAmp) +
        (Math.sin(((point[0] * -0.58) + (point[1] * 0.92)) * Math.PI * 2 * preset.warpFreq * 0.72) * preset.warpAmp * 0.6);
      const field = ((distA * 0.92) + (distB * 0.78) + warp) * preset.lineScale;
      const line = 1 - smoothstep(preset.lineWidth, preset.lineWidth + 0.22, Math.abs(Math.sin(field * Math.PI)));
      const glow = 1 - smoothstep(
        preset.lineWidth * 2.6,
        (preset.lineWidth * 2.6) + 0.24,
        Math.abs(Math.sin((field + 0.32) * Math.PI))
      );
      const edgeBias = mix(
        preset.centerCalm,
        1,
        smoothstep(0.18, 0.84, Math.hypot(centeredX, centeredY))
      );
      const phaseMix = clamp01((Math.sin((distA - distB) * 4.6) * 0.5) + 0.5);
      const lineR = mix(lineAColor[0], lineBColor[0], phaseMix);
      const lineG = mix(lineAColor[1], lineBColor[1], phaseMix);
      const lineB = mix(lineAColor[2], lineBColor[2], phaseMix);

      let litR = baseColor[0];
      let litG = baseColor[1];
      let litB = baseColor[2];
      litR += lineR * line * preset.lineStrength * edgeBias;
      litG += lineG * line * preset.lineStrength * edgeBias;
      litB += lineB * line * preset.lineStrength * edgeBias;
      litR += glowColor[0] * glow * preset.glowStrength * edgeBias;
      litG += glowColor[1] * glow * preset.glowStrength * edgeBias;
      litB += glowColor[2] * glow * preset.glowStrength * edgeBias;

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(centeredX, centeredY)
      ) * preset.vignetteStrength;
      litR = mix(litR, ink[0], vignette);
      litG = mix(litG, ink[1], vignette);
      litB = mix(litB, ink[2], vignette);

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

export function fillWindowlightCausticsBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: WindowlightCausticsBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const beamAColor = hexToLinear(preset.colors.beamA);
  const beamBColor = hexToLinear(preset.colors.beamB);
  const highlightColor = hexToLinear(preset.colors.highlight);
  const ink = hexToLinear(preset.colors.ink);
  const aspect = width / Math.max(1, height);
  const beamACenter: [number, number] = [preset.beamACenter[0] * aspect, preset.beamACenter[1]];
  const beamBCenter: [number, number] = [preset.beamBCenter[0] * aspect, preset.beamBCenter[1]];
  const highlightCenter: [number, number] = [preset.highlightCenter[0] * aspect, preset.highlightCenter[1]];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const edgeBias = mix(
        preset.centerCalm,
        1,
        smoothstep(0.16, 0.84, Math.hypot(centeredX, centeredY))
      );
      const ambient = clamp01(((1 - uy) * 0.62) + (ux * 0.18) + 0.12);
      let litR = baseColor[0] * mix(0.92, 1.08, ambient);
      let litG = baseColor[1] * mix(0.92, 1.08, ambient);
      let litB = baseColor[2] * mix(0.92, 1.08, ambient);

      const beamA = sampleWindowlightBeam(
        point,
        beamACenter,
        preset.beamAAngle,
        preset.beamAWidth,
        preset.beamALength,
        preset.beamAIntensity,
        preset.causticFreq,
        preset.causticWarp,
        preset.causticStrength
      ) * edgeBias;
      const beamB = sampleWindowlightBeam(
        point,
        beamBCenter,
        preset.beamBAngle,
        preset.beamBWidth,
        preset.beamBLength,
        preset.beamBIntensity,
        preset.causticFreq * 1.12,
        preset.causticWarp * 0.92,
        preset.causticStrength * 0.85
      ) * edgeBias;
      const highlight =
        (1 - smoothstep(0, preset.highlightRadius, Math.hypot(point[0] - highlightCenter[0], point[1] - highlightCenter[1]))) *
        preset.highlightIntensity *
        edgeBias;
      const beamCoreLift = (beamA * 0.18) + (beamB * 0.15);

      litR += (beamAColor[0] * beamA) + (beamBColor[0] * beamB) + (highlightColor[0] * (highlight + beamCoreLift));
      litG += (beamAColor[1] * beamA) + (beamBColor[1] * beamB) + (highlightColor[1] * (highlight + beamCoreLift));
      litB += (beamAColor[2] * beamA) + (beamBColor[2] * beamB) + (highlightColor[2] * (highlight + beamCoreLift));

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(centeredX, centeredY)
      ) * preset.vignetteStrength;
      litR = mix(litR, ink[0], vignette);
      litG = mix(litG, ink[1], vignette);
      litB = mix(litB, ink[2], vignette);

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

export function fillMatteCollageBackgroundPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  preset: MatteCollageBackgroundPreset
): void {
  const baseColor = hexToLinear(preset.colors.base);
  const layerAColor = hexToLinear(preset.colors.layerA);
  const layerBColor = hexToLinear(preset.colors.layerB);
  const layerCColor = hexToLinear(preset.colors.layerC);
  const shadowColor = hexToLinear(preset.colors.shadow);
  const aspect = width / Math.max(1, height);
  const collageAspect = getMatteCollageAspectScale(aspect);
  const layerACenter: [number, number] = [preset.layerACenter[0] * aspect, preset.layerACenter[1]];
  const layerBCenter: [number, number] = [preset.layerBCenter[0] * aspect, preset.layerBCenter[1]];
  const layerCCenter: [number, number] = [preset.layerCCenter[0] * aspect, preset.layerCCenter[1]];
  const layerARadii: [number, number] = [preset.layerARadii[0] * collageAspect, preset.layerARadii[1]];
  const layerBRadii: [number, number] = [preset.layerBRadii[0] * collageAspect, preset.layerBRadii[1]];
  const layerCRadii: [number, number] = [preset.layerCRadii[0] * collageAspect, preset.layerCRadii[1]];
  const shadowOffset: [number, number] = [preset.shadowOffset[0] * collageAspect, preset.shadowOffset[1]];

  let idx = 0;
  for (let y = 0; y < height; y++) {
    const uy = (y + 0.5) / height;
    for (let x = 0; x < width; x++) {
      const ux = (x + 0.5) / width;
      const point: [number, number] = [ux * aspect, uy];
      const centeredX = (ux - 0.5) * aspect;
      const centeredY = uy - 0.5;
      const ambient = clamp01(((1 - uy) * 0.46) + ((1 - ux) * 0.18) + 0.26);
      let litR = baseColor[0] * mix(0.92, 1.06, ambient);
      let litG = baseColor[1] * mix(0.92, 1.06, ambient);
      let litB = baseColor[2] * mix(0.92, 1.06, ambient);

      const shadowA = sampleMatteCollageShadow(
        point,
        [layerACenter[0] + shadowOffset[0], layerACenter[1] + shadowOffset[1]],
        layerARadii,
        preset.layerAAngle,
        preset.shadowBlur
      );
      const shadowB = sampleMatteCollageShadow(
        point,
        [layerBCenter[0] + shadowOffset[0], layerBCenter[1] + shadowOffset[1]],
        layerBRadii,
        preset.layerBAngle,
        preset.shadowBlur
      );
      const shadowC = sampleMatteCollageShadow(
        point,
        [layerCCenter[0] + shadowOffset[0], layerCCenter[1] + shadowOffset[1]],
        layerCRadii,
        preset.layerCAngle,
        preset.shadowBlur
      );
      const shadowMix = clamp01((shadowA + shadowB + shadowC) * preset.shadowStrength);
      litR = mix(litR, shadowColor[0], shadowMix);
      litG = mix(litG, shadowColor[1], shadowMix);
      litB = mix(litB, shadowColor[2], shadowMix);

      const layerA = sampleMatteCollageLayer(point, layerACenter, layerARadii, preset.layerAAngle);
      const layerB = sampleMatteCollageLayer(point, layerBCenter, layerBRadii, preset.layerBAngle);
      const layerC = sampleMatteCollageLayer(point, layerCCenter, layerCRadii, preset.layerCAngle);

      litR = mix(litR, layerAColor[0] * layerA.shade, layerA.mask);
      litG = mix(litG, layerAColor[1] * layerA.shade, layerA.mask);
      litB = mix(litB, layerAColor[2] * layerA.shade, layerA.mask);

      litR = mix(litR, layerBColor[0] * layerB.shade, layerB.mask);
      litG = mix(litG, layerBColor[1] * layerB.shade, layerB.mask);
      litB = mix(litB, layerBColor[2] * layerB.shade, layerB.mask);

      litR = mix(litR, layerCColor[0] * layerC.shade, layerC.mask);
      litG = mix(litG, layerCColor[1] * layerC.shade, layerC.mask);
      litB = mix(litB, layerCColor[2] * layerC.shade, layerC.mask);

      const vignette = smoothstep(
        preset.vignetteStart,
        preset.vignetteEnd,
        Math.hypot(centeredX, centeredY)
      ) * preset.vignetteStrength;
      litR = mix(litR, litR * 0.86, vignette);
      litG = mix(litG, litG * 0.86, vignette);
      litB = mix(litB, litB * 0.86, vignette);

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
