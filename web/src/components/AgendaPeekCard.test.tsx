import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { AgendaPeekCard } from "./AgendaPeekCard.tsx";

const peek = (key: string) => ({
  key,
  entries: [
    { place: 1, name: "Mutiny", tags: ["Law", "For/Against"], text: ["For: x."] },
    { place: 2, name: "Secret", tags: ["Directive"], text: [] },
  ],
});

const open = () => fireEvent.click(screen.getByTestId("agenda-peek-toggle"));

describe("AgendaPeekCard", () => {
  it("starts as a small chip that names the next agenda and does not list the cards", () => {
    render(<AgendaPeekCard peek={peek("a,b")} />);
    expect(screen.getByTestId("agenda-peek-toggle")).toHaveTextContent("Next agenda: Mutiny +1");
    expect(screen.getByTestId("agenda-peek-toggle")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryAllByTestId("agenda-peek-entry")).toHaveLength(0);
  });

  it("opens to the cards in deck order and says it is private", () => {
    render(<AgendaPeekCard peek={peek("a,b")} />);
    open();
    expect(screen.getAllByTestId("agenda-peek-entry")).toHaveLength(2);
    expect(screen.getByText("Mutiny")).toBeInTheDocument();
    expect(screen.getByLabelText("Next to be revealed")).toHaveTextContent("1");
    expect(screen.getByText(/only you can see this/)).toBeInTheDocument();
  });

  it("collapses again from the open card", () => {
    render(<AgendaPeekCard peek={peek("a,b")} />);
    open();
    open();
    expect(screen.queryAllByTestId("agenda-peek-entry")).toHaveLength(0);
  });

  it("closes, stays closed for the same cards and returns (collapsed) for a new peek", () => {
    const { rerender } = render(<AgendaPeekCard peek={peek("a,b")} />);
    fireEvent.click(screen.getByTestId("agenda-peek-close"));
    expect(screen.queryByTestId("agenda-peek")).toBeNull();
    rerender(<AgendaPeekCard peek={peek("a,b")} />);
    expect(screen.queryByTestId("agenda-peek")).toBeNull();
    rerender(<AgendaPeekCard peek={peek("c,d")} />);
    expect(screen.getByTestId("agenda-peek")).toBeInTheDocument();
    expect(screen.queryAllByTestId("agenda-peek-entry")).toHaveLength(0);
  });

  it("Escape closes the open card, and the chip only while it has focus", () => {
    render(<AgendaPeekCard peek={peek("a,b")} />);
    // The chip leaves an Escape elsewhere on the page (the turn menu, a dialog) alone.
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByTestId("agenda-peek")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByTestId("agenda-peek-toggle"), { key: "Escape" });
    expect(screen.queryByTestId("agenda-peek")).toBeNull();
  });

  it("Escape anywhere closes the opened card", () => {
    render(<AgendaPeekCard peek={peek("a,b")} />);
    open();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByTestId("agenda-peek")).toBeNull();
  });

  it("sits above the turn bar instead of covering it", async () => {
    const bar = document.createElement("div");
    bar.setAttribute("data-testid", "turn-action-bar");
    bar.getBoundingClientRect = () => ({ top: window.innerHeight - 300, height: 300, bottom: window.innerHeight, left: 0, right: 0, width: 0, x: 0, y: 0, toJSON: () => ({}) });
    document.body.appendChild(bar);
    try {
      render(<AgendaPeekCard peek={peek("a,b")} />);
      expect(screen.getByTestId("agenda-peek").style.getPropertyValue("--peek-bottom")).toBe("308px");
    } finally {
      bar.remove();
    }
  });

  it("renders nothing without a peek", () => {
    render(<AgendaPeekCard peek={null} />);
    expect(screen.queryByTestId("agenda-peek")).toBeNull();
  });
});
