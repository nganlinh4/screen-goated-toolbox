import { act, fireEvent, render, screen } from "@testing-library/react";
import { SettingRow } from "@/components/layout/SettingRow";
import { CustomAudioPlayer } from "@/components/dialogs/MediaResultPlayers";
import { ColorPicker } from "@/components/ui/ColorPicker";
import { PanelSelect } from "@/components/ui/PanelSelect";
import { Slider } from "@/components/ui/Slider";
import { Switch } from "@/components/ui/Switch";
import { Checkbox } from "@/components/ui/checkbox";
import { UserNotificationRegion } from "@/components/UserNotificationRegion";
import { notifyUserError } from "@/lib/userNotifications";
import { SidePanelTabs } from "@/components/sidepanel/SidePanelTabs";

describe("shared control accessibility contract", () => {
  it("associates SettingRow labels with every shared field primitive", () => {
    render(
      <>
        <SettingRow label="Scale">
          <Slider min={0} max={100} value={50} onChange={() => undefined} />
        </SettingRow>
        <SettingRow label="Mirror">
          <Switch checked={false} onCheckedChange={() => undefined} />
        </SettingRow>
        <SettingRow label="Enabled">
          <Checkbox checked={false} readOnly />
        </SettingRow>
        <SettingRow label="Mode">
          <PanelSelect
            value="auto"
            options={[{ value: "auto", label: "Automatic" }]}
            onChange={() => undefined}
          />
        </SettingRow>
      </>,
    );

    expect(screen.getByRole("slider", { name: "Scale" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Mirror" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Enabled" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mode" })).toBeInTheDocument();
  });

  it("exposes color and media controls to keyboard and assistive technology", () => {
    render(
      <>
        <ColorPicker label="Text color" value="#ffffff" onChange={() => undefined} />
        <CustomAudioPlayer src="audio.wav" onReady={() => undefined} />
      </>,
    );

    expect(screen.getByRole("button", { name: "Text color" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Seek media" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mute" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Text color" }));
    expect(screen.getByRole("slider", { name: "Color saturation" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Color brightness" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Color hue" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Hex color" })).toBeInTheDocument();
  });

  it("announces recoverable async failures and provides a named dismiss action", () => {
    render(<UserNotificationRegion />);

    act(() => notifyUserError("copyMediaFailed"));

    expect(screen.getByRole("alert")).toHaveTextContent("Could not copy this media file.");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("uses roving tab semantics and arrow-key panel navigation", () => {
    const onPanelChange = vi.fn();
    render(
      <SidePanelTabs
        activePanel="background"
        onPanelChange={onPanelChange}
        hiddenTabs={new Set(["zoom", "camera"])}
      />,
    );

    const backgroundTab = screen.getByRole("tab", { name: "Background" });
    expect(backgroundTab).toHaveAttribute("aria-selected", "true");
    expect(backgroundTab).toHaveAttribute("tabindex", "0");
    fireEvent.keyDown(backgroundTab, { key: "ArrowRight" });
    expect(onPanelChange).toHaveBeenCalledWith("cursor");
  });
});
