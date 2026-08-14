import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CanvasModeToggle } from '@/components/CanvasModeToggle';
import type { BackgroundConfig } from '@/types/video';
import { normalizeBackgroundConfig } from '@/lib/backgroundConfig';
import { resolveExportDimensions } from '@/lib/exportEstimator';

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

  it('displays the same canonical custom dimensions that preview and export use', () => {
    const stored = normalizeBackgroundConfig({
      ...backgroundConfig,
      canvasMode: 'custom',
      canvasWidth: 100_001,
      canvasHeight: 50_001,
    });
    const exported = resolveExportDimensions(
      0,
      0,
      stored.canvasWidth!,
      stored.canvasHeight!,
    );
    render(
      <CanvasModeToggle
        backgroundConfig={stored}
        setBackgroundConfig={vi.fn()}
        customCanvasBaseDimensions={exported}
        getAutoCanvasSelectionConfig={() => ({
          canvasMode: 'auto',
          canvasWidth: exported.width,
          canvasHeight: exported.height,
          autoSourceClipId: 'root',
        })}
        handleActivateCustomCanvas={() => {}}
        handleApplyCanvasRatioPreset={() => {}}
        isAutoCanvasDisabled
      />,
    );

    expect(stored.canvasWidth).toBe(exported.width);
    expect(stored.canvasHeight).toBe(exported.height);
    expect(screen.getByRole('button')).toHaveTextContent(
      `Custom ${exported.width}×${exported.height}`,
    );
  });
});
