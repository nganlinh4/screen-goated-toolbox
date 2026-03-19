export function CanvasRatioIcon({
  ratioWidth,
  ratioHeight,
}: {
  ratioWidth: number;
  ratioHeight: number;
}) {
  const frameSize = 12;
  const scale = Math.min(frameSize / ratioWidth, frameSize / ratioHeight);
  const width = ratioWidth * scale;
  const height = ratioHeight * scale;
  const x = (18 - width) / 2;
  const y = (18 - height) / 2;
  const radius = Math.max(1.5, Math.min(width, height) * 0.14);

  return (
    <svg
      className="canvas-ratio-icon h-4 w-4 flex-shrink-0"
      viewBox="0 0 18 18"
      fill="none"
      aria-hidden="true"
    >
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        rx={radius}
        stroke="currentColor"
        strokeWidth="1.5"
      />
    </svg>
  );
}
