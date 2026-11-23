import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ColorPicker } from "@/components/ui/ColorPicker";
import en from "@/i18n/en";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

function renderPicker() {
  const onOpen = vi.fn();
  const onClose = vi.fn();
  render(
    <ColorPicker value="#112233" onChange={vi.fn()} onOpen={onOpen} onClose={onClose} label="Fill" />,
  );
  const open = () => fireEvent.click(screen.getByRole("button", { name: "Fill" }));
  return { onOpen, onClose, open };
}

// Each close path must end the history batch the open started exactly once.
describe("ColorPicker close paths", () => {
  it("closes once on Enter in the hex input", () => {
    const { onOpen, onClose, open } = renderPicker();
    open();
    expect(onOpen).toHaveBeenCalledOnce();
    fireEvent.keyDown(screen.getByRole("textbox", { name: en.hexColor }), { key: "Enter" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes once on Escape", () => {
    const { onClose, open } = renderPicker();
    open();
    fireEvent.keyDown(screen.getByRole("textbox", { name: en.hexColor }), { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes once when the trigger is toggled", () => {
    const { onOpen, onClose, open } = renderPicker();
    open();
    open();
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes once on an outside pointer press", async () => {
    const { onClose, open } = renderPicker();
    open();
    // Radix arms its outside-press listener on the next task after opening.
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    fireEvent.pointerDown(document.body);
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
