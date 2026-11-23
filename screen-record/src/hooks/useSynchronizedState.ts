import { useCallback, useRef, useState, type Dispatch, type SetStateAction } from "react";

/** Keep imperative editor handoffs current even when React batches renders. */
export function useSynchronizedState<T>(initial: () => T) {
  const [value, setValue] = useState(initial);
  const valueRef = useRef(value);
  const setSynchronizedValue = useCallback<Dispatch<SetStateAction<T>>>((update) => {
    const next = typeof update === "function"
      ? (update as (previous: T) => T)(valueRef.current)
      : update;
    valueRef.current = next;
    setValue(() => next);
  }, []);
  return [value, setSynchronizedValue, valueRef] as const;
}
