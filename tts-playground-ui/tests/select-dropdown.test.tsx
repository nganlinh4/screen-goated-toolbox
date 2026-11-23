import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { Select } from "../src/components";

const options = Array.from({ length: 30 }, (_, index) => ({
  value: `voice-${index}`,
  label: `Voice ${index}`,
}));

function openSelect() {
  const onChange = vi.fn();
  render(<Select value="voice-25" options={options} onChange={onChange} />);
  fireEvent.click(screen.getByRole("button", { name: "Voice 25" }));
  const list = document.querySelector<HTMLElement>(".tts-select-list");
  expect(list).not.toBeNull();
  return { list: list!, onChange };
}

describe("Select dropdown", () => {
  afterEach(() => cleanup());

  it("stays open while its own long option list scrolls", () => {
    const { list, onChange } = openSelect();
    act(() => {
      fireEvent.scroll(list);
    });
    const option = screen.getByRole("button", { name: "Voice 29" });
    fireEvent.click(option);
    expect(onChange).toHaveBeenCalledWith("voice-29");
  });

  it("still closes when the page behind it scrolls", () => {
    openSelect();
    act(() => {
      fireEvent.scroll(window);
    });
    expect(document.querySelector(".tts-select-list")).toBeNull();
  });
});
