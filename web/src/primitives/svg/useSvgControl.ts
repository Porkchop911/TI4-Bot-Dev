import React from "react";

export interface UseSvgControlOptions<T extends SVGElement = SVGElement> {
  isInteractive?: boolean;
  disabled?: boolean;
  label?: string;
  role?: "button" | "link" | "checkbox" | "radio";
  pressed?: boolean;
  onActivate?: () => void;
  onClick?: (event: React.MouseEvent<T>) => void;
  onKeyDown?: (event: React.KeyboardEvent<T>) => void;
}

export function useSvgControl<T extends SVGElement = SVGElement>(options: UseSvgControlOptions<T>) {
  const {
    isInteractive = true,
    disabled = false,
    label,
    role = "button",
    pressed,
    onActivate,
    onClick,
    onKeyDown,
  } = options;

  const handleClick = (e: React.MouseEvent<T>) => {
    if (disabled || !isInteractive) return;
    if (onClick) {
      onClick(e);
    } else {
      onActivate?.();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<T>) => {
    onKeyDown?.(e);
    if (disabled || !isInteractive) return;

    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      onActivate?.();
    }
  };

  const a11yProps = isInteractive
    ? {
        role,
        tabIndex: disabled ? -1 : 0,
        "aria-label": label,
        "aria-disabled": disabled ? ("true" as const) : undefined,
        "aria-pressed":
          pressed !== undefined ? (pressed ? ("true" as const) : ("false" as const)) : undefined,
      }
    : {
        role: undefined,
        tabIndex: undefined,
        "aria-label": undefined,
        "aria-disabled": undefined,
        "aria-pressed": undefined,
      };

  return {
    ...a11yProps,
    onClick: handleClick,
    onKeyDown: handleKeyDown,
    cursor: isInteractive && !disabled ? "pointer" : "default",
  };
}
