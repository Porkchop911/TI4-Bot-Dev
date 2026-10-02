import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MapOverlayToolbar } from "./MapOverlayToolbar.tsx";

describe("MapOverlayToolbar", () => {
  it("renders all overlay options and Standard button", () => {
    render(<MapOverlayToolbar activeMode="none" onSelectMode={vi.fn()} />);

    expect(screen.getByTestId("map-overlay-toolbar")).toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-none")).toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-economy")).toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-space_combat")).toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-ground_combat")).toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-tech_benefits")).toBeInTheDocument();
  });

  it("indicates which overlay is active via aria-pressed", () => {
    const { rerender } = render(<MapOverlayToolbar activeMode="economy" onSelectMode={vi.fn()} />);
    expect(screen.getByTestId("overlay-btn-economy")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("overlay-btn-none")).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByTestId("overlay-btn-space_combat")).toHaveAttribute("aria-pressed", "false");

    rerender(<MapOverlayToolbar activeMode="space_combat" onSelectMode={vi.fn()} />);
    expect(screen.getByTestId("overlay-btn-space_combat")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("overlay-btn-economy")).toHaveAttribute("aria-pressed", "false");
  });

  it("calls onSelectMode when clicking an inactive overlay button", () => {
    const onSelectMode = vi.fn();
    render(<MapOverlayToolbar activeMode="none" onSelectMode={onSelectMode} />);

    fireEvent.click(screen.getByTestId("overlay-btn-economy"));
    expect(onSelectMode).toHaveBeenCalledWith("economy");
  });

  it("toggles to 'none' when clicking the currently active overlay button", () => {
    const onSelectMode = vi.fn();
    render(<MapOverlayToolbar activeMode="space_combat" onSelectMode={onSelectMode} />);

    fireEvent.click(screen.getByTestId("overlay-btn-space_combat"));
    expect(onSelectMode).toHaveBeenCalledWith("none");
  });

  it("calls onSelectMode('none') when clicking the Standard button", () => {
    const onSelectMode = vi.fn();
    render(<MapOverlayToolbar activeMode="ground_combat" onSelectMode={onSelectMode} />);

    fireEvent.click(screen.getByTestId("overlay-btn-none"));
    expect(onSelectMode).toHaveBeenCalledWith("none");
  });
});
