import { act, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ToastGallery, TOAST_SCENARIOS } from "./ToastGallery.tsx";

describe("ToastGallery", () => {
  it("shows every notification of the requested scenario as an auto-selected toast", async () => {
    window.history.pushState({}, "", "/dev/toasts?scenario=stack");
    await act(async () => {
      render(<ToastGallery />);
    });
    const toasts = screen.getAllByTestId("corner-toast");
    expect(toasts).toHaveLength(TOAST_SCENARIOS.stack.length);
    expect(toasts[0]).toHaveTextContent("Only one strategy card was left: you took Technology");
  });
});
