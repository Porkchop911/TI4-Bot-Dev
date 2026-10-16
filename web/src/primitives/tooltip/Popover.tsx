import React, {
  useState,
  useRef,
  useId,
  useEffect,
  useLayoutEffect,
  cloneElement,
  isValidElement,
} from "react";
import { createPortal } from "react-dom";
import { overlayStack } from "../core/overlayStack.ts";

export interface PopoverProps {
  /**
   * Renders the card in document.body, fixed beside the trigger and kept inside the viewport, so a
   * scrolling or clipping ancestor (a dialog body) cannot cut it off. Off by default.
   */
  portal?: boolean;
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
  portal = false,
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

  // Portal mode: where the card sits, measured from the trigger once the card has rendered.
  const [fixedPlace, setFixedPlace] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    if (!portal || !isOpen) {
      setFixedPlace(null);
      return;
    }
    const place = () => {
      const trigger = triggerRef.current?.getBoundingClientRect();
      const card = popoverRef.current?.getBoundingClientRect();
      if (!trigger || !card) return;
      const margin = 8;
      const roomBelow = window.innerHeight - trigger.bottom - margin;
      const roomAbove = trigger.top - margin;
      const above = roomBelow < card.height && roomAbove > roomBelow;
      let top = above ? trigger.top - card.height - 6 : trigger.bottom + 6;
      let left = Math.min(
        Math.max(margin, trigger.left + trigger.width / 2 - card.width / 2),
        Math.max(margin, window.innerWidth - card.width - margin),
      );
      if (roomBelow < card.height && roomAbove < card.height) {
        // Neither side has room for the whole card. Put it beside the trigger when there is width for
        // that (it then never covers the trigger, so a pointer resting on the trigger cannot reopen
        // it after Escape), else overlap. Either way its bottom edge stays on screen.
        const roomRight = window.innerWidth - trigger.right - margin;
        const roomLeft = trigger.left - margin;
        if (roomRight >= card.width) left = trigger.right + 6;
        else if (roomLeft >= card.width) left = trigger.left - card.width - 6;
        top = trigger.top;
      }
      const fit = Math.min(top, window.innerHeight - card.height - margin);
      setFixedPlace({ left, top: Math.max(margin, fit) });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [portal, isOpen]);

  // Register with overlayStack on open
  useEffect(() => {
    if (!isOpen) return;
    // A portalled card is outside the container, but a press inside it is not an outside press.
    const element = portal
      ? ({
          contains: (node: Node | null) =>
            !!node &&
            (!!containerRef.current?.contains(node) || !!popoverRef.current?.contains(node)),
        } as unknown as HTMLElement)
      : containerRef.current;
    const unregister = overlayStack.register({
      id: popoverId,
      modal: false,
      element,
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

  const popoverBox = (
    <div
      ref={popoverRef}
      role="dialog"
      id={popoverId}
      aria-label={ariaLabel}
      data-testid={testId || "accessible-popover"}
      data-position={position}
      data-portal={portal ? "true" : undefined}
      className={`accessible-popover ${className || ""}`}
      style={
        portal
          ? {
              position: "fixed",
              left: fixedPlace?.left ?? 0,
              top: fixedPlace?.top ?? 0,
              // Hidden for the one frame before it is measured, so it does not flash at the corner.
              visibility: fixedPlace ? "visible" : "hidden",
              zIndex: "var(--layer-popover-portal, var(--layer-popover))",
            }
          : {
              position: "absolute",
              ...(position === "top"
                ? { bottom: "100%", left: "50%", transform: "translateX(-50%) translateY(-8px)" }
                : position === "bottom"
                  ? { top: "100%", left: "50%", transform: "translateX(-50%) translateY(8px)" }
                  : position === "left"
                    ? { right: "100%", top: "50%", transform: "translateY(-50%) translateX(-8px)" }
                    : { left: "100%", top: "50%", transform: "translateY(-50%) translateX(8px)" }),
              zIndex: "var(--layer-popover)",
            }
      }
    >
      {content}
    </div>
  );

  return (
    <div ref={containerRef} style={{ display: "inline-flex", position: "relative" }}>
      {triggerElement}
      {isOpen && !portal && popoverBox}
      {isOpen && portal && typeof document !== "undefined" && createPortal(popoverBox, document.body)}
    </div>
  );
};
