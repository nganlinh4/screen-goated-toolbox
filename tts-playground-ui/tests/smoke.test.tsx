import { describe, expect, it, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../src/App";

describe("TTS Playground UI smoke", () => {
  beforeEach(() => {
    cleanup();
    // Reset to fallback state for each test
    delete (window as any).__TTS_INITIAL_STATE__;
  });

  it("renders the header title", () => {
    render(<App />);
    // FALLBACK has strings.title = "TTS Playground"
    expect(screen.getByText("TTS Playground")).toBeInTheDocument();
  });

  it("renders the four mode tabs", () => {
    render(<App />);
    expect(screen.getByText("S2S")).toBeInTheDocument();
    expect(screen.getByText("TTS / Clone")).toBeInTheDocument();
    expect(screen.getByText("Audio Edit")).toBeInTheDocument();
    expect(screen.getByText("Reference Library")).toBeInTheDocument();
  });

  it("shows the method picker dropdown in TtsClone mode", () => {
    render(<App />);
    // Default mode is TtsClone, so the Method label should be visible
    expect(screen.getByText("Method")).toBeInTheDocument();
  });

  it("shows the text input and generate button", () => {
    render(<App />);
    expect(screen.getByText("Text")).toBeInTheDocument();
    expect(screen.getByText("Generate")).toBeInTheDocument();
    expect(screen.getByText("Clear")).toBeInTheDocument();
  });

  it("shows the 'no audio' message when no current clip", () => {
    render(<App />);
    expect(
      screen.getByText("No audio yet — generate one above."),
    ).toBeInTheDocument();
  });
});
