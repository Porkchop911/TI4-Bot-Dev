import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./cx";
import { Icon, type IconName } from "./Icon";

export interface ListRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  icon?: IconName;
  /** Replaces the icon, for example with an order number. */
  lead?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  /** Shown under the subtitle, for example a route. */
  extra?: ReactNode;
  controls?: ReactNode;
  invalid?: boolean;
  linked?: boolean;
  /**
   * On a phone too the controls stay at the end of the line (one compact button such as
   * "Change"): the text wraps instead.
   */
  inline?: boolean;
}

/**
 * One thing in a list with its controls on the right: a unit line, an option, a player.
 * It is one line: the panel is wide at the design size, and a short row keeps the step free of scroll.
 */
export function ListRow({
  icon,
  lead,
  title,
  subtitle,
  extra,
  controls,
  invalid,
  linked,
  inline,
  className,
  ...rest
}: ListRowProps) {
  return (
    <div
      className={cx(
        // On a phone the controls go under a text that has no room next to them, unless they are
        // one compact button.
        "row flex min-h-9 items-center justify-between gap-3 px-3.5 py-0.5 phone:gap-y-1 phone:px-2.5 phone:py-1.5 [.row+&]:border-t [.row+&]:border-line/50",
        !inline && "phone:flex-wrap",
        invalid && "bg-red/[.027]",
        linked && "bg-cyan/[.06]",
        className,
      )}
      {...rest}
    >
      <div className="flex min-w-0 items-center gap-2.5 text-base">
        {lead ?? (icon && <Icon name={icon} className="size-5 shrink-0 text-icon" />)}
        <div className="flex min-w-0 flex-wrap items-center gap-x-2.5">
          <strong className={cx("font-mid", invalid && "text-red")}>{title}</strong>
          {subtitle != null && (
            <small className={cx("text-xs", invalid ? "text-red" : "text-muted")}>{subtitle}</small>
          )}
          {extra}
        </div>
      </div>
      {controls != null && (
        <div className="flex shrink-0 items-center gap-2 phone:ml-auto">{controls}</div>
      )}
    </div>
  );
}
