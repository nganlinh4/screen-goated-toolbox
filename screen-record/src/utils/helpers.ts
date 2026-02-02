import { MonitorInfo } from '@/hooks/useAppHooks';
import { ZoomKeyframe } from '@/types/video';

export const sortMonitorsByPosition = (monitors: MonitorInfo[]) => {
  return [...monitors]
    .sort((a, b) => a.x - b.x)
    .map((monitor, index) => ({
      ...monitor,
      name: `Display ${index + 1}${monitor.is_primary ? ' (Primary)' : ''}`
    }));
};

export const getKeyframeRange = (
  keyframes: ZoomKeyframe[],
  index: number
): { rangeStart: number; rangeEnd: number } => {
  const keyframe = keyframes[index];
  const prevKeyframe = index > 0 ? keyframes[index - 1] : null;
  const rangeStart =
    prevKeyframe && keyframe.time - prevKeyframe.time <= 1.0
      ? prevKeyframe.time
      : Math.max(0, keyframe.time - 1.0);
  return { rangeStart, rangeEnd: keyframe.time };
};

export function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}
