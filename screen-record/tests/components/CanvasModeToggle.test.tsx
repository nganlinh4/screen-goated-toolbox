import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CanvasModeToggle } from '@/components/CanvasModeToggle';
import type { BackgroundConfig } from '@/types/video';

const backgroundConfig: BackgroundConfig = {
  scale: 100,
  borderRadius: 0,
  backgroundType: 'gradient4',
  canvasMode: 'auto',
  canvasWidth: 376,
  canvasHeight: 108,
};

describe('CanvasModeToggle', () => {
  it('shows the same resolved auto dimensions used by preview and export', () => {
    const setBackgroundConfig = vi.fn();
    render(
      <CanvasModeToggle
        backgroundConfig={backgroundConfig}
        setBackgroundConfig={setBackgroundConfig}
        customCanvasBaseDimensions={{ width: 376, height: 108 }}
        getAutoCanvasSelectionConfig={() => ({
          canvasMode: 'auto',
          canvasWidth: 376,
          canvasHeight: 108,
          autoSourceClipId: 'root',
        })}
        handleActivateCustomCanvas={() => {}}
        handleApplyCanvasRatioPreset={() => {}}
      />,
    );

    const autoButton = screen.getByRole('button', { name: 'Auto 376×108' });
    expect(autoButton).toHaveAttribute('aria-pressed', 'true');
    expect(document.querySelectorAll('.playback-canvas-ratio-btn')).toHaveLength(5);

    fireEvent.click(autoButton);
    expect(setBackgroundConfig).toHaveBeenCalledOnce();
  });
});
