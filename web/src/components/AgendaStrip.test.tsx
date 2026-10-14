import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgendaPillProvider, AgendaStrip, useAgendaPill } from "./AgendaStrip.tsx";
import type { AgendaStrip as Model } from "../presentation/agendaStrip.ts";

const model = (extra: Partial<Model> = {}): Model => ({
  number: 1,
  name: "Minister of Peace",
  kind: "for_against",
  kindLabel: "For/Against",
  type: "Law",
  text: ["For: something.", "Against: nothing."],
  outcomes: ["FOR", "AGAINST"],
  whose: { seat: "p1", label: "You", you: true, role: "vote", text: "Your vote" },
  speaker: "Alice",
  tally: "FOR 3 · AGAINST 1",
  ...extra,
});

afterEach(cleanup);

describe("AgendaStrip", () => {
  it("shows the one-line summary and publishes its bottom edge for the dialogs", () => {
    render(<AgendaStrip strip={model()} />);
    expect(screen.getByTestId("agenda-strip-text").textContent).toBe(
      "Agenda 1/2: Minister of Peace · For/Against · Your vote",
    );
    expect(document.documentElement.dataset.agendaStrip).toBe("on");
    expect(document.documentElement.style.getPropertyValue("--agenda-strip-bottom")).toMatch(/px$/);
  });

  it("opens the printed text, outcomes, tally and speaker on tap and closes again", () => {
    render(<AgendaStrip strip={model()} />);
    expect(screen.queryByTestId("agenda-strip-details")).toBeNull();
    fireEvent.click(screen.getByTestId("agenda-strip-toggle"));
    const details = screen.getByTestId("agenda-strip-details");
    expect(details.textContent).toContain("For: something.");
    expect(details.textContent).toContain("Outcomes: FOR / AGAINST");
    expect(screen.getByTestId("agenda-strip-tally").textContent).toContain("FOR 3 · AGAINST 1");
    expect(details.textContent).toContain("Speaker: Alice");
    fireEvent.click(screen.getByTestId("agenda-strip-toggle"));
    expect(screen.queryByTestId("agenda-strip-details")).toBeNull();
  });

  it("is not a toggle when there is nothing more to show, and cleans up the document on unmount", () => {
    const { unmount } = render(
      <AgendaStrip strip={model({ text: [], outcomes: [], tally: null, speaker: null, type: null })} />,
    );
    expect((screen.getByTestId("agenda-strip-toggle") as HTMLButtonElement).disabled).toBe(true);
    unmount();
    expect(document.documentElement.dataset.agendaStrip).toBeUndefined();
    expect(document.documentElement.style.getPropertyValue("--agenda-strip-bottom")).toBe("");
  });
});

describe("the pill context", () => {
  const Probe = () => <span data-testid="probe">{useAgendaPill() ?? "none"}</span>;
  it("carries the pill wording and defaults to none", () => {
    render(<Probe />);
    expect(screen.getByTestId("probe").textContent).toBe("none");
    cleanup();
    render(
      <AgendaPillProvider value="Voting: Minister of Peace — For/Against">
        <Probe />
      </AgendaPillProvider>,
    );
    expect(screen.getByTestId("probe").textContent).toBe("Voting: Minister of Peace — For/Against");
  });
});
