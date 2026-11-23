import { useEffect, useReducer, useState } from "react";
import { DragHold } from "../../ui-shared/drag-hold";

/**
 * React binding for DragHold: the value a slider should show, plus handlers
 * that hold it while the user drags and hand control back to the host after.
 */
export function useDragHold(hostValue: number) {
  const [hold] = useState(() => new DragHold<number>());
  const [, rerender] = useReducer((count: number) => count + 1, 0);
  const release = () => {
    if (!hold.active) return;
    hold.release();
    rerender();
  };
  const active = hold.active;
  useEffect(() => {
    if (!active) return;
    // A drag can end outside the slider, where its own pointerup never fires.
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      window.removeEventListener("blur", release);
    };
  });
  return {
    value: hold.present(hostValue),
    hold: (value: number) => {
      hold.hold(value);
      rerender();
    },
    releaseHandlers: { onPointerUp: release, onKeyUp: release, onBlur: release },
  };
}
