import type { BackgroundConfig } from '@/types/video';
import sharedBackgroundPresets from '@/config/shared-background-presets.json';

export type BuiltInBackgroundId = Exclude<BackgroundConfig['backgroundType'], 'custom'>;

export interface LinearBackgroundPreset {
  family: 'linear';
  axis: 'horizontal' | 'vertical';
  colors: {
    start: string;
    end: string;
  };
}

export interface StackedRadialBackgroundPreset {
  family: 'stacked-radial';
  gradientAxis: 'horizontal' | 'vertical';
  colors: {
    start: string;
    mid: string;
    end: string;
    overlay: string;
  };
  overlayCenter: [number, number];
  overlayRadius: number;
  overlayOpacity: number;
}

export interface DiagonalGlowBackgroundPreset {
  family: 'diagonal-glow';
  colors: {
    start: string;
    mid: string;
    end: string;
    glowAColorLinear: [number, number, number];
    glowBColorLinear: [number, number, number];
  };
  diagWeights: [number, number];
  split: number;
  glowACenter: [number, number];
  glowAOuterRadius: number;
  glowAInnerRadius: number;
  glowAStrength: number;
  glowBCenter: [number, number];
  glowBOuterRadius: number;
  glowBInnerRadius: number;
  glowBStrength: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface EdgeRibbonBackgroundPreset {
  family: 'edge-ribbons';
  colors: {
    base: string;
    depth: string;
    ribbonA: string;
    ribbonB: string;
    glow: string;
  };
  ribbonAStart: [number, number];
  ribbonAEnd: [number, number];
  ribbonAWidth: number;
  ribbonACurveAmp: number;
  ribbonACurveFreq: number;
  ribbonAIntensity: number;
  ribbonBStart: [number, number];
  ribbonBEnd: [number, number];
  ribbonBWidth: number;
  ribbonBCurveAmp: number;
  ribbonBCurveFreq: number;
  ribbonBIntensity: number;
  glowCenter: [number, number];
  glowRadius: number;
  glowIntensity: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface PrismFoldBackgroundPreset {
  family: 'prism-fold';
  colors: {
    base: string;
    paneA: string;
    paneB: string;
    paneC: string;
    paneD: string;
  };
  paneALine: [number, number, number, number];
  paneBLine: [number, number, number, number];
  paneCLine: [number, number, number, number];
  paneDLine: [number, number, number, number];
  paneStrength: number;
  foldStrength: number;
  overlapGain: number;
  softness: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface TopographicFlowBackgroundPreset {
  family: 'topographic-flow';
  colors: {
    base: string;
    lineA: string;
    lineB: string;
    glow: string;
    ink: string;
  };
  sourceA: [number, number];
  sourceB: [number, number];
  lineScale: number;
  warpFreq: number;
  warpAmp: number;
  lineWidth: number;
  lineStrength: number;
  glowStrength: number;
  centerCalm: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface WindowlightCausticsBackgroundPreset {
  family: 'windowlight-caustics';
  colors: {
    base: string;
    beamA: string;
    beamB: string;
    highlight: string;
    ink: string;
  };
  beamACenter: [number, number];
  beamAAngle: number;
  beamAWidth: number;
  beamALength: number;
  beamAIntensity: number;
  beamBCenter: [number, number];
  beamBAngle: number;
  beamBWidth: number;
  beamBLength: number;
  beamBIntensity: number;
  highlightCenter: [number, number];
  highlightRadius: number;
  highlightIntensity: number;
  causticFreq: number;
  causticWarp: number;
  causticStrength: number;
  centerCalm: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface MatteCollageBackgroundPreset {
  family: 'matte-collage';
  colors: {
    base: string;
    layerA: string;
    layerB: string;
    layerC: string;
    shadow: string;
  };
  layerACenter: [number, number];
  layerARadii: [number, number];
  layerAAngle: number;
  layerBCenter: [number, number];
  layerBRadii: [number, number];
  layerBAngle: number;
  layerCCenter: [number, number];
  layerCRadii: [number, number];
  layerCAngle: number;
  shadowOffset: [number, number];
  shadowBlur: number;
  shadowStrength: number;
  noiseIntensity: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
}

export interface OrbitalArcsBackgroundPreset {
  family: 'orbital-arcs';
  colors: {
    baseStart: string;
    baseEnd: string;
    arcA: string;
    arcB: string;
    arcC: string;
  };
  arcACenter: [number, number];
  arcARadius: number;
  arcAThickness: number;
  arcAIntensity: number;
  arcBCenter: [number, number];
  arcBRadius: number;
  arcBThickness: number;
  arcBIntensity: number;
  arcCCenter: [number, number];
  arcCRadius: number;
  arcCThickness: number;
  arcCIntensity: number;
  overlapGlow: number;
  centerCalm: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export interface MeltedGlassBackgroundPreset {
  family: 'melted-glass';
  colors: {
    baseStart: string;
    baseEnd: string;
    poolA: string;
    poolB: string;
    poolC: string;
  };
  poolACenter: [number, number];
  poolARadius: number;
  poolAWeight: number;
  poolBCenter: [number, number];
  poolBRadius: number;
  poolBWeight: number;
  poolCCenter: [number, number];
  poolCRadius: number;
  poolCWeight: number;
  threshold: number;
  softness: number;
  rimStrength: number;
  highlightStrength: number;
  centerCalm: number;
  vignetteStart: number;
  vignetteEnd: number;
  vignetteStrength: number;
  noiseIntensity: number;
}

export type BuiltInBackgroundPreset =
  | LinearBackgroundPreset
  | StackedRadialBackgroundPreset
  | DiagonalGlowBackgroundPreset
  | EdgeRibbonBackgroundPreset
  | PrismFoldBackgroundPreset
  | TopographicFlowBackgroundPreset
  | WindowlightCausticsBackgroundPreset
  | MatteCollageBackgroundPreset
  | OrbitalArcsBackgroundPreset
  | MeltedGlassBackgroundPreset;

interface SharedBackgroundCatalog {
  defaultId: BuiltInBackgroundId;
  panelOrder: BuiltInBackgroundId[];
  backgrounds: Record<BuiltInBackgroundId, BuiltInBackgroundPreset>;
}

const backgroundCatalog = sharedBackgroundPresets as SharedBackgroundCatalog;

export const DEFAULT_BUILT_IN_BACKGROUND_ID = backgroundCatalog.defaultId;
export const BUILT_IN_BACKGROUND_PANEL_ORDER = backgroundCatalog.panelOrder;

export function getBuiltInBackgroundPreset(id: BuiltInBackgroundId): BuiltInBackgroundPreset {
  return backgroundCatalog.backgrounds[id];
}

export function isBuiltInBackgroundId(value: string): value is BuiltInBackgroundId {
  return value in backgroundCatalog.backgrounds;
}
