export interface TextSplitPreview {
  leftText: string;
  rightText: string;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function distributeByUnits(
  units: string[],
  ratio: number,
): TextSplitPreview | null {
  if (units.length < 2) return null;
  const splitIndex = clamp(Math.round(units.length * ratio), 1, units.length - 1);
  return {
    leftText: units.slice(0, splitIndex).join(''),
    rightText: units.slice(splitIndex).join(''),
  };
}

export function buildTextSplitPreview(params: {
  text: string;
  startTime: number;
  endTime: number;
  splitTime: number;
}): TextSplitPreview | null {
  const trimmed = params.text.trim();
  if (!trimmed) return null;
  const duration = params.endTime - params.startTime;
  if (duration <= 0.0001) return null;

  const ratio = clamp(
    (params.splitTime - params.startTime) / duration,
    0.01,
    0.99,
  );

  const wordTokens = trimmed.split(/\s+/).filter(Boolean);
  if (wordTokens.length >= 2) {
    const wordPreview = distributeByUnits(
      wordTokens.map((token, index) => (index === 0 ? token : ` ${token}`)),
      ratio,
    );
    if (wordPreview) return wordPreview;
  }

  return distributeByUnits(Array.from(trimmed), ratio);
}
