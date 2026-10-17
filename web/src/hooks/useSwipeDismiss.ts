import { useCallback, useRef, useState } from "react";
import type React from "react";
import { isSwipe, swipeEnd, type SwipeEnd } from "../presentation/swipeDismiss.ts";

interface Drag {
  pointerId: number;
  startX: number;
  startY: number;
  startedAt: number;
  swiping: boolean;
}

/**
 * Drag an element sideways to dismiss it: touch, pen and mouse alike (pointer events). While the
 * drag runs `offset` follows the pointer; on release `onEnd` hears how it ended. A drag that
 * turned into a swipe also swallows the click the browser sends after it, so letting go over the
 * toast never counts as a tap. Vertical movement hands the gesture back to the page (scrolling).
 */
export function useSwipeDismiss(onEnd: (end: SwipeEnd, offset: number) => void, disabled = false) {
  const drag = useRef<Drag | null>(null);
  const swallow = useRef(false);
  const [offset, setOffset] = useState(0);
  const [dragging, setDragging] = useState(false);

  const finish = useCallback(
    (event: React.PointerEvent<HTMLElement>, cancelled: boolean) => {
      const current = drag.current;
      if (!current || current.pointerId !== event.pointerId) return;
      drag.current = null;
      try {
        event.currentTarget.releasePointerCapture?.(event.pointerId);
      } catch {
        /* the pointer is already gone */
      }
      setDragging(false);
      if (!current.swiping) return;
      swallow.current = true;
      const dx = event.clientX - current.startX;
      const end = cancelled ? "snap-back" : swipeEnd({ dx, elapsedMs: Date.now() - current.startedAt });
      if (end === "snap-back") setOffset(0);
      else setOffset(dx);
      onEnd(end, dx);
    },
    [onEnd],
  );

  const handlers = {
    onPointerDown: (event: React.PointerEvent<HTMLElement>) => {
      if (disabled || drag.current || (event.pointerType === "mouse" && event.button !== 0)) return;
      swallow.current = false;
      drag.current = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        startedAt: Date.now(),
        swiping: false,
      };
    },
    onPointerMove: (event: React.PointerEvent<HTMLElement>) => {
      const current = drag.current;
      if (!current || current.pointerId !== event.pointerId) return;
      const dx = event.clientX - current.startX;
      const dy = event.clientY - current.startY;
      if (!current.swiping) {
        // Mostly vertical: the page scrolls, this is not a swipe.
        if (Math.abs(dy) > Math.abs(dx) && Math.abs(dy) > 10) {
          drag.current = null;
          return;
        }
        if (!isSwipe(dx)) return;
        current.swiping = true;
        try {
          event.currentTarget.setPointerCapture?.(event.pointerId);
        } catch {
          /* synthetic pointer: no capture, the move events still arrive */
        }
        setDragging(true);
      }
      setOffset(dx);
    },
    onPointerUp: (event: React.PointerEvent<HTMLElement>) => finish(event, false),
    onPointerCancel: (event: React.PointerEvent<HTMLElement>) => finish(event, true),
    /** Capture phase: a click that ends a swipe is not a tap. */
    onClickCapture: (event: React.MouseEvent<HTMLElement>) => {
      if (!swallow.current) return;
      swallow.current = false;
      event.preventDefault();
      event.stopPropagation();
    },
  };

  return { handlers, offset, dragging };
}
