import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CropRatioPanel } from "@/components/CropRatioPanel";
import { getAspectRatioCrop } from "@/lib/cropAspectRatio";

describe("CropRatioPanel", () => {
  it("shows exact output dimensions and applies a centered ratio preset", () => {
    const onCropChange = vi.fn();
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0, y: 0, width: 1, height: 1 }}
        onCropChange={onCropChange}
      />,
    );

    expect(screen.getByRole("button", { name: "Original, 1920×1080" }))
      .toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", {
      name: "Aspect ratio 9:16, 608×1080",
    }));

    expect(onCropChange).toHaveBeenCalledWith(getAspectRatioCrop(1920, 1080, 9, 16));
  });

  it("reflects a manually resized crop as custom", () => {
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0.1, y: 0.1, width: 0.45, height: 0.7 }}
        onCropChange={() => {}}
      />,
    );

    expect(screen.getByText("Custom")).toBeInTheDocument();
    expect(screen.getByText("864 × 756")).toBeInTheDocument();
  });
});
