import React, { forwardRef } from "react";
import { useSvgControl } from "./useSvgControl.ts";

export interface SvgButtonProps extends React.SVGAttributes<SVGGElement> {
  label?: string;
  isInteractive?: boolean;
  disabled?: boolean;
  pressed?: boolean;
  onActivate?: () => void;
  "data-testid"?: string;
}

export const SvgButton = forwardRef<SVGGElement, SvgButtonProps>(
  (
    {
      label,
      isInteractive = true,
      disabled = false,
      pressed,
      onActivate,
      onClick,
      onKeyDown,
      children,
      className,
      style,
      "data-testid": testId,
      ...rest
    },
    ref,
  ) => {
    const {
      role,
      tabIndex,
      "aria-label": ariaLabel,
      "aria-disabled": ariaDisabled,
      "aria-pressed": ariaPressed,
      onClick: handleClick,
      onKeyDown: handleKeyDown,
      cursor,
    } = useSvgControl<SVGGElement>({
      isInteractive,
      disabled,
      label,
      pressed,
      onActivate,
      onClick,
      onKeyDown,
    });

    return (
      <g
        ref={ref}
        role={role}
        tabIndex={tabIndex}
        aria-label={ariaLabel}
        aria-disabled={ariaDisabled}
        aria-pressed={ariaPressed}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
        data-testid={testId}
        className={`svg-interactive-control ${className || ""}`}
        style={{
          cursor,
          outline: "none",
          ...style,
        }}
        {...rest}
      >
        {children}
      </g>
    );
  },
);

SvgButton.displayName = "SvgButton";
