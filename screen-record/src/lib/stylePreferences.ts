import type { TextStyle } from "@/types/video";
import { createPersistedSetting } from "./persistedState";
import { normalizeTextStyle } from "./textStyleDefaults";

type StyleKind = "text" | "subtitle";
const styleSetting = (kind: StyleKind) => createPersistedSetting<TextStyle | null>(
  `screen-record-${kind}-style-default-v1`,
  {
    parse: (raw) => {
      if (!raw) return null;
      const style = JSON.parse(raw);
      if (!style || typeof style !== "object" || typeof style.color !== "string") return null;
      if (![style.fontSize, style.x, style.y].every((value) => typeof value === "number" && Number.isFinite(value))) return null;
      return normalizeTextStyle(style);
    },
    serialize: JSON.stringify,
    fallback: null,
  },
);

export function getDefaultStyle(kind: StyleKind, fallback: TextStyle): TextStyle {
  return structuredClone(styleSetting(kind).getInitial() ?? fallback);
}

export function saveStyleDefault(kind: StyleKind, style: TextStyle) {
  styleSetting(kind).persist(normalizeTextStyle(style));
}
