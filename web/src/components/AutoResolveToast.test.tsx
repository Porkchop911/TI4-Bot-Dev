import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AutoResolveToast } from "./AutoResolveToast.tsx";

describe("AutoResolveToast", () => {
  it("names what was chosen and why there was only one choice", () => {
    render(
      <AutoResolveToast
        notification={{
          id: "a",
          decisionType: "pay 1 more resources",
          selectedValue: "trade goods",
          reason: "it was the only way left to pay",
        }}
        onDismiss={() => {}}
      />,
    );
    const toast = screen.getByTestId("corner-toast");
    expect(toast.textContent).toBe("Only one way left to pay: trade goods");
    expect(toast.textContent).not.toContain("auto-selected");
  });

  it("reads the engine's draft note naturally and falls back to a generic sentence", () => {
    const { rerender } = render(
      <AutoResolveToast
        notification={{ id: "c", decisionType: "choose a strategy card", selectedValue: "7. Construction", reason: "only one strategy card left" }}
        onDismiss={() => {}}
      />,
    );
    expect(screen.getByTestId("corner-toast").textContent).toBe(
      "Only one strategy card was left: you took 7. Construction",
    );
    rerender(
      <AutoResolveToast
        notification={{ id: "d", decisionType: "discard an action card for the expedition", selectedValue: "Sabotage", reason: "it was the only action card you held" }}
        onDismiss={() => {}}
      />,
    );
    expect(screen.getByTestId("corner-toast").textContent).toBe("Only one action card to discard: Sabotage");
    rerender(
      <AutoResolveToast
        notification={{ id: "e", decisionType: "something new", selectedValue: "X", reason: "because" }}
        onDismiss={() => {}}
      />,
    );
    expect(screen.getByTestId("corner-toast").textContent).toBe("Only one choice: X (because)");
  });

  it("falls back to a generic reason and dismisses on click", () => {
    vi.useFakeTimers();
    const onDismiss = vi.fn();
    render(
      <AutoResolveToast
        notification={{ id: "b", decisionType: "Strategy Card", selectedValue: "Technology" }}
        onDismiss={onDismiss}
      />,
    );
    expect(screen.getByTestId("corner-toast").textContent).toBe(
      "Only one strategy card was left: you took Technology",
    );
    fireEvent.click(screen.getByTestId("corner-toast"));
    vi.advanceTimersByTime(400);
    expect(onDismiss).toHaveBeenCalledWith("b");
    vi.useRealTimers();
  });
});
