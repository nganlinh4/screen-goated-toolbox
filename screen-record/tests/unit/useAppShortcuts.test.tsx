import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useAppShortcuts, type UseAppShortcutsParams } from "@/hooks/useAppShortcuts";
import type { VideoSegment } from "@/types/video";

function params(overrides: Partial<UseAppShortcutsParams> = {}): UseAppShortcutsParams {
  return {
    togglePlayPause: vi.fn(), currentTime: 10, duration: 60, seek: vi.fn(), isCropping: false,
    editingKeyframeId: null, editingTextId: null, editingSubtitleId: null,
    editingKeystrokeSegmentId: null, editingPointerId: null, segment: null,
    setSegment: vi.fn(), setEditingKeyframeId: vi.fn(), handleDeleteText: vi.fn(),
    handleDeleteSubtitle: vi.fn(), handleDeleteKeystrokeSegment: vi.fn(),
    handleDeletePointerSegment: vi.fn(), canUndo: true, canRedo: true, undo: vi.fn(), redo: vi.fn(),
    setSeekIndicatorKey: vi.fn(), setSeekIndicatorDir: vi.fn(), ...overrides,
  };
}

function press(target: EventTarget, init: KeyboardEventInit) {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

function mount(element: HTMLElement) {
  document.body.appendChild(element);
  return element;
}

describe("useAppShortcuts", () => {
  afterEach(() => { document.body.innerHTML = ""; });

  it("leaves undo and redo to a focused text field", () => {
    const p = params();
    renderHook(() => useAppShortcuts(p));
    const textarea = mount(document.createElement("textarea"));
    const undoEvent = press(textarea, { code: "KeyZ", ctrlKey: true });
    press(textarea, { code: "KeyY", ctrlKey: true });
    expect(p.undo).not.toHaveBeenCalled();
    expect(p.redo).not.toHaveBeenCalled();
    expect(undoEvent.defaultPrevented).toBe(false);
    press(document.body, { code: "KeyZ", ctrlKey: true });
    expect(p.undo).toHaveBeenCalledOnce();
  });

  it("does not undo the timeline while a modal dialog is open", () => {
    const p = params({ isModalOpen: true });
    renderHook(() => useAppShortcuts(p));
    press(document.body, { code: "KeyZ", ctrlKey: true });
    expect(p.undo).not.toHaveBeenCalled();
  });

  it("lets focused sliders and selects keep their arrow keys", () => {
    const p = params();
    renderHook(() => useAppShortcuts(p));
    const slider = mount(document.createElement("input"));
    slider.type = "range";
    const select = mount(document.createElement("select"));
    const sliderEvent = press(slider, { code: "ArrowRight" });
    press(select, { code: "ArrowLeft" });
    expect(p.seek).not.toHaveBeenCalled();
    expect(sliderEvent.defaultPrevented).toBe(false);
    press(document.body, { code: "ArrowRight" });
    expect(p.seek).toHaveBeenCalledWith(15);
  });

  it.each([0, 1])("deletes the selected zoom block %i rather than the selected text", (index) => {
    const segment = { zoomBlocks: [{ id: "a" }, { id: "b" }] } as unknown as VideoSegment;
    const p = params({ editingTextId: "text-1", editingKeyframeId: index, segment });
    renderHook(() => useAppShortcuts(p));
    press(document.body, { code: "Delete" });
    expect(p.handleDeleteText).not.toHaveBeenCalled();
    expect(p.setSegment).toHaveBeenCalledWith({
      zoomBlocks: segment.zoomBlocks!.filter((_, i) => i !== index),
    });
  });

  it("deletes the selected text when no zoom block is selected", () => {
    const p = params({ editingTextId: "text-1" });
    renderHook(() => useAppShortcuts(p));
    press(document.body, { code: "Backspace" });
    expect(p.handleDeleteText).toHaveBeenCalledOnce();
  });
});
