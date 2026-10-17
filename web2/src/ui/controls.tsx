import type { ReactNode, SelectHTMLAttributes } from "react";
import { cx } from "./cx";
import { Icon, type IconName } from "./Icon";

export interface CounterProps {
  value: number;
  /** The least value. 0 when it is not given. */
  min?: number;
  max: number;
  /** Names the thing counted, for assistive technology: "Carrier from Jord". */
  label: string;
  /** The number that is shown, when one step is more than one: fighters come two at a time. */
  shown?: number;
  onChange: (value: number) => void;
}

export function Counter({ value, min = 0, max, label, shown = value, onChange }: CounterProps) {
  const button =
    "size-7 text-lg text-muted enabled:hover:bg-raised enabled:hover:text-text disabled:opacity-30 touch:size-10";
  return (
    <div className="inline-flex items-center rounded-md border border-line bg-canvas">
      <button
        type="button"
        className={button}
        aria-label={`Remove ${label}`}
        disabled={value <= min}
        onClick={() => onChange(value - 1)}
      >
        −
      </button>
      <output className="min-w-7 text-center tabular-nums" aria-label={`${label} quantity`}>
        {shown}
      </output>
      <button
        type="button"
        className={button}
        aria-label={`Add ${label}`}
        disabled={value >= max}
        onClick={() => onChange(value + 1)}
      >
        +
      </button>
    </div>
  );
}

export interface SegmentedProps<T extends string> {
  label: string;
  /** With an icon, the option shows the icon alone; the label is its name and its tooltip. */
  options: readonly { id: T; label: string; icon?: IconName }[];
  value: T | null;
  onChange: (id: T) => void;
  className?: string;
}

export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  className,
}: SegmentedProps<T>) {
  return (
    <div
      role="group"
      aria-label={label}
      className={cx("inline-flex overflow-hidden rounded-md border border-line", className)}
    >
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={option.id === value}
          aria-label={option.icon && option.label}
          title={option.icon && option.label}
          onClick={() => onChange(option.id)}
          className="px-[9px] py-[5px] text-xs whitespace-nowrap text-muted aria-pressed:bg-raised aria-pressed:text-accent touch:px-3 touch:py-2.5"
        >
          {option.icon ? <Icon name={option.icon} /> : option.label}
        </button>
      ))}
    </div>
  );
}

/** Pill-shaped tabs for the rolls of one battle. */
export function ChipTabs<T extends string>({ label, options, value, onChange }: SegmentedProps<T>) {
  if (options.length < 2) {
    return null;
  }
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={option.id === value}
          onClick={() => onChange(option.id)}
          className="rounded-full border border-line px-[11px] py-[3px] text-xs font-semibold text-muted aria-pressed:border-line-strong aria-pressed:bg-raised aria-pressed:text-text touch:px-3.5 touch:py-2"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Gauge({ label, used, total }: { label: string; used: number; total: number }) {
  const over = used > total;
  const width = Math.min(100, total ? (used / total) * 100 : used ? 100 : 0);
  return (
    <div>
      <div className="flex justify-between gap-2 text-xs text-muted">
        <span>{label}</span>
        <strong
          className={cx("font-mid whitespace-nowrap tabular-nums", over ? "text-red" : "text-text")}
        >
          {used} / {total}
        </strong>
      </div>
      <div className="mt-[5px] h-1 overflow-hidden rounded-full bg-line">
        <div
          className={cx("h-full rounded-full", over ? "bg-red" : "bg-cyan")}
          style={{ width: `${width}%` }}
        />
      </div>
    </div>
  );
}

export function Gauges({ children }: { children: ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-x-4 gap-y-3">
      {children}
    </div>
  );
}

export function Select({ className, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cx(
        "max-w-full rounded-md border border-line bg-surface px-2 py-1 text-xs touch:py-2",
        className,
      )}
      {...rest}
    />
  );
}
