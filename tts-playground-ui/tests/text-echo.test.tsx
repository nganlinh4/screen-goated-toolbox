import { describe, expect, it, afterEach } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { App } from "../src/App";

type Pending = { cmd: string; args: Record<string, any>; resolve: (value: unknown) => void };

/**
 * Mimics the WRY host: each command is handled in order, replies first, and
 * then (for every command except set_draft_text) echoes the whole state.
 */
function installHost() {
  const queue: Pending[] = [];
  const rust = { mode: "TtsClone", method: "GeminiLive", gemini: { model: "", voice: "", speed: "Normal", instruction: "", conditions: [] } };
  (window as any).invoke = (cmd: string, args: Record<string, any> = {}) =>
    new Promise((resolve) => queue.push({ cmd, args, resolve }));
  const handleNext = async () => {
    const next = queue.shift()!;
    if (next.cmd === "patch_gemini") Object.assign(rust.gemini, next.args.patch);
    await act(async () => {
      next.resolve(null);
      await Promise.resolve();
      await Promise.resolve();
    });
    act(() => (window as any).__TTS_SET_STATE__({ ...rust, gemini: { ...rust.gemini } }));
  };
  act(() => (window as any).__TTS_SET_STATE__({ ...rust, gemini: { ...rust.gemini } }));
  return { rust, queue, handleNext };
}

function type(field: HTMLTextAreaElement, character: string) {
  fireEvent.change(field, { target: { value: field.value + character } });
}

describe("text fields backed by echoed host state", () => {
  afterEach(() => {
    cleanup();
    delete (window as any).invoke;
  });

  it("keeps every typed character when replies and echoes lag behind typing", async () => {
    const host = installHost();
    const { container } = render(<App />);
    const field = container.querySelector<HTMLTextAreaElement>(".tts-gemini-instruction")!;

    type(field, "a");
    type(field, "b");
    await host.handleNext(); // echo of "a" arrives while "ab" is still in flight
    type(field, "c");
    while (host.queue.length) await host.handleNext();

    expect(host.rust.gemini.instruction).toBe("abc");
    expect(field.value).toBe("abc");
  });

  it("still accepts a host-side change once no edit is in flight", async () => {
    const host = installHost();
    const { container } = render(<App />);
    const field = container.querySelector<HTMLTextAreaElement>(".tts-gemini-instruction")!;
    type(field, "x");
    while (host.queue.length) await host.handleNext();

    host.rust.gemini.instruction = "restored instruction";
    act(() => (window as any).__TTS_SET_STATE__({ ...host.rust, gemini: { ...host.rust.gemini } }));
    expect(field.value).toBe("restored instruction");
  });
});
