import { cx } from "./cx";

export type TrailStatus = "complete" | "active" | "todo" | "skipped";
export interface TrailItem {
  label: string;
  status: TrailStatus;
  /** Why a substep is skipped. */
  reason?: string;
}

const TRAIL: Record<TrailStatus, string> = {
  complete: "text-green border-green/15",
  active: "text-accent bg-gold/[.03] border-gold/20",
  todo: "text-faint border-line",
  skipped: "text-faint border-line border-dashed",
};

/** The substeps of a step, in order. */
export function Trail({ items }: { items: TrailItem[] }) {
  if (!items.length) {
    return null;
  }
  return (
    <div role="group" className="flex flex-wrap items-center gap-[5px]" aria-label="Substeps">
      {items.map((item) => (
        <span
          key={item.label}
          className={cx("rounded-sm border px-[7px] py-px text-xs", TRAIL[item.status])}
        >
          {item.status === "complete" && "✓ "}
          {item.label}
          {item.status === "skipped" && item.reason ? ` · ${item.reason}` : ""}
        </span>
      ))}
    </div>
  );
}

/** One die. Without a roll it is a slot for a die that is not rolled yet. */
export function Die({ roll, hit }: { roll?: number; hit?: boolean }) {
  const base = "inline-grid size-6 place-items-center rounded-[5px] border text-xs tabular-nums";
  if (roll === undefined) {
    return <span aria-hidden="true" className={cx(base, "border-dashed border-line")} />;
  }
  return (
    <span
      role="img"
      aria-label={`Rolled ${roll}${hit === undefined ? "" : hit ? ": hit" : ": miss"}`}
      className={cx(
        base,
        hit
          ? "border-green/40 bg-green/[.07] font-bold text-green"
          : "border-line bg-canvas text-muted",
      )}
    >
      {roll}
    </span>
  );
}
