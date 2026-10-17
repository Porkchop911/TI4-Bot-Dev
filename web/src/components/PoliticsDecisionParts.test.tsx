import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { DecisionTableProvider } from "./PoliticsDecisionParts.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";
import type { DecisionTable } from "../presentation/politicsDecision.ts";

const table = {
  players: [
    { id: "a", faction: "sol", victory_points: 3, commodities: 1 },
    { id: "b", faction: "hacan", victory_points: 5, commodities: 0 },
  ],
  seating_order: ["a", "b"],
  speaker: "a",
} as unknown as DecisionTable;

const base = (subtype: string, extra: Partial<PendingChoiceDto>): PendingChoiceDto => ({
  prompt: "q",
  actor: "a",
  nonce: "n1",
  context: { subtype } as never,
  options: [],
  ...extra,
});

describe("Politics, Trade and Hacan agent decisions", () => {
  it("shows the agenda and keeps the options and submit button", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PendingChoiceModal
        onSubmit={onSubmit}
        choice={base("politics_place_agenda", {
          prompt: "place mutiny where",
          details: { agenda: { name: "Mutiny", type: "Law", target: "For/Against", text1: "Printed text.", text2: "" } },
          options: [
            { id: "top", label: "on top of the deck", payload: {}, kind: "" },
            { id: "bottom", label: "on the bottom", payload: {}, kind: "" },
          ],
        })}
      />,
    );
    expect(screen.getByTestId("agenda-card-name")).toHaveTextContent("Mutiny");
    expect(screen.getByTestId("agenda-card-text")).toHaveTextContent("Printed text.");
    const rows = screen.getAllByTestId("choice-option");
    expect(rows.map((row) => row.getAttribute("data-option-id"))).toEqual(["top", "bottom"]);
    fireEvent.click(screen.getAllByRole("radio")[1]);
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalledWith("bottom"));
  });

  it("describes speaker candidates and submits the same option id", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <DecisionTableProvider table={table}>
        <PendingChoiceModal
          onSubmit={onSubmit}
          choice={base("politics_choose_speaker", {
            prompt: "who becomes speaker",
            details: { seats: { Hacan: "b" } },
            options: [{ id: "Hacan", label: "Hacan becomes speaker", payload: {}, kind: "" }],
          })}
        />
      </DecisionTableProvider>,
    );
    expect(screen.getByTestId("choice-option-note")).toHaveTextContent(/Hacan.*5 VP.*2nd in speaker order/);
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalledWith("Hacan"));
  });

  it("shows commodities for replenish and a reason for the Hacan agent", () => {
    const { unmount } = render(
      <DecisionTableProvider table={table}>
        <PendingChoiceModal
          onSubmit={vi.fn()}
          choice={base("trade_choose_replenish", {
            details: { seats: { Hacan: "b" } },
            options: [
              { id: "Hacan", label: "Hacan replenishes commodities", kind: "replenish", payload: {} },
              { id: "done", label: "nobody else replenishes", kind: "decline", payload: {} },
            ],
          })}
        />
      </DecisionTableProvider>,
    );
    expect(screen.getAllByTestId("choice-option-note")[0]).toHaveTextContent("0 commodities held");
    expect(screen.getAllByTestId("choice-option")).toHaveLength(2);
    unmount();
    render(
      <DecisionTableProvider table={table}>
        <PendingChoiceModal
          onSubmit={vi.fn()}
          choice={base("leader_hacanagent_branch", {
            options: [
              { id: "self", label: "gain 2 commodities", payload: {}, kind: "" },
              { id: "b", label: "replenish b", payload: {}, kind: "" },
            ],
          })}
        />
      </DecisionTableProvider>,
    );
    expect(screen.getByTestId("hacan-reason")).toHaveTextContent(/holds 0/);
    expect(screen.getAllByTestId("choice-option-note")[1]).toHaveTextContent(/Hacan/);
    expect(screen.getByTestId("submit-choice-button")).toBeInTheDocument();
  });
});

describe("decisions about a revealed agenda show that agenda", () => {
  const secret = {
    id: "secret",
    name: "Classified Document Leaks",
    type: "Directive",
    target: "Elect Scored Secret Objective",
    text1: "The elected secret objective becomes a public objective.",
    text2: "",
  };

  it("Quash names the agenda it would discard, above the use/decline options", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PendingChoiceModal
        onSubmit={onSubmit}
        choice={{
          ...base("quash", {
            prompt: "Quash: spend 1 strategy token to discard this agenda and reveal the next",
            details: { agenda: secret },
            options: [
              { id: "use", label: "use quash", kind: "promissory_note", payload: {} },
              { id: "decline", label: "decline", kind: "decline", payload: {} },
            ],
          }),
          context: { subtype: "quash", source: { Content: "quash" } } as never,
        }}
      />,
    );
    const card = screen.getByTestId("decision-agenda-card");
    expect(card).toHaveTextContent("Revealed agenda");
    expect(screen.getByTestId("decision-agenda-name")).toHaveTextContent("Classified Document Leaks");
    expect(screen.getByTestId("decision-agenda-name")).toHaveTextContent("Directive");
    expect(screen.getByTestId("decision-agenda-text")).toHaveTextContent("becomes a public objective");
    const rows = screen.getAllByTestId("choice-option");
    expect(rows.map((row) => row.getAttribute("data-option-id"))).toEqual(["use", "decline"]);
    fireEvent.click(screen.getAllByRole("radio")[0]);
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalledWith("use"));
  });

  it("an action-card window on the reveal shows it too", () => {
    render(
      <PendingChoiceModal
        onSubmit={vi.fn()}
        choice={{
          ...base("reaction_when_AGENDA_REVEALED", {
            prompt: "when AGENDA_REVEALED",
            details: { agenda: secret },
            options: [
              { id: "action_card:a:veto:AGENDA_REVEALED:when", label: "Veto", payload: {}, kind: "" },
              { id: "decline", label: "pass", kind: "decline", payload: {} },
            ],
          }),
          context: {
            subtype: "reaction_when_AGENDA_REVEALED",
            trigger: { kind: "agenda_revealed", event_type: "AGENDA_REVEALED", relation: "when", agenda: "secret" },
          } as never,
        }}
      />,
    );
    expect(screen.getByTestId("decision-agenda-name")).toHaveTextContent("Classified Document Leaks");
  });

  it("decisions without an agenda show no agenda card", () => {
    render(
      <PendingChoiceModal
        onSubmit={vi.fn()}
        choice={base("some_other", { options: [{ id: "x", label: "x", payload: {}, kind: "" }] })}
      />,
    );
    expect(screen.queryByTestId("decision-agenda-card")).toBeNull();
  });
});
