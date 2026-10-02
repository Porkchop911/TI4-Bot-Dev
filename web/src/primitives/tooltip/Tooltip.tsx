import React, { useState, useRef, useId, useEffect, cloneElement, isValidElement } from "react";
import { overlayStack } from "../core/overlayStack.ts";

export interface TooltipProps {
  content: React.ReactNode;
  children: React.ReactElement;
  position?: "top" | "bottom" | "left" | "right";
  delayMs?: number;
  disabled?: boolean;
  className?: string;
  "data-testid"?: string;
  as?: "div" | "span";
  wrapperStyle?: React.CSSProperties;
  wrapperClassName?: string;
}

export const Tooltip: React.FC<TooltipProps> = ({
  content,
  children,
  position = "top",
  delayMs = 200,
  disabled = false,
  className,
  "data-testid": testId,
  as = "div",
  wrapperStyle,
  wrapperClassName,
}) => {
  const [isVisible, setIsVisible] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const id = useId();
  const tooltipId = `tooltip-${id}`;

  const show = () => {
    if (disabled || !content) return;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      setIsVisible(true);
    }, delayMs);
  };

  const hide = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setIsVisible(false);
  };

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  // Register with overlayStack on show so Escape or tap outside dismisses tooltip
  useEffect(() => {
    if (!isVisible) return;
    const unregister = overlayStack.register({
      id: tooltipId,
      modal: false,
      element: containerRef.current,
      closeOnOutsideClick: true,
      onDismiss: hide,
    });
    return unregister;
  }, [isVisible, tooltipId]);

  if (!isValidElement(children)) {
    return children;
  }

  const childProps = children.props as Record<string, unknown>;

  const triggerElement = cloneElement(children as React.ReactElement<Record<string, unknown>>, {
    ref: (node: HTMLElement | null) => {
      triggerRef.current = node;
      const childRef = (children as { ref?: React.Ref<HTMLElement> }).ref;
      if (typeof childRef === "function") {
        childRef(node);
      } else if (childRef && "current" in childRef) {
        (childRef as React.MutableRefObject<HTMLElement | null>).current = node;
      }
    },
    "aria-describedby": isVisible ? tooltipId : undefined,
    onPointerEnter: (e: React.PointerEvent) => {
      show();
      (childProps.onPointerEnter as ((e: React.PointerEvent) => void) | undefined)?.(e);
    },
    onPointerLeave: (e: React.PointerEvent) => {
      hide();
      (childProps.onPointerLeave as ((e: React.PointerEvent) => void) | undefined)?.(e);
    },
    onFocus: (e: React.FocusEvent) => {
      show();
      (childProps.onFocus as ((e: React.FocusEvent) => void) | undefined)?.(e);
    },
    onBlur: (e: React.FocusEvent) => {
      hide();
      (childProps.onBlur as ((e: React.FocusEvent) => void) | undefined)?.(e);
    },
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === "Escape" && isVisible) {
        e.stopPropagation();
        hide();
      }
      (childProps.onKeyDown as ((e: React.KeyboardEvent) => void) | undefined)?.(e);
    },
  });

  const Container = as;

  return (
    <Container
      ref={containerRef}
      className={wrapperClassName}
      style={{
        display: as === "span" ? "inline-block" : "inline-flex",
        position: "relative",
        ...wrapperStyle,
      }}
    >
      {triggerElement}
      {isVisible && content && (
        <div
          ref={tooltipRef}
          role="tooltip"
          id={tooltipId}
          data-testid={testId || "accessible-tooltip"}
          data-position={position}
          className={`accessible-tooltip ${className || ""}`}
          style={{
            position: "absolute",
            ...(position === "top"
              ? { bottom: "100%", left: "50%", transform: "translateX(-50%) translateY(-6px)" }
              : position === "bottom"
                ? { top: "100%", left: "50%", transform: "translateX(-50%) translateY(6px)" }
                : position === "left"
                  ? { right: "100%", top: "50%", transform: "translateY(-50%) translateX(-6px)" }
                  : { left: "100%", top: "50%", transform: "translateY(-50%) translateX(6px)" }),
            zIndex: "var(--layer-popover)",
            pointerEvents: "none",
            whiteSpace: typeof content === "string" ? "pre-line" : "normal",
          }}
        >
          {content}
        </div>
      )}
    </Container>
  );
};
