import { useEffect, useRef, type InputHTMLAttributes, type KeyboardEvent, type PointerEvent } from 'react';
import { useFieldLabelId } from '@/components/ui/FieldLabelContext';
import { cn } from '@/lib/utils';

export interface SliderProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'type' | 'min' | 'max' | 'step' | 'value' | 'onChange'
> {
  min: number;
  max: number;
  step?: number;
  value: number;
  onChange: (value: number) => void;
  /** Fires once when a pointer drag or keyboard adjustment begins (e.g. history beginBatch). */
  onGestureStart?: () => void;
  /** Fires once when that gesture ends: pointer release/cancel, key release or blur. */
  onGestureEnd?: () => void;
}

const KEYBOARD_ADJUST_KEYS = new Set([
  'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End',
]);

/**
 * Range slider that manages the `--value-pct` CSS variable used by App.css
 * to paint the active-track fill. Each pointer drag or held-key adjustment is
 * reported as exactly one start/end gesture so callers record a single undo step.
 */
export function Slider({
  min, max, step = 1, value, onChange, className, disabled = false,
  'aria-labelledby': ariaLabelledBy, onGestureStart, onGestureEnd, ...props
}: SliderProps) {
  const labelId = useFieldLabelId(ariaLabelledBy);
  const pct = max === min ? 0 : ((value - min) / (max - min)) * 100;
  const gestureActiveRef = useRef(false);
  const gestureEndRef = useRef(onGestureEnd);
  useEffect(() => {
    gestureEndRef.current = onGestureEnd;
  }, [onGestureEnd]);

  const startGesture = () => {
    if (gestureActiveRef.current) return;
    gestureActiveRef.current = true;
    onGestureStart?.();
  };
  const endGesture = () => {
    if (!gestureActiveRef.current) return;
    gestureActiveRef.current = false;
    gestureEndRef.current?.();
  };
  // A slider unmounted mid-gesture must still close its gesture.
  useEffect(() => () => {
    if (!gestureActiveRef.current) return;
    gestureActiveRef.current = false;
    gestureEndRef.current?.();
  }, []);

  return (
    <input
      type="range"
      min={min}
      max={max}
      step={step}
      value={value}
      style={{ '--value-pct': `${pct}%` } as React.CSSProperties}
      onChange={(e) => onChange(Number(e.target.value))}
      aria-labelledby={labelId}
      disabled={disabled}
      className={cn('flex-1 min-w-0', className)}
      {...props}
      onPointerDown={(event: PointerEvent<HTMLInputElement>) => {
        props.onPointerDown?.(event);
        if (event.button !== 0) return;
        try {
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          // Synthetic or already-released pointers cannot be captured.
        }
        startGesture();
      }}
      onPointerUp={(event: PointerEvent<HTMLInputElement>) => {
        props.onPointerUp?.(event);
        endGesture();
      }}
      onPointerCancel={(event: PointerEvent<HTMLInputElement>) => {
        props.onPointerCancel?.(event);
        endGesture();
      }}
      onKeyDown={(event: KeyboardEvent<HTMLInputElement>) => {
        props.onKeyDown?.(event);
        if (KEYBOARD_ADJUST_KEYS.has(event.key)) startGesture();
      }}
      onKeyUp={(event: KeyboardEvent<HTMLInputElement>) => {
        props.onKeyUp?.(event);
        if (KEYBOARD_ADJUST_KEYS.has(event.key)) endGesture();
      }}
      onBlur={(event) => {
        props.onBlur?.(event);
        endGesture();
      }}
    />
  );
}
