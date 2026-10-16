import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./cx";
import { Eyebrow } from "./Badge";

export function Card({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cx("rounded-lg border border-line bg-white/[.008]", className)} {...rest} />
  );
}

export function CardHeading({
  title,
  bad,
  children,
}: {
  title: ReactNode;
  bad?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-2.5 gap-y-1 border-b border-line px-3.5 py-1.5 phone:px-2.5">
      <h3 className="text-base font-semibold">{title}</h3>
      {children != null && (
        <span className={cx("text-xs", bad ? "text-red" : "text-muted")}>{children}</span>
      )}
    </div>
  );
}

export function CardBody({ className, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cx("space-y-2.5 px-3.5 py-3", className)} {...rest} />;
}

/** A quiet line inside a card: an empty state or a read-only summary. */
export function CardText({ children }: { children: ReactNode }) {
  return <div className="px-3.5 py-3 text-xs text-muted">{children}</div>;
}

export function SummaryGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3">{children}</div>
  );
}

/** One fact of the game state with its label. It does not hold rule text. */
export function SummaryStat({
  eyebrow,
  title,
  children,
}: {
  eyebrow: string;
  title: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="rounded border border-line px-3.5 py-2">
      <Eyebrow className="text-faint">{eyebrow}</Eyebrow>
      <strong className="mt-[3px] block text-md font-medium">{title}</strong>
      {children != null && <p className="text-xs text-muted">{children}</p>}
    </div>
  );
}

export type NoteTone = "accent" | "error" | "success" | "quiet";

const NOTE: Record<NoteTone, string> = {
  accent: "border-accent bg-gold/[.024] text-muted",
  error: "border-red bg-red/[.027] text-red",
  success: "border-green bg-green/[.024] text-muted",
  quiet: "border-line text-muted",
};

/** A short remark attached to the content above it. `strong` leads the sentence. */
export function InlineNote({
  tone = "accent",
  strong,
  children,
}: {
  tone?: NoteTone;
  strong?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className={cx("rounded-r-[5px] border-l-2 px-3 py-2 text-sm", NOTE[tone])}>
      {strong != null && <strong className="font-mid text-text">{strong} </strong>}
      {children}
    </div>
  );
}

/** A decision the player may take now: an action card, a retreat. `hint` holds the rule text. */
export function Offer({
  eyebrow,
  title,
  hint,
  actions,
}: {
  eyebrow: string;
  title: string;
  hint?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-lg border border-gold/25 bg-gold/[.03] px-3.5 py-1.5">
      <div className="flex flex-wrap items-center gap-x-2.5">
        <Eyebrow className="text-gold">{eyebrow}</Eyebrow>
        <h3 className="text-[15px] font-semibold">{title}</h3>
        {hint}
      </div>
      {actions != null && <div className="flex flex-wrap gap-1.5">{actions}</div>}
    </div>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h3 className="mt-[18px]! text-muted">
      <Eyebrow>{children}</Eyebrow>
    </h3>
  );
}
