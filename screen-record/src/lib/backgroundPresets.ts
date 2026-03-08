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

export type BuiltInBackgroundPreset =
  | LinearBackgroundPreset
  | StackedRadialBackgroundPreset
  | DiagonalGlowBackgroundPreset
  | EdgeRibbonBackgroundPreset
  | PrismFoldBackgroundPreset;

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
