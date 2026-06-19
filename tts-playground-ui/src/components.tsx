import clsx from "clsx";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTtsState } from "./state";
import type { CatalogOption } from "./types";

// NOTE: every element carries a semantic `tts-*` className (kebab-case) for
// DevTools debugging, in addition to the Tailwind utility classes. The `tts-*`
// class is always FIRST in the string so it's easy to spot in the inspector.

export function Card({
  title,
  children,
  description,
  action,
  className,
}: {
  title?: string;
  children: React.ReactNode;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={clsx(
        "tts-card flex flex-col gap-2.5 rounded-lg bg-surface-soft p-3.5 shadow-elevation-2",
        className,
      )}
    >
      {title && (
        <div className="tts-card-header flex items-baseline gap-2">
          <h3 className="tts-card-title text-md font-semibold text-fg">
            {title}
          </h3>
          {description && (
            <p className="tts-card-desc flex-1 truncate text-xs text-muted">
              {description}
            </p>
          )}
          {action}
        </div>
      )}
      {!title && description && (
        <p className="tts-card-desc text-xs text-muted">{description}</p>
      )}
      {children}
    </div>
  );
}

export function FormRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="tts-form-row grid grid-cols-[104px_minmax(0,1fr)] items-center gap-3">
      <label className="tts-form-row-label self-center text-xs font-medium leading-tight text-muted">
        {label}
      </label>
      <div className="tts-form-row-control min-w-0">{children}</div>
    </div>
  );
}

// ----------------------------------------------------------------------------
// Button — single source of button hierarchy. Variant classes are written as
// FULL literal strings so Tailwind's JIT keeps them (no runtime concatenation
// of shadow-elevation-*/color utilities).
// ----------------------------------------------------------------------------

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
type ButtonSize = "sm" | "md";

const BUTTON_BASE =
  "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition ease-spring focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 disabled:cursor-not-allowed disabled:opacity-50";

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-accent-fg shadow-elevation-1 hover:brightness-105 active:brightness-95",
  secondary:
    "border border-border bg-surface-soft text-fg hover:border-border-strong hover:bg-surface-strong",
  ghost: "text-muted hover:bg-surface-strong hover:text-fg",
  danger:
    "border border-danger/40 text-danger hover:bg-danger/10 hover:border-danger/60",
};

const BUTTON_SIZES: Record<ButtonSize, string> = {
  sm: "px-2.5 py-1 text-xs",
  md: "px-3.5 py-1.5 text-sm",
};

export function Button({
  children,
  onClick,
  variant = "secondary",
  size = "sm",
  disabled,
  className,
  type = "button",
  title,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  variant?: ButtonVariant;
  size?: ButtonSize;
  disabled?: boolean;
  className?: string;
  type?: "button" | "submit";
  title?: string;
}) {
  return (
    <button
      type={type}
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={clsx(
        `tts-btn tts-btn-${variant}`,
        BUTTON_BASE,
        BUTTON_VARIANTS[variant],
        BUTTON_SIZES[size],
        className,
      )}
    >
      {children}
    </button>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      className={clsx(
        "tts-select-chevron h-3 w-3 shrink-0 text-muted transition-transform duration-150",
        open && "rotate-180",
      )}
      fill="currentColor"
    >
      <path d="m12 15.375l-6-6l1.4-1.4l4.6 4.6l4.6-4.6l1.4 1.4z" />
    </svg>
  );
}

/**
 * Custom dropdown — replaces the native <select>. The option list renders in a
 * portal to <body> with fixed positioning so it is never clipped by a scrolling
 * panel, and flips above the trigger when there isn't room below.
 */
export function Select<T extends string>({
  value,
  options,
  onChange,
  placeholder,
  className,
}: {
  value: T;
  options: CatalogOption<T>[];
  onChange: (v: T) => void;
  placeholder?: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{
    left: number;
    width: number;
    rectTop: number;
    rectBottom: number;
    above: boolean;
  } | null>(null);

  const selected = options.find((o) => o.value === value);
  const isPlaceholder = !selected;
  const displayLabel = selected?.label ?? placeholder ?? "—";
  // A placeholder choice (value "") lets the user clear back to "none".
  const items: CatalogOption<string>[] = placeholder
    ? [{ value: "", label: placeholder }, ...options]
    : options;

  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const r = triggerRef.current.getBoundingClientRect();
    const estimated = Math.min(248, items.length * 30 + 8);
    const below = window.innerHeight - r.bottom;
    setPos({
      left: r.left,
      width: r.width,
      rectTop: r.top,
      rectBottom: r.bottom,
      above: below < estimated && r.top > below,
    });
  }, [open, items.length]);

  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(false);
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (triggerRef.current?.contains(t) || listRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close, true);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close, true);
    };
  }, [open]);

  // Bring the selected row into view when the list opens.
  useEffect(() => {
    if (!open || !listRef.current) return;
    listRef.current
      .querySelector('[data-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [open]);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={clsx(
          "tts-select-trigger flex w-full items-center justify-between gap-2 rounded-md bg-surface px-2.5 py-1.5 text-left text-sm text-fg transition ease-spring hover:bg-surface-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
          className,
        )}
      >
        <span
          className={clsx(
            "tts-select-value truncate",
            isPlaceholder && "text-muted",
          )}
        >
          {displayLabel}
        </span>
        <Chevron open={open} />
      </button>
      {open &&
        pos &&
        createPortal(
          <div
            ref={listRef}
            style={{
              position: "fixed",
              left: pos.left,
              width: pos.width,
              maxHeight: 248,
              ...(pos.above
                ? { bottom: window.innerHeight - pos.rectTop + 4 }
                : { top: pos.rectBottom + 4 }),
            }}
            className="tts-select-list z-50 overflow-y-auto rounded-md border border-border bg-surface-soft py-1 shadow-elevation-3"
          >
            {items.map((o) => {
              const active = o.value === value;
              return (
                <button
                  key={o.value || "__empty"}
                  type="button"
                  data-selected={active || undefined}
                  onClick={() => {
                    onChange(o.value as T);
                    setOpen(false);
                  }}
                  className={clsx(
                    "tts-select-option flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm transition",
                    active
                      ? "tts-select-option--active bg-accent-soft/60 font-medium text-fg"
                      : "text-fg hover:bg-surface-strong",
                    !o.value && "text-muted",
                  )}
                >
                  <span className="min-w-0 flex-1 truncate">{o.label}</span>
                  {active && (
                    <svg
                      viewBox="0 0 24 24"
                      className="tts-select-check h-3 w-3 shrink-0 text-accent"
                      fill="currentColor"
                    >
                      <path d="m9.55 18l-5.7-5.7l1.425-1.425L9.55 15.15l9.175-9.175L20.15 7.4z" />
                    </svg>
                  )}
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </>
  );
}

export function SmallButton({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <Button variant="secondary" size="sm" className="tts-small-btn" onClick={onClick}>
      {children}
    </Button>
  );
}

export function NumberRange({
  value,
  min,
  max,
  step,
  suffix,
  onChange,
  name,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
  onChange: (v: number) => void;
  name?: string;
}) {
  return (
    <div
      className={clsx(
        "tts-range flex items-center gap-2.5",
        name && `tts-range-${name}`,
      )}
    >
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="tts-range-input seek-bar flex-1"
      />
      <span className="tts-range-value min-w-[3.25ch] text-right font-mono text-xs tabular-nums text-muted">
        {value}
        {suffix ?? ""}
      </span>
    </div>
  );
}

export function SpeedRadios({
  value,
  onChange,
  strings,
}: {
  value: "Slow" | "Normal" | "Fast";
  onChange: (v: "Slow" | "Normal" | "Fast") => void;
  strings: ReturnType<typeof useTtsState>["strings"];
}) {
  const opts: Array<{ id: "Slow" | "Normal" | "Fast"; label: string }> = [
    { id: "Slow", label: strings.speedSlow },
    { id: "Normal", label: strings.speedNormal },
    { id: "Fast", label: strings.speedFast },
  ];
  return (
    <div className="tts-speed-radios inline-flex w-full rounded-md bg-surface p-0.5">
      {opts.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={clsx(
            "tts-speed-option flex-1 rounded-[6px] px-2 py-1 text-xs font-medium transition ease-spring",
            value === o.id
              ? "tts-speed-option--active bg-accent text-accent-fg shadow-elevation-1"
              : "text-muted hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
