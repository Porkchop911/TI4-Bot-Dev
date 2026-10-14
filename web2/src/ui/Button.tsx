import type { ButtonHTMLAttributes } from "react";
import { cx } from "./cx";
import { Icon, type IconName } from "./Icon";

export type ButtonTone = "default" | "primary" | "quiet" | "demo";
export type ButtonSize = "md" | "sm" | "icon";

const HOVER = "enabled:hover:bg-hover enabled:hover:border-line-strong";
const TONES: Record<ButtonTone, string> = {
  default: `border-line bg-raised ${HOVER}`,
  primary:
    "border-accent bg-accent text-[#171d27] enabled:hover:bg-[color-mix(in_srgb,var(--accent),white_15%)] enabled:hover:border-[color-mix(in_srgb,var(--accent),white_15%)]",
  quiet: `border-line bg-transparent ${HOVER}`,
  demo: `border-dashed border-line bg-transparent text-muted ${HOVER}`,
};
const SIZES: Record<ButtonSize, string> = {
  md: "min-h-9 px-3.5 py-2 text-sm touch:min-h-11",
  sm: "min-h-7 px-[9px] py-[3px] text-xs touch:min-h-9 touch:px-3",
  icon: "w-8 min-h-8 p-0 text-sm touch:w-10 touch:min-h-10",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;
  size?: ButtonSize;
  /** Marks a toggle that is on, for example the button of an open drawer. */
  active?: boolean;
  icon?: IconName;
  iconAfter?: IconName;
}

export function Button({
  tone = "default",
  size = "md",
  active,
  icon,
  iconAfter,
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type="button"
      className={cx(
        "inline-flex items-center justify-center gap-[7px] rounded border font-strong whitespace-nowrap transition-colors disabled:opacity-[.42]",
        TONES[tone],
        SIZES[size],
        active && "border-cyan! text-cyan",
        className,
      )}
      {...rest}
    >
      {icon && <Icon name={icon} />}
      {children}
      {iconAfter && <Icon name={iconAfter} />}
    </button>
  );
}

/** A bare ✕ button for panels, drawers and popovers. */
export function CloseButton({
  label,
  className,
  ...rest
}: { label: string } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        "grid size-7 shrink-0 place-items-center rounded-md text-muted hover:bg-hover hover:text-text touch:size-10",
        className,
      )}
      {...rest}
    >
      <Icon name="close" className="size-3.5" />
    </button>
  );
}
