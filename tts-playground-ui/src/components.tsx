import clsx from "clsx";
import { useTtsState } from "./state";
import type { CatalogOption } from "./types";

export function Card({
  title,
  children,
  description,
  action,
}: {
  title?: string;
  children: React.ReactNode;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2.5 rounded-lg border border-border bg-surface-soft p-3 shadow-sm">
      {title && (
        <div className="flex items-baseline gap-2">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-fg">
            {title}
          </h3>
          {description && (
            <p className="flex-1 truncate text-[11px] text-muted">
              {description}
            </p>
          )}
          {action}
        </div>
      )}
      {!title && description && (
        <p className="text-[11px] text-muted">{description}</p>
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
    <div className="grid grid-cols-[88px,1fr] items-center gap-3">
      <label className="text-xs text-muted">{label}</label>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function Select<T extends string>({
  value,
  options,
  onChange,
  placeholder,
}: {
  value: T;
  options: CatalogOption<T>[];
  onChange: (v: T) => void;
  placeholder?: string;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className="w-full rounded-md border border-border bg-surface px-2 py-1 text-xs focus:border-accent focus:outline-none"
    >
      {placeholder && <option value="">{placeholder}</option>}
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
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
    <button
      type="button"
      onClick={onClick}
      className="rounded-md border border-border bg-surface px-2 py-1 text-[11px] text-muted hover:border-border-strong hover:text-fg"
    >
      {children}
    </button>
  );
}

export function NumberRange({
  value,
  min,
  max,
  step,
  suffix,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  suffix?: string;
  onChange: (v: number) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="seek-bar flex-1"
      />
      <span className="font-mono text-[11px] tabular-nums text-muted">
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
    <div className="flex gap-1">
      {opts.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={clsx(
            "flex-1 rounded-md border px-2 py-1 text-xs transition-colors",
            value === o.id
              ? "border-accent bg-accent/15 text-fg"
              : "border-border bg-surface text-muted hover:border-border-strong hover:text-fg",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
