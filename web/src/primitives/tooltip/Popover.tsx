import React, { useState, useRef, useId, useEffect, cloneElement, isValidElement } from "react";
import { overlayStack } from "../core/overlayStack.ts";

export interface PopoverProps {
  content: React.ReactNode;
  children: React.ReactElement;
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  position?: "top" | "bottom" | "left" | "right";
  className?: string;
  "data-testid"?: string;
  ariaLabel?: string;
}

export const Popover: React.FC<PopoverProps> = ({
  content,
  children,
  isOpen: controlledIsOpen,
  onOpenChange,
  position = "bottom",
  className,
  "data-testid": testId,
  ariaLabel,
}) => {
  const [uncontrolledIsOpen, setUncontrolledIsOpen] = useState(false);
  const isOpen = controlledIsOpen ?? uncontrolledIsOpen;
  const setIsOpen = (next: boolean) => {
    if (controlledIsOpen === undefined) {
      setUncontrolledIsOpen(next);
    }
    onOpenChange?.(next);
  };

  const containerRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const id = useId();
  const popoverId = `popover-${id}`;

  const toggle = () => setIsOpen(!isOpen);
  const close = (restoreFocus = true) => {
    setIsOpen(false);
    if (restoreFocus) {
      triggerRef.current?.focus();
    }
  };

  // Register with overlayStack on open
  useEffect(() => {
    if (!isOpen) return;
    const unregister = overlayStack.register({
      id: popoverId,
      modal: false,
      element: containerRef.current,
      closeOnOutsideClick: true,
      onDismiss: () => close(true),
    });
    return unregister;
  }, [isOpen, popoverId]);

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
    "aria-haspopup": "dialog",
    "aria-expanded": isOpen,
    "aria-controls": isOpen ? popoverId : undefined,
    onClick: (e: React.MouseEvent) => {
      toggle();
      (childProps.onClick as ((e: React.MouseEvent) => void) | undefined)?.(e);
    },
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        e.stopPropagation();
        close();
      }
      (childProps.onKeyDown as ((e: React.KeyboardEvent) => void) | undefined)?.(e);
    },
  });

  return (
    <div ref={containerRef} style={{ display: "inline-flex", position: "relative" }}>
      {triggerElement}
      {isOpen && (
        <div
          ref={popoverRef}
          role="dialog"
          id={popoverId}
          aria-label={ariaLabel}
          data-testid={testId || "accessible-popover"}
          data-position={position}
          className={`accessible-popover ${className || ""}`}
          style={{
            position: "absolute",
            ...(position === "top"
              ? { bottom: "100%", left: "50%", transform: "translateX(-50%) translateY(-8px)" }
              : position === "bottom"
                ? { top: "100%", left: "50%", transform: "translateX(-50%) translateY(8px)" }
                : position === "left"
                  ? { right: "100%", top: "50%", transform: "translateY(-50%) translateX(-8px)" }
                  : { left: "100%", top: "50%", transform: "translateY(-50%) translateX(8px)" }),
            zIndex: "var(--layer-popover)",
          }}
        >
          {content}
        </div>
      )}
    </div>
  );
};
