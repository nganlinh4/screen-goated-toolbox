import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { App } from "../src/App";
import { NumberRange } from "../src/components";

/**
 * The host answers each slider step and then pushes its whole state, so an
 * echo of an earlier step (or a playback tick) arrives while the pointer has
 * already moved on.
 */
describe("sliders under host echoes", () => {
  afterEach(() => {
    cleanup();
    act(() => (window as any).__TTS_SET_STATE__({}));
  });

  it("a provider slider stays under the pointer while dragged", () => {
    const view = render(<NumberRange value={10} min={-50} max={50} step={1} onChange={() => {}} />);
    const slider = view.container.querySelector<HTMLInputElement>("input[type=range]")!;
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "30" } });
    view.rerender(<NumberRange value={20} min={-50} max={50} step={1} onChange={() => {}} />);
    expect(slider.value).toBe("30");
    fireEvent.pointerUp(slider);
    view.rerender(<NumberRange value={25} min={-50} max={50} step={1} onChange={() => {}} />);
    expect(slider.value).toBe("25");
  });

  it("the seek bar is not pulled back by playback ticks while dragged", () => {
    const push = (positionSec: number) => act(() => (window as any).__TTS_SET_STATE__({
      player: {
        isPlaying: true,
        positionSec,
        current: { id: "clip", voiceLabel: "Voice", text: "Hello", durationSec: 10 },
      },
    }));
    const view = render(<App />);
    push(2);
    const seek = view.container.querySelector<HTMLInputElement>(".tts-player-seek")!;
    fireEvent.pointerDown(seek);
    fireEvent.change(seek, { target: { value: "7" } });
    push(2.5);
    expect(seek.value).toBe("7");
    fireEvent.pointerUp(seek);
    push(7.1);
    expect(seek.value).toBe("7.1");
  });
});
