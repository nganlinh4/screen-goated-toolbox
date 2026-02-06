import React from 'react';

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

interface TimeRulerProps {
  duration: number;
}

export const TimeRuler: React.FC<TimeRulerProps> = ({ duration }) => {
  // Adaptive tick density: aim for ~8-12 ticks depending on duration
  const getTickCount = () => {
    if (duration <= 5) return 5;
    if (duration <= 15) return 8;
    if (duration <= 30) return 10;
    if (duration <= 60) return 12;
    return 15;
  };

  const tickCount = getTickCount();

  return (
    <div className="relative h-5 select-none">
      <div className="absolute inset-0 flex items-end">
        {Array.from({ length: tickCount + 1 }).map((_, i) => {
          const time = (duration * i) / tickCount;
          const left = (i / tickCount) * 100;
          const isMajor = i === 0 || i === tickCount || i % Math.ceil(tickCount / 4) === 0;

          return (
            <div
              key={i}
              className="absolute flex flex-col items-center"
              style={{ left: `${left}%`, transform: 'translateX(-50%)' }}
            >
              {isMajor && (
                <span className="text-[10px] font-mono text-[var(--outline)] leading-none mb-0.5">
                  {formatTime(time)}
                </span>
              )}
              <div
                className={`w-px ${isMajor ? 'h-1.5 bg-[var(--outline)]/40' : 'h-1 bg-[var(--outline)]/20'}`}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
};
