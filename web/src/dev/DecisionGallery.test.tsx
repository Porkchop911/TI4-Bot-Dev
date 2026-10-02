import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { deriveChoiceRendererModel, type ChoiceWorkflowKind } from "../presentation/choiceModel.ts";
import { DecisionGallery } from "./DecisionGallery.tsx";
import { fallbackCases, galleryCases } from "./decisionGalleryCases.ts";

describe("development decision gallery", () => {
  it("has a correctly classified synthetic preview for every workflow and labels fallbacks", () => {
    expect(galleryCases).toHaveLength(18);
    expect(new Set(galleryCases.map(({ workflow }) => workflow)).size).toBe(18);
    for (const item of [...galleryCases, ...fallbackCases]) {
      expect(deriveChoiceRendererModel(item.choice, item.choice.actor)?.workflow).toBe(
        item.workflow as ChoiceWorkflowKind,
      );
      expect(deriveChoiceRendererModel(item.choice, "other_seat")).toBeNull();
    }
    expect(fallbackCases.every((item) => Boolean(item.fallback))).toBe(true);
  });

  it("previews the real renderer, offered IDs, local submissions and the other-seat boundary", async () => {
    render(<DecisionGallery />);
    expect(screen.getByText(/Workflow kinds \(18\)/)).toBeInTheDocument();
    expect(
      screen.getByText(`Fallbacks and boundary states (${fallbackCases.length})`),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /empty movement/i }));
    expect(screen.getByTestId("tactical-movement-tray")).toBeVisible();
    fireEvent.click(screen.getByTestId("commit-moves-btn"));
    await waitFor(() =>
      expect(screen.getByText(/Local submission: done_moving/)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByText("Gallery debug details · synthetic fixture"));
    expect(screen.getByText(/"type":"submit_choice"/)).toHaveTextContent(
      '"option_id":"done_moving"',
    );
    expect(screen.getByText(/Awaiting next decision/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Minimize decision" }));
    fireEvent.click(screen.getByLabelText("View as another seat"));
    expect(screen.queryByTestId("tactical-movement-tray")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "All decisions" }));
    fireEvent.click(
      screen.getByRole("button", { name: /^Unknown subtype Unknown subtype → generic modal/i }),
    );
    expect(screen.getByTestId("pending-choice-dialog")).toBeVisible();
    fireEvent.click(screen.getByText("Gallery debug details · synthetic fixture"));
    expect(screen.getByLabelText("Gallery debug details")).toHaveTextContent(
      "Unknown subtype → generic modal",
    );
  });

  it("shows a consistent payment map with only ready offered planets targetable", () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^payment pay_resources/i }));
    expect(screen.getByTestId("planet-jord")).toHaveAttribute("data-target-candidate", "true");
    expect(screen.getByTestId("planet-exhausted")).not.toHaveAttribute("data-target-candidate");
    expect(screen.queryByText(/Unknown participant/)).not.toBeInTheDocument();
  });

  it("matches the engine vote planet ID to a ready planet on the map", () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^agenda vote planets/i }));
    expect(screen.getByTestId("planet-jord")).toHaveAttribute("data-target-candidate", "true");
    expect(screen.getByTestId("planet-exhausted")).not.toHaveAttribute("data-target-candidate");
  });

  it("shows strategy card printed text and records simulated rejection without advancing nonce", async () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^strategy card draft/i }));
    expect(screen.getByText(/Gain 3 command tokens/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Reject next submission"));
    fireEvent.click(screen.getByTestId("submit-choice-button"));
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Simulated rejection"));
    fireEvent.click(screen.getByText("Gallery debug details · synthetic fixture"));
    expect(screen.getByText(/simulated rejection/)).toHaveTextContent(
      '"option_id":"pok1leadership"',
    );
    expect(screen.queryByText(/Awaiting next decision/)).not.toBeInTheDocument();
  });

  it("selects an offered activation target directly on the map without modal and submits on confirmation", async () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^system activation/i }));
    expect(screen.queryByTestId("pending-choice-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).toBeVisible();
    expect(screen.getByText(/directly on the map to activate/i)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("system-hex-24"));
    expect(screen.getByTestId("system-hex-24")).toHaveAttribute("data-system-selected", "true");
    const bar = screen.getByTestId("system-activation-bar");
    expect(bar).toHaveTextContent(/Activate/);
    expect(bar).toHaveTextContent("Mehar Xull");
    fireEvent.click(screen.getByTestId("confirm-activation-btn"));
    await waitFor(() => expect(screen.getByText(/Local submission: 24/)).toBeInTheDocument());
  });

  it("switches from confirm activation bar to blocked status when clicking a non-activatable system", () => {
    render(<DecisionGallery />);
    fireEvent.click(screen.getByRole("button", { name: /^system activation/i }));

    // 1. Click activatable system (24 - Mehar Xull)
    fireEvent.click(screen.getByTestId("system-hex-24"));
    expect(screen.getByTestId("confirm-activation-btn")).toBeVisible();
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent("Mehar Xull");

    // 2. Click blocked/non-activatable system (22 - Tar'Mann, contains player token)
    fireEvent.click(screen.getByTestId("system-hex-22"));
    expect(screen.queryByTestId("confirm-activation-btn")).not.toBeInTheDocument();
    expect(screen.getByTestId("system-activation-bar")).not.toHaveTextContent("Mehar Xull");
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent("Activated / Blocked");
    expect(screen.getByTestId("system-activation-bar")).toHaveTextContent("Tar'Mann");
  });
});
