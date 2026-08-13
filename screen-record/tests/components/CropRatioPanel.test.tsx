import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CropRatioPanel } from "@/components/CropRatioPanel";
import { getAspectRatioCrop } from "@/lib/cropAspectRatio";
import { CROP_ASPECT_RATIO_PRESETS } from "@/lib/aspectRatioPresets";

describe("CropRatioPanel", () => {
  it("shows exact output dimensions and applies a centered ratio preset", () => {
    const onCropChange = vi.fn();
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0, y: 0, width: 1, height: 1 }}
        lockedPresetId={null}
        onCropChange={onCropChange}
        onUnlockRatio={() => {}}
      />,
    );

    expect(screen.getByRole("button", { name: "Original, 1920×1080" }))
      .toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", {
      name: "Aspect ratio 9:16, 608×1080",
    }));

    expect(onCropChange).toHaveBeenCalledWith(
      getAspectRatioCrop(1920, 1080, 9, 16),
      "portrait-9-16",
    );
  });

  it("reflects a manually resized crop as custom", () => {
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0.1, y: 0.1, width: 0.45, height: 0.7 }}
        lockedPresetId={null}
        onCropChange={() => {}}
        onUnlockRatio={() => {}}
      />,
    );

    expect(screen.getByText("Custom")).toBeInTheDocument();
    expect(screen.getByText("864 × 756")).toBeInTheDocument();
  });

  it("shows common ratios first and progressively reveals the full catalog", () => {
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0, y: 0, width: 1, height: 1 }}
        lockedPresetId={null}
        onCropChange={() => {}}
        onUnlockRatio={() => {}}
      />,
    );

    const commonGroup = screen.getByRole("region", { name: "Common" });
    expect(within(commonGroup).getAllByRole("button")).toHaveLength(6);
    expect(screen.queryByRole("button", { name: /^Aspect ratio 2\.39:1, / }))
      .not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Aspect ratio / })).toHaveLength(6);

    fireEvent.click(screen.getByRole("button", { name: "More ratios, 13" }));

    expect(screen.getAllByRole("button", { name: /^Aspect ratio / }))
      .toHaveLength(CROP_ASPECT_RATIO_PRESETS.length);
    expect(screen.getByRole("region", { name: "Landscape" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Portrait" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Fewer ratios, 13" }));
    expect(screen.getAllByRole("button", { name: /^Aspect ratio / })).toHaveLength(6);
  });

  it("applies an added cinema ratio as an explicit locked preset", () => {
    const onCropChange = vi.fn();
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={{ x: 0, y: 0, width: 1, height: 1 }}
        lockedPresetId={null}
        onCropChange={onCropChange}
        onUnlockRatio={() => {}}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "More ratios, 13" }));
    fireEvent.click(screen.getByRole("button", {
      name: /^Aspect ratio 2\.39:1, /,
    }));

    expect(onCropChange).toHaveBeenCalledWith(
      getAspectRatioCrop(1920, 1080, 239, 100),
      "cinema-239-100",
    );
  });

  it("reveals a specialist ratio when reopening an existing crop", async () => {
    const crop = getAspectRatioCrop(1920, 1080, 239, 100);
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={crop}
        lockedPresetId="cinema-239-100"
        onCropChange={() => {}}
        onUnlockRatio={() => {}}
      />,
    );

    expect(await screen.findByRole("button", { name: /^Aspect ratio 2\.39:1, / }))
      .toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Fewer ratios, 13" }))
      .toHaveAttribute("aria-expanded", "true");
  });

  it("keeps a selected preset explicit until Free unlocks it", () => {
    const onUnlockRatio = vi.fn();
    const crop = getAspectRatioCrop(1920, 1080, 9, 16);
    render(
      <CropRatioPanel
        sourceWidth={1920}
        sourceHeight={1080}
        crop={crop}
        lockedPresetId="portrait-9-16"
        onCropChange={() => {}}
        onUnlockRatio={onUnlockRatio}
      />,
    );

    expect(screen.getByRole("button", { name: "Aspect ratio 9:16, 608×1080" }))
      .toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Unlock aspect ratio" }));
    expect(onUnlockRatio).toHaveBeenCalledOnce();
  });
});
