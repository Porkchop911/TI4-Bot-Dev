import React, { useEffect, useRef, useState } from "react";
import { Popover } from "../primitives/index.ts";
import "./UnitInfo.css";

export interface InfoPopoverProps {
  /** Accessible name of the trigger button and of the card it opens. */
  label: string;
  /** What the trigger button shows; defaults to an info mark. */
  trigger?: React.ReactNode;
  className?: string;
  "data-testid"?: string;
  position?: "top" | "bottom" | "left" | "right";
  /** The card body. */
  children: React.ReactNode;
}

const CLOSE_DELAY_MS = 120;

/**
 * An info button that opens a card on mouse hover, on keyboard focus, and on click or tap (a click
 * pins it open). Escape and a click outside close it. Built on the Popover primitive, which owns the
 * dialog semantics, the overlay stack and focus return.
 */
export const InfoPopover: React.FC<InfoPopoverProps> = ({
  label,
  trigger = "i",
  className,
  "data-testid": testId,
  position = "bottom",
  children,
}) => {
  const [open, setOpen] = useState(false);
  const pinned = useRef(false);
  const clicked = useRef(false);
  const pointer = useRef(false);
  const suppressFocus = useRef(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelClose = () => {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  };
  useEffect(() => cancelClose, []);

  const openTransient = () => {
    cancelClose();
    if (open) return;
    pinned.current = false;
    setOpen(true);
  };
  const closeTransient = () => {
    cancelClose();
    if (pinned.current) return;
    closeTimer.current = setTimeout(() => setOpen(false), CLOSE_DELAY_MS);
  };

  const onOpenChange = (next: boolean) => {
    cancelClose();
    if (!next && clicked.current && open && !pinned.current) {
      // A click on a card that hover or focus opened pins it instead of toggling it shut.
      clicked.current = false;
      pinned.current = true;
      return;
    }
    clicked.current = false;
    pinned.current = next;
    if (!next) {
      // Closing returns focus to the trigger; that must not count as a focus that reopens it.
      suppressFocus.current = true;
      setTimeout(() => (suppressFocus.current = false), 0);
    }
    setOpen(next);
  };

  return (
    <Popover
      isOpen={open}
      onOpenChange={onOpenChange}
      position={position}
      portal
      ariaLabel={label}
      className={`info-card ${className ?? ""}`}
      data-testid={testId ? `${testId}-card` : undefined}
      content={
        <div onPointerEnter={cancelClose} onPointerLeave={closeTransient}>
          {children}
        </div>
      }
    >
      <button
        type="button"
        className="info-trigger"
        data-testid={testId}
        aria-label={label}
        onClickCapture={() => {
          clicked.current = true;
        }}
        onPointerDown={() => {
          pointer.current = true;
        }}
        onPointerEnter={(e) => {
          if (e.pointerType !== "touch") openTransient();
        }}
        onPointerLeave={(e) => {
          if (e.pointerType !== "touch") closeTransient();
        }}
        onFocus={() => {
          if (!pointer.current && !suppressFocus.current) openTransient();
        }}
        onBlur={() => {
          if (!pointer.current) {
            pinned.current = false;
            cancelClose();
            setOpen(false);
          }
          pointer.current = false;
        }}
      >
        <span aria-hidden="true">{trigger}</span>
      </button>
    </Popover>
  );
};
