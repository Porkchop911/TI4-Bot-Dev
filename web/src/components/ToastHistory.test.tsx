import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ToastHistory } from "./ToastHistory.tsx";

describe("ToastHistory", () => {
  it("renders nothing without history", () => {
    render(<ToastHistory history={[]} />);
    expect(screen.queryByTestId("toast-history-toggle")).toBeNull();
  });

  it("lists the toasts again, newest first, behind a button", () => {
    render(
      <ToastHistory
        history={[
          { id: "a", kind: "action", actor: "p2", text: "played Sabotage" },
          { id: "b", kind: "agenda", tag: "Agenda", text: "Mutiny passed" },
        ]}
      />,
    );
    expect(screen.queryByTestId("toast-history-list")).toBeNull();
    fireEvent.click(screen.getByTestId("toast-history-toggle"));
    const items = screen.getAllByTestId("toast-history-item");
    expect(items.map((i) => i.textContent)).toEqual([expect.stringContaining("Mutiny passed"), expect.stringContaining("played Sabotage")]);
    fireEvent.click(screen.getByTestId("toast-history-toggle"));
    expect(screen.queryByTestId("toast-history-list")).toBeNull();
  });
});
