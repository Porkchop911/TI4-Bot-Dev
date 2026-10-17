import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AutoResolveToast } from "./AutoResolveToast.tsx";

const toast = { id: "t", kind: "notice" as const, decisionType: "x", selectedValue: "y", text: "Something happened", tag: "Note" };

/** A pointer drag with the movement the browser would report (jsdom has no layout). */
function drag(el: HTMLElement, from: number, to: number, ms = 400, pointerType = "touch") {
  fireEvent.pointerDown(el, { pointerId: 1, clientX: from, clientY: 100, pointerType, button: 0 });
  fireEvent.pointerMove(el, { pointerId: 1, clientX: (from + to) / 2, clientY: 101, pointerType });
  fireEvent.pointerMove(el, { pointerId: 1, clientX: to, clientY: 102, pointerType });
  act(() => vi.advanceTimersByTime(ms));
  fireEvent.pointerUp(el, { pointerId: 1, clientX: to, clientY: 102, pointerType });
}

describe("AutoResolveToast swipe", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("follows the finger, and is dismissed once it is let go far enough to the left", () => {
    const onDismiss = vi.fn();
    render(<AutoResolveToast notification={toast} onDismiss={onDismiss} />);
    const el = screen.getByTestId("corner-toast");
    fireEvent.pointerDown(el, { pointerId: 1, clientX: 200, clientY: 100, pointerType: "touch" });
    fireEvent.pointerMove(el, { pointerId: 1, clientX: 150, clientY: 100, pointerType: "touch" });
    expect(el.style.transform).toBe("translateX(-50px)");
    expect(Number(el.style.opacity)).toBeLessThan(1);
    fireEvent.pointerMove(el, { pointerId: 1, clientX: 100, clientY: 100, pointerType: "touch" });
    act(() => vi.advanceTimersByTime(500));
    fireEvent.pointerUp(el, { pointerId: 1, clientX: 100, clientY: 100, pointerType: "touch" });
    expect(el).toHaveClass("dismissing", "dismissing--left");
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(350));
    expect(onDismiss).toHaveBeenCalledWith("t");
  });

  it("is dismissed to the right as well, and by a mouse drag", () => {
    const onDismiss = vi.fn();
    render(<AutoResolveToast notification={toast} onDismiss={onDismiss} />);
    const el = screen.getByTestId("corner-toast");
    drag(el, 100, 220, 500, "mouse");
    expect(el).toHaveClass("dismissing--right");
    act(() => vi.advanceTimersByTime(350));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("springs back when a slow drag stops short, and stays on screen", () => {
    const onDismiss = vi.fn();
    render(<AutoResolveToast notification={toast} onDismiss={onDismiss} />);
    const el = screen.getByTestId("corner-toast");
    drag(el, 200, 170, 800);
    expect(el).not.toHaveClass("dismissing");
    expect(el.style.transform).toBe("");
    act(() => vi.advanceTimersByTime(350));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("dismisses on a quick flick that is shorter than the dismiss distance", () => {
    render(<AutoResolveToast notification={toast} onDismiss={vi.fn()} />);
    const el = screen.getByTestId("corner-toast");
    drag(el, 200, 160, 50);
    expect(el).toHaveClass("dismissing--left");
  });

  it("ignores a mostly vertical drag (the page scrolls)", () => {
    render(<AutoResolveToast notification={toast} onDismiss={vi.fn()} />);
    const el = screen.getByTestId("corner-toast");
    fireEvent.pointerDown(el, { pointerId: 1, clientX: 100, clientY: 100, pointerType: "touch" });
    fireEvent.pointerMove(el, { pointerId: 1, clientX: 108, clientY: 160, pointerType: "touch" });
    fireEvent.pointerMove(el, { pointerId: 1, clientX: 20, clientY: 170, pointerType: "touch" });
    fireEvent.pointerUp(el, { pointerId: 1, clientX: 20, clientY: 170, pointerType: "touch" });
    expect(el).not.toHaveClass("dismissing");
  });

  it("the click that follows a swipe is not a second dismissal path, a plain click still dismisses", () => {
    render(<AutoResolveToast notification={toast} onDismiss={vi.fn()} />);
    const el = screen.getByTestId("corner-toast");
    drag(el, 200, 170, 800); // sprang back
    fireEvent.click(el); // the browser's click after the drag: swallowed
    expect(el).not.toHaveClass("dismissing");
    fireEvent.click(el); // a deliberate click
    expect(el).toHaveClass("dismissing");
  });

  it("can be dismissed from the keyboard with Escape or Delete", () => {
    const { unmount } = render(<AutoResolveToast notification={toast} onDismiss={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId("corner-toast"), { key: "Escape" });
    expect(screen.getByTestId("corner-toast")).toHaveClass("dismissing");
    unmount();
    render(<AutoResolveToast notification={toast} onDismiss={vi.fn()} />);
    fireEvent.keyDown(screen.getByTestId("corner-toast"), { key: "Delete" });
    expect(screen.getByTestId("corner-toast")).toHaveClass("dismissing");
  });
});
