import { describe, expect, it, afterEach } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { App } from "../src/App";

type Pending = { cmd: string; args: Record<string, any>; resolve: (value: unknown) => void };

/**
 * Mimics the WRY host for the main draft: set_draft_text is handled in order
 * and replied to without an echo, while unrelated host events (a generation
 * finishing, playback ending) push the whole state at any time.
 */
function installHost() {
  const queue: Pending[] = [];
  const rust = { mode: "TtsClone", draftText: "", player: { isGenerating: true } };
  (window as any).invoke = (cmd: string, args: Record<string, any> = {}) =>
    new Promise((resolve) => queue.push({ cmd, args, resolve }));
  const push = () => act(() => (window as any).__TTS_SET_STATE__({ ...rust, player: { ...rust.player } }));
  const handleNext = async () => {
    const next = queue.shift()!;
    if (next.cmd === "set_draft_text") rust.draftText = next.args.text;
    await act(async () => {
      next.resolve(null);
      await Promise.resolve();
      await Promise.resolve();
    });
  };
  push();
  return { rust, queue, push, handleNext };
}

function type(field: HTMLTextAreaElement, character: string) {
  fireEvent.change(field, { target: { value: field.value + character } });
}

describe("main draft text under unrelated host pushes", () => {
  afterEach(() => {
    cleanup();
    delete (window as any).invoke;
  });

  it("keeps a keystroke the host has not handled when another event pushes state", async () => {
    const host = installHost();
    const { container } = render(<App />);
    const field = container.querySelector<HTMLTextAreaElement>(".tts-text-area")!;

    type(field, "a");
    await host.handleNext();
    type(field, "b");
    // Generation finishes before the host reads "ab"; its push still carries "a".
    host.rust.player.isGenerating = false;
    host.push();
    expect(field.value).toBe("ab");
    type(field, "c");
    while (host.queue.length) await host.handleNext();

    expect(host.rust.draftText).toBe("abc");
    expect(field.value).toBe("abc");
  });

  it("adopts a host-side draft once no keystroke is in flight", async () => {
    const host = installHost();
    const { container } = render(<App />);
    const field = container.querySelector<HTMLTextAreaElement>(".tts-text-area")!;
    type(field, "x");
    while (host.queue.length) await host.handleNext();

    host.rust.draftText = "restored draft";
    host.push();
    expect(field.value).toBe("restored draft");
  });
});
