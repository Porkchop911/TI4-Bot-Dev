import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  deriveChoiceRendererModel,
  type ChoiceWorkflowKind,
} from "../presentation/choiceModel.ts";
import { DecisionGallery } from "./DecisionGallery.tsx";
import { fallbackCases, galleryCases } from "./decisionGalleryCases.ts";

describe("development decision gallery", () => {
  it("has a correctly classified synthetic preview for every workflow and labels fallbacks", () => {
    expect(galleryCases).toHaveLength(20);
    expect(new Set(galleryCases.map(({ workflow }) => workflow)).size).toBe(20);
    for (const item of [...galleryCases, ...fallbackCases]) {
      expect(
        deriveChoiceRendererModel(item.choice, item.choice.actor)?.workflow,
      ).toBe(item.workflow as ChoiceWorkflowKind);
      expect(deriveChoiceRendererModel(item.choice, "other_seat")).toBeNull();
    }
    expect(fallbackCases.every((item) => Boolean(item.fallback))).toBe(true);
  });

  it("previews the real renderer, offered IDs, local submissions and the other-seat boundary", async () => {
    render(<DecisionGallery />);
    expect(screen.getByText(/Workflow kinds \(20\)/)).toBeInTheDocument();
    expect(
      screen.getByText(
        `Fallbacks and boundary states (${fallbackCases.length})`,
      ),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /empty movement/i }));
    expect(screen.getByTestId("tactical-movement-tray")).toBeVisible();
    fireEvent.click(screen.getByTestId("commit-moves-btn"));
    await waitFor(() =>
      expect(
        screen.getByText(/Local submission: done_moving/),
      ).toBeInTheDocument(),
    );
    fireEvent.click(
      screen.getByText("Gallery debug details · synthetic fixture"),
    );
    expect(screen.getByText(/"type":"submit_choice"/)).toHaveTextContent(
      '"option_id":"done_moving"',
    );
    expect(screen.getByText(/Awaiting next decision/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Minimize decision" }));
    fireEvent.click(screen.getByLabelText("View as another seat"));
    expect(
      screen.queryByTestId("tactical-movement-tray"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    fireEvent.click(
      screen.getByRole("button", {
        name: /^Unknown subtype Unknown subtype → generic modal/i,
      }),
    );
    expect(screen.getByTestId("pending-choice-dialog")).toBeVisible();
    fireEvent.click(
      screen.getByText("Gallery debug details · synthetic fixture"),
    );
    expect(screen.getByLabelText("Gallery debug details")).toHaveTextContent(
      "Unknown subtype → generic modal",
    );
  });

  it("previews staged hit assignment over a mixed fleet and records the casualty plan locally", async () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /Assign hits across a mixed fleet/i }),
    );
    expect(screen.getByTestId("hit-row-fighter|intact")).toHaveTextContent(
      "×8",
    );
    expect(screen.getByTestId("hit-row-destroyer|intact")).toHaveTextContent(
      "×2",
    );
    expect(screen.getByTestId("hit-row-dreadnought#1")).toBeInTheDocument();
    expect(screen.getByTestId("hit-row-dreadnought#2")).toBeInTheDocument();
    expect(screen.getByTestId("hit-row-carrier#1")).toHaveTextContent(
      "capacity 4",
    );
    expect(screen.getByTestId("hit-assignment-cargo")).toHaveTextContent(
      "Cargo in space: 10",
    );
    fireEvent.click(screen.getByTestId("hit-auto-assign"));
    fireEvent.click(screen.getByTestId("hit-confirm"));
    await waitFor(() =>
      expect(
        screen.getByText(/Local submission: batch casualties/),
      ).toBeInTheDocument(),
    );
  });

  it("stages one hit when a sustain decision does not state its amount", () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /Sustain with no amount stated/i }),
    );
    expect(screen.getByTestId("hit-assignment-remaining")).toHaveTextContent(
      "1 of 1 hit left to assign",
    );
  });

  it("shows a consistent payment map with only ready offered planets targetable", () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^payment pay_resources/i }),
    );
    expect(screen.getByTestId("planet-jord")).toHaveAttribute(
      "data-target-candidate",
      "true",
    );
    expect(screen.getByTestId("planet-exhausted")).not.toHaveAttribute(
      "data-target-candidate",
    );
    expect(screen.queryByText(/Unknown participant/)).not.toBeInTheDocument();
  });

  it("matches the engine vote planet ID to a ready planet on the map", () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^agenda vote planets/i }),
    );
    expect(screen.getByTestId("planet-jord")).toHaveAttribute(
      "data-target-candidate",
      "true",
    );
    expect(screen.getByTestId("planet-exhausted")).not.toHaveAttribute(
      "data-target-candidate",
    );
  });

  it("shows strategy card printed text and records simulated rejection without advancing nonce", async () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^strategy card draft/i }),
    );
    expect(screen.getByText(/Gain 3 command tokens/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Reject next submission"));
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Simulated rejection",
      ),
    );
    fireEvent.click(
      screen.getByText("Gallery debug details · synthetic fixture"),
    );
    expect(screen.getByText(/simulated rejection/)).toHaveTextContent(
      '"option_id":"pok1leadership"',
    );
    expect(
      screen.queryByText(/Awaiting next decision/),
    ).not.toBeInTheDocument();
  });

  it("selects an offered activation target directly on the map without modal and submits on confirmation", async () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^system activation/i }),
    );
    expect(
      screen.queryByTestId("pending-choice-dialog"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).toBeVisible();
    expect(
      screen.getByText(/directly on the map to activate/i),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("system-hex-24"));
    expect(screen.getByTestId("system-hex-24")).toHaveAttribute(
      "data-system-selected",
      "true",
    );
    const bar = screen.getByTestId("system-activation-bar");
    expect(bar).toHaveTextContent(/Activate/);
    expect(bar).toHaveTextContent("Mehar Xull");
    fireEvent.click(screen.getByTestId("confirm-activation-btn"));
    await waitFor(() =>
      expect(screen.getByText(/Local submission: 24/)).toBeInTheDocument(),
    );
  });

  it("switches from confirm activation bar to blocked status when clicking a non-activatable system", () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^system activation/i }),
    );

    // 1. Click activatable system (24 - Mehar Xull)
    fireEvent.click(screen.getByTestId("system-hex-24"));
    expect(screen.getByTestId("confirm-activation-btn")).toBeVisible();
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent(
      "Mehar Xull",
    );

    // 2. Click blocked/non-activatable system (22 - Tar'Mann, contains player token)
    fireEvent.click(screen.getByTestId("system-hex-22"));
    expect(
      screen.queryByTestId("confirm-activation-btn"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).not.toHaveTextContent(
      "Mehar Xull",
    );
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent(
      "Activated / Blocked",
    );
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent(
      "Tar'Mann",
    );
  });
  it("picks a planet on the map, shows the action in the bar and confirms it", async () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^planet selection/i }));
    expect(
      screen.queryByTestId("pending-choice-dialog"),
    ).not.toBeInTheDocument();
    const bar = screen.getByTestId("planet-selection-bar");
    expect(bar).toHaveTextContent("Mining Initiative");
    expect(bar).toHaveTextContent("Mine which planet");
    expect(
      screen.getByTestId("planet-target-reticle-lodor"),
    ).toBeInTheDocument();
    // System 33 has two candidates: a hex click only inspects.
    fireEvent.click(screen.getByTestId("system-hex-33"));
    expect(screen.queryByTestId("confirm-planet-btn")).not.toBeInTheDocument();
    // Quann's option has no payload.system: the planet is still found on the map.
    fireEvent.click(screen.getByTestId("planet-quann"));
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "Mining Initiative — mine Quann (21)",
    );
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() =>
      expect(screen.getByText(/Local submission: quann/)).toBeInTheDocument(),
    );
  });

  it("offers each structure after picking a planet on the map", async () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^Place a structure/i }),
    );
    fireEvent.click(screen.getByTestId("planet-jord"));
    fireEvent.click(screen.getByTestId("planet-option-spacedock|18|jord"));
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "place spacedock on Jord",
    );
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() =>
      expect(
        screen.getByText(/Local submission: spacedock\|18\|jord/),
      ).toBeInTheDocument(),
    );
  });

  it("votes for a planet on an Elect Planet agenda from the map", async () => {
    render(<DecisionGallery />);
    fireEvent.click(
      screen.getByRole("button", { name: /^Elect Planet vote/i }),
    );
    expect(screen.queryByTestId("agenda-ballot-modal")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("planet-lodor"));
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "Agenda Vote — vote for Lodor (31) · 3 votes cast",
    );
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() =>
      expect(
        screen.getByText(/Local submission: vote\|lodor/),
      ).toBeInTheDocument(),
    );
  });

  it("previews the persistent turn bar: one card, two cards, a used card, end turn and not your turn", async () => {
    render(<DecisionGallery />);
    const open = (name: RegExp) =>
      fireEvent.click(screen.getByRole("button", { name }));
    open(/Turn bar: your turn, one strategy card/);
    expect(screen.getByTestId("turn-action-bar")).toHaveAttribute("data-mode", "active");
    expect(screen.getAllByTestId(/^turn-bar-strategic-/)).toHaveLength(1);
    fireEvent.click(screen.getByTestId("turn-bar-trade"));
    expect(screen.getByTestId("turn-bar-partner-xxcha")).toHaveTextContent("No contact");
    fireEvent.click(screen.getByTestId("turn-bar-open-hacan"));
    await waitFor(() =>
      expect(screen.getByText(/Local submission: component\|trade\|hacan/)).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    open(/Turn bar: two strategy cards/);
    expect(screen.getAllByTestId(/^turn-bar-strategic-/)).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    open(/Turn bar: strategic card used/);
    expect(screen.getByTestId("turn-bar-strategic-pok2diplomacy")).toHaveAttribute("aria-disabled", "true");

    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    open(/Turn bar: end your turn/);
    expect(screen.getByTestId("turn-bar-end")).toHaveAttribute("aria-disabled", "false");

    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    open(/Turn bar: not your turn/);
    expect(screen.getByTestId("turn-action-bar")).toHaveAttribute("data-mode", "readonly");
    expect(screen.getByTestId("turn-bar-tactical")).toHaveTextContent("Not your turn");
  });
});
