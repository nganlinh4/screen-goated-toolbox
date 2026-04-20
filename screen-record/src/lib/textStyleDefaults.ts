import type {
  TextAnimationStyle,
  TextBackground,
  TextShadowStyle,
  TextStrokeStyle,
  TextStyle,
  TextWrapStyle,
} from "@/types/video";

export const DEFAULT_TEXT_LINE_HEIGHT = 1.25;
export const DEFAULT_TEXT_WRAP: TextWrapStyle = {
  enabled: true,
  maxWidthPercent: 80,
};
export const DEFAULT_TEXT_STROKE: TextStrokeStyle = {
  enabled: false,
  color: "#000000",
  width: 2,
  opacity: 1,
};
export const DEFAULT_TEXT_SHADOW: TextShadowStyle = {
  enabled: true,
  color: "#000000",
  blur: 4,
  offsetX: 2,
  offsetY: 2,
  opacity: 0.7,
};
export const DEFAULT_TEXT_ANIMATION: TextAnimationStyle = {
  preset: "fade",
  inDuration: 0.3,
  outDuration: 0.3,
};

export function defaultTextBackground(
  overrides: Partial<TextBackground> = {},
): TextBackground {
  return {
    enabled: true,
    color: "#000000",
    opacity: 0.6,
    paddingX: 16,
    paddingY: 8,
    borderRadius: 32,
    ...overrides,
  };
}

export function normalizeTextStyle(style: TextStyle): TextStyle {
  return {
    ...style,
    lineHeight: style.lineHeight ?? DEFAULT_TEXT_LINE_HEIGHT,
    wrap: {
      ...DEFAULT_TEXT_WRAP,
      ...(style.wrap ?? {}),
    },
    stroke: {
      ...DEFAULT_TEXT_STROKE,
      ...(style.stroke ?? {}),
    },
    shadow: {
      ...DEFAULT_TEXT_SHADOW,
      ...(style.shadow ?? {}),
    },
    animation: {
      ...DEFAULT_TEXT_ANIMATION,
      ...(style.animation ?? {}),
    },
    background: style.background
      ? {
          ...defaultTextBackground(),
          ...style.background,
        }
      : style.background,
  };
}
