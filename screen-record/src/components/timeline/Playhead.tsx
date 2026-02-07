import React from 'react';

interface PlayheadProps {
  currentTime: number;
  duration: number;
}

export const Playhead: React.FC<PlayheadProps> = ({ currentTime, duration }) => (
  <div
    className="playhead absolute top-0 bottom-0 flex flex-col items-center pointer-events-none z-40"
    style={{
      left: `${(currentTime / duration) * 100}%`,
      transform: 'translateX(-50%)',
    }}
  >
    <div
      className="w-0 h-0 flex-shrink-0"
      style={{
        borderLeft: '5px solid transparent',
        borderRight: '5px solid transparent',
        borderTop: '6px solid #ef4444',
      }}
    />
    <div className="playhead-line w-0.5 flex-1 bg-red-500" />
  </div>
);
