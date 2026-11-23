import type { BackgroundConfig, TextSegment } from "@/types/video";
import { normalizeTextStyle } from "@/lib/textStyleDefaults";

/** The canvas whose pixels every style length is authored in. */
export interface RenderReference {
  width: number;
  height: number;
}

/**
 * Text, keystroke, webcam and frame lengths are authored in pixels of the
 * project canvas that the editor previews (the export "Original" size).
 * Rendering at another output size multiplies every such length by this
 * factor, so each overlay keeps its size relative to the frame in preview and
 * at every export resolution.
 */
export function resolveRenderScale(
  outputHeight: number,
  reference: RenderReference,
): number {
  if (
    !Number.isFinite(outputHeight) ||
    !Number.isFinite(reference.height) ||
    outputHeight <= 0 ||
    reference.height <= 0
  ) {
    return 1;
  }
  return outputHeight / reference.height;
}

/** Returns the text overlay with every pixel length multiplied by `scale`. */
export function scaleTextSegmentForRender(
  text: TextSegment,
  scale: number,
): TextSegment {
  if (scale === 1) return text;
  const style = normalizeTextStyle(text.style);
  const { stroke, shadow, background } = style;
  return {
    ...text,
    style: {
      ...style,
      fontSize: style.fontSize * scale,
      letterSpacing: (style.letterSpacing ?? 0) * scale,
      stroke: stroke && { ...stroke, width: stroke.width * scale },
      shadow: shadow && {
        ...shadow,
        blur: shadow.blur * scale,
        offsetX: shadow.offsetX * scale,
        offsetY: shadow.offsetY * scale,
      },
      background: background && {
        ...background,
        paddingX: background.paddingX * scale,
        paddingY: background.paddingY * scale,
        borderRadius: background.borderRadius * scale,
      },
    },
  };
}

/** Returns the frame styling with its corner radius and shadow in output pixels. */
export function scaleBackgroundFrameForRender(
  config: BackgroundConfig,
  scale: number,
): BackgroundConfig {
  if (scale === 1) return config;
  return {
    ...config,
    borderRadius: config.borderRadius * scale,
    shadow: (config.shadow ?? 0) * scale,
  };
}
