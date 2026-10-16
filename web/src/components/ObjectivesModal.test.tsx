import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { ObjectivesModal } from "./ObjectivesModal.tsx";
import type { PlayerView, ObjectiveProgressView } from "../protocol/types.ts";

const mockPlayers: PlayerView[] = [
  {
    id: "p1",
    faction: "sol",
    victory_points: 3,
    trade_goods: 4,
    commodities: 2,
    tactic_tokens: 3,
    fleet_tokens: 4,
    strategic_tokens: 2,
    passed: false,
    strategy_cards: ["leadership"],
    exhausted_strategy_cards: [],
    technologies: ["sarween_tools"],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 2,
    secret_objectives_count: 1,
    leaders: {},
  },
  {
    id: "p2",
    faction: "hacan",
    victory_points: 2,
    trade_goods: 8,
    commodities: 6,
    tactic_tokens: 2,
    fleet_tokens: 3,
    strategic_tokens: 3,
    passed: false,
    strategy_cards: ["trade"],
    exhausted_strategy_cards: [],
    technologies: [],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 3,
    secret_objectives_count: 2,
    leaders: {},
  },
];

describe("ObjectivesModal", () => {
  it("renders empty state when no public objectives are revealed", () => {
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={[]}
        players={mockPlayers}
      />,
    );

    expect(screen.getByTestId("objectives-empty")).toHaveTextContent(
      "No public objectives revealed yet.",
    );
  });

  it("renders matrix table with Stage I and Stage II objectives", () => {
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["corner", "unify_colonies"]}
        players={mockPlayers}
        viewerSeat="p1"
      />,
    );

    expect(screen.getByTestId("objectives-matrix-table")).toBeInTheDocument();
    expect(screen.getByText("Stage I Objectives (1 VP)")).toBeInTheDocument();
    expect(screen.getByText("Stage II Objectives (2 VP)")).toBeInTheDocument();

    expect(screen.getByTestId("objective-row-corner")).toBeInTheDocument();
    expect(screen.getByTestId("objective-row-unify_colonies")).toBeInTheDocument();
  });

  it("highlights the viewer player column with a 'You' tag", () => {
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["corner"]}
        players={mockPlayers}
        viewerSeat="p1"
      />,
    );

    const p1Col = screen.getByTestId("player-col-header-p1");
    expect(p1Col).toHaveClass("objectives-matrix__player-col--self");
    expect(p1Col).toHaveTextContent("You");

    const p2Col = screen.getByTestId("player-col-header-p2");
    expect(p2Col).not.toHaveClass("objectives-matrix__player-col--self");
    expect(p2Col).not.toHaveTextContent("You");
  });

  it("renders Scored status, Ready status, and In Progress status correctly", () => {
    const scoredObjectives: Record<string, string[]> = {
      p1: ["corner"],
    };

    const objectiveProgress: Record<string, Record<string, ObjectiveProgressView>> = {
      p1: {
        unify_colonies: { have: 6, threshold: 6, satisfied: true },
      },
      p2: {
        corner: { have: 3, threshold: 4, satisfied: false },
        unify_colonies: { have: 2, threshold: 6, satisfied: false },
      },
    };

    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["corner", "unify_colonies"]}
        scoredObjectives={scoredObjectives}
        objectiveProgress={objectiveProgress}
        players={mockPlayers}
        viewerSeat="p1"
      />,
    );

    // p1 scored "corner"
    expect(screen.getByTestId("status-scored-corner-p1")).toBeInTheDocument();
    expect(screen.getByTestId("status-scored-corner-p1")).toHaveTextContent("Scored");

    // p2 in progress on "corner" (3/4)
    expect(screen.getByTestId("status-progress-corner-p2")).toBeInTheDocument();
    expect(screen.getByTestId("status-progress-corner-p2")).toHaveTextContent("3 / 4");

    // p1 is Ready on "unify_colonies" (6/6)
    expect(screen.getByTestId("status-ready-unify_colonies-p1")).toBeInTheDocument();
    expect(screen.getByTestId("status-ready-unify_colonies-p1")).toHaveTextContent("Ready (6/6)");

    // p2 in progress on "unify_colonies" (2/6)
    expect(screen.getByTestId("status-progress-unify_colonies-p2")).toBeInTheDocument();
    expect(screen.getByTestId("status-progress-unify_colonies-p2")).toHaveTextContent("2 / 6");
  });

  it("calls onInspectCard when an objective name button is clicked", () => {
    const onInspectCard = vi.fn();
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["corner"]}
        players={mockPlayers}
        onInspectCard={onInspectCard}
      />,
    );

    fireEvent.click(screen.getByTestId("inspect-objective-corner"));
    expect(onInspectCard).toHaveBeenCalledWith({
      kind: "publicObjective",
      id: "corner",
    });
  });

  it("renders in scoring mode with selectable options, decline, and submit button", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    render(
      <ObjectivesModal
        isOpen={true}
        onClose={onClose}
        revealedObjectives={["lead", "trade_routes", "corner"]}
        players={mockPlayers}
        viewerSeat="p1"
        choice={{
          actor: "p1",
          nonce: "nonce-1",
          prompt: "Score a public objective",
          context: { subtype: "score_objective" },
          options: [
            { id: "lead", label: "Lead From the Front" },
            { id: "trade_routes", label: "Negotiate Trade Routes" },
            { id: "decline", label: "Decline" },
          ],
        }}
        onSubmit={onSubmit}
        isScoringMode={true}
      />,
    );

    // Scoring title and badge
    expect(screen.getByText("Score Public Objective")).toBeInTheDocument();
    expect(screen.getByText("Scoring Window")).toBeInTheDocument();
    expect(screen.getByTestId("objectives-scoring-footer")).toBeInTheDocument();

    // Check minimize button
    const minimizeBtn = screen.getByTestId("minimize-choice-button");
    expect(minimizeBtn).toBeInTheDocument();
    fireEvent.click(minimizeBtn);
    expect(onClose).toHaveBeenCalled();

    // Scoreable options are actionable
    const options = screen.getAllByTestId("choice-option");
    expect(options.length).toBe(3); // lead, trade_routes, decline

    // Select trade_routes
    const tradeOption = document.querySelector(
      '[data-testid="choice-option"][data-option-id="trade_routes"]',
    )!;
    expect(tradeOption).not.toBeNull();
    fireEvent.click(tradeOption);

    // Footer summary updates
    const footer = screen.getByTestId("objectives-scoring-footer");
    expect(footer).toHaveTextContent("Negotiate Trade Routes");

    // Submit
    const submitBtn = screen.getByTestId("submit-choice-button");
    expect(submitBtn).toBeEnabled();
    await act(async () => {
      fireEvent.click(submitBtn);
    });

    expect(onSubmit).toHaveBeenCalledWith("trade_routes");
  });

  it("preserves selection when props/snapshot updates arrive with the same choice nonce", () => {
    const choice = {
      actor: "p1",
      nonce: "nonce-same",
      prompt: "Score a public objective",
      context: { subtype: "score_objective" },
      options: [
        { id: "lead", label: "Lead From the Front" },
        { id: "trade_routes", label: "Negotiate Trade Routes" },
      ],
    };

    const { rerender } = render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["lead", "trade_routes"]}
        players={mockPlayers}
        viewerSeat="p1"
        choice={choice}
        isScoringMode={true}
      />,
    );

    // Select second objective: trade_routes
    const tradeOption = document.querySelector(
      '[data-testid="choice-option"][data-option-id="trade_routes"]',
    )!;
    fireEvent.click(tradeOption);

    const footer = screen.getByTestId("objectives-scoring-footer");
    expect(footer).toHaveTextContent("Negotiate Trade Routes");

    // Simulate snapshot update arriving after 1-2 seconds with fresh object references
    rerender(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["lead", "trade_routes"]}
        players={[...mockPlayers]}
        viewerSeat="p1"
        choice={{
          ...choice,
          options: [
            { id: "lead", label: "Lead From the Front" },
            { id: "trade_routes", label: "Negotiate Trade Routes" },
          ],
        }}
        isScoringMode={true}
      />,
    );

    // Selection MUST remain Negotiate Trade Routes, NOT reset to Lead From the Front!
    expect(footer).toHaveTextContent("Negotiate Trade Routes");
  });

  describe("Imperial", () => {
    const imperial = (controlsMecatol: boolean) => ({
      actor: "p1",
      nonce: "imperial-1",
      prompt: "score a public objective with Imperial",
      context: { subtype: "imperial_score_objective" },
      options: [
        { id: "lead", label: "lead" },
        { id: "decline", kind: "decline", label: "decline" },
      ],
      details: {
        kind: "imperial",
        controls_mecatol: controlsMecatol,
        secrets_held: 2,
        secrets_max: 3,
      },
    });
    const open = (controlsMecatol: boolean, onSubmit = vi.fn().mockResolvedValue(undefined)) =>
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["lead"]}
          players={mockPlayers}
          viewerSeat="p1"
          choice={imperial(controlsMecatol)}
          onSubmit={onSubmit}
        />,
      );

    it("always shows the Mecatol point when the seat holds Mecatol Rex", () => {
      open(true);
      expect(screen.getByTestId("imperial-outcome")).toHaveAttribute("data-variant", "mecatol");
      expect(screen.getByTestId("imperial-outcome-headline")).toHaveTextContent(
        "+1 VP (you hold Mecatol Rex)",
      );
    });

    it("shows the secret draw with the hand when it does not", () => {
      open(false);
      expect(screen.getByTestId("imperial-outcome")).toHaveAttribute("data-variant", "secret");
      expect(screen.getByTestId("imperial-outcome-headline")).toHaveTextContent(
        "Draw a secret objective (2/3 held)",
      );
    });

    it("repeats the outcome in the confirm summary, scored or skipped", () => {
      open(true);
      fireEvent.click(document.querySelector('[data-option-id="decline"]')!);
      expect(screen.getByTestId("objectives-scoring-footer")).toHaveTextContent(
        "Do not score (Decline) · +1 VP for Mecatol Rex",
      );
    });

    it("shows no outcome card for an ordinary scoring window", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["lead"]}
          players={mockPlayers}
          viewerSeat="p1"
          choice={{ ...imperial(true), context: { subtype: "score_objective" } }}
          isScoringMode={true}
        />,
      );
      expect(screen.queryByTestId("imperial-outcome")).toBeNull();
    });
  });

  describe("bought objective progress", () => {
    const render1 = (props: Partial<React.ComponentProps<typeof ObjectivesModal>> = {}) =>
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["monument", "sway_council", "corner"]}
          players={mockPlayers}
          viewerSeat="p1"
          objectiveProgress={{
            p1: {
              monument: { have: 5, threshold: 8, satisfied: false },
              sway_council: { have: 0, threshold: 8, satisfied: false },
              corner: { have: 3, threshold: 4, satisfied: false },
            },
          }}
          {...props}
        />,
      );

    it("labels spending capacity, unit-aware, and marks it as paid at scoring", () => {
      render1();
      const monument = screen.getByTestId("bought-progress-monument-p1");
      expect(monument).toHaveTextContent("can pay 5 / 8 now · pay at scoring");
      expect(monument.querySelector("[data-kind=resources]")).not.toBeNull();
      expect(monument.getAttribute("aria-label")).toContain("can pay 5 / 8 resources now");
      expect(monument).toHaveAttribute("title", expect.stringContaining("paid for when you score them"));
      expect(monument.getAttribute("aria-label")).toContain("ready planets and trade goods");
      expect(screen.getByTestId("bought-progress-sway_council-p1")).toHaveTextContent(
        "can pay 0 / 8 now · pay at scoring",
      );
    });

    it("leaves counting objectives as a plain N / M", () => {
      render1();
      expect(screen.queryByTestId("bought-progress-corner-p1")).toBeNull();
      expect(screen.getByTestId("status-progress-corner-p1")).toHaveTextContent("3 / 4");
    });

    it("drops the suffix for the seat whose scoring window is open", () => {
      render1({
        choice: {
          actor: "p1",
          nonce: "n",
          prompt: "Score",
          context: { subtype: "score_objective" },
          options: [{ id: "corner", label: "Corner" }],
        },
      });
      expect(screen.getByTestId("bought-progress-monument-p1")).toHaveTextContent(/^can pay 5 \/ 8 now$/);
    });

    it("keeps Ready when satisfied and Scored over progress", () => {
      render1({
        scoredObjectives: { p1: ["sway_council"] },
        objectiveProgress: {
          p1: {
            monument: { have: 8, threshold: 8, satisfied: true },
            sway_council: { have: 8, threshold: 8, satisfied: true },
          },
        },
      });
      expect(screen.getByTestId("status-ready-monument-p1")).toHaveTextContent("Ready (8/8)");
      expect(screen.getByTestId("status-scored-sway_council-p1")).toBeInTheDocument();
      expect(screen.queryByTestId("bought-progress-monument-p1")).toBeNull();
    });
  });

  describe("secret objectives in the scoring window", () => {
    const withHand: PlayerView[] = [
      { ...mockPlayers[0], held_secret_objectives: ["ans", "ctr", "baf"] },
      mockPlayers[1],
    ];
    const scoring = (options: { id: string; label: string }[], subtype = "score_objective") => ({
      actor: "p1",
      nonce: "secret-nonce",
      prompt: "score an objective",
      context: { subtype },
      options: [...options, { id: "decline", label: "decline" }],
    });

    it("shows an offered secret as a card with name, condition, phase and VP", async () => {
      const onSubmit = vi.fn().mockResolvedValue(undefined);
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={withHand}
          viewerSeat="p1"
          choice={scoring([
            { id: "corner", label: "corner" },
            { id: "ans", label: "ans" },
          ])}
          onSubmit={onSubmit}
          isScoringMode={true}
        />,
      );
      expect(screen.getByText("Score Objective")).toBeInTheDocument();
      expect(screen.getByTestId("secret-objectives-header")).toBeInTheDocument();
      const row = screen.getByTestId("secret-row-ans");
      expect(row).toHaveTextContent("Adapt New Strategies");
      expect(row).toHaveTextContent("Own 2 faction technologies");
      expect(row).toHaveTextContent("1 VP");
      expect(row).toHaveTextContent("Status phase");
      expect(row).not.toHaveTextContent(/^ans$/);

      fireEvent.click(
        document.querySelector('[data-testid="choice-option"][data-option-id="ans"]')!,
      );
      expect(screen.getByTestId("objectives-scoring-footer")).toHaveTextContent(
        "Adapt New Strategies (+1 VP)",
      );
      await act(async () => {
        fireEvent.click(screen.getByTestId("submit-choice-button"));
      });
      expect(onSubmit).toHaveBeenCalledWith("ans");
    });

    it("lists the rest of the hand as not scoreable now", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={withHand}
          viewerSeat="p1"
          choice={scoring([{ id: "ans", label: "ans" }])}
          isScoringMode={true}
        />,
      );
      expect(screen.getByTestId("secret-held-held-ctr")).toHaveTextContent("not scoreable now");
      expect(screen.getByTestId("secret-row-held-ctr")).toHaveTextContent("Control the Region");
      expect(screen.queryByTestId("secret-row-held-ans")).toBeNull();
      // Not an option: only the offered secret, and decline, are actionable.
      const ids = screen.getAllByTestId("choice-option").map((el) => el.getAttribute("data-option-id"));
      expect(ids).toEqual(["ans", "decline"]);
    });

    it("never files a secret among the public objectives when none are revealed", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={[]}
          players={withHand}
          viewerSeat="p1"
          choice={scoring([{ id: "baf", label: "baf" }], "score_secret_objective")}
          isScoringMode={true}
        />,
      );
      expect(screen.getByText("Score Secret Objective")).toBeInTheDocument();
      expect(screen.queryByTestId("objective-row-baf")).toBeNull();
      expect(screen.queryByText("Stage I Objectives (1 VP)")).toBeNull();
      expect(screen.getByTestId("secret-row-baf")).toHaveTextContent("Betray a Friend");
      expect(screen.getByTestId("secret-row-baf")).toHaveTextContent("Action phase");
    });

    it("reads a Plots Within Plots option as the secret, scored for no point", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={mockPlayers}
          viewerSeat="p1"
          choice={scoring([{ id: "plot|ctr|p2", label: "ctr as a plot with p2's control token" }])}
          isScoringMode={true}
        />,
      );
      const row = screen.getByTestId("secret-row-plot|ctr|p2");
      expect(row).toHaveTextContent("Control the Region");
      expect(row).toHaveTextContent("Plot");
      expect(row).toHaveTextContent("no VP");
    });

    it("hides another seat's hand", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={withHand}
          viewerSeat="p2"
        />,
      );
      expect(screen.queryByTestId("secret-objectives-header")).toBeNull();
    });
  });
  describe("secret objective progress", () => {
    const owner: PlayerView = {
      ...mockPlayers[0],
      held_secret_objectives: ["ctr", "dtgs", "eap"],
      secret_objective_progress: {
        ctr: { have: 4, threshold: 6, satisfied: false },
        eap: { have: 4, threshold: 4, satisfied: true },
      },
    };

    it("lists the viewer's own secrets with progress outside a scoring window", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={[owner, mockPlayers[1]]}
          viewerSeat="p1"
        />,
      );
      expect(screen.getByTestId("secret-objectives-header")).toBeInTheDocument();
      const ctr = screen.getByTestId("secret-progress-ctr");
      expect(ctr).toHaveTextContent("4 / 6 systems with at least one of your ships");
      expect(ctr).toHaveAttribute("data-satisfied", "false");
      expect(screen.getByTestId("secret-progress-eap")).toHaveAttribute("data-satisfied", "true");
      expect(screen.getByTestId("secret-progress-eap")).toHaveTextContent("met");
      // An event secret has no count, only a note.
      expect(screen.queryByTestId("secret-progress-dtgs")).toBeNull();
      expect(screen.getByTestId("secret-progress-note-dtgs")).toBeInTheDocument();
      expect(screen.getByTestId("secret-held-held-ctr")).toHaveTextContent("In your hand");
      expect(screen.getByTestId("secret-held-held-ctr")).not.toHaveTextContent("not scoreable");
    });

    it("shows progress beside held secrets in the scoring window", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={[owner, mockPlayers[1]]}
          viewerSeat="p1"
          choice={{
            actor: "p1",
            nonce: "n",
            prompt: "score",
            context: { subtype: "score_objective" },
            options: [
              { id: "eap", label: "eap" },
              { id: "decline", label: "decline" },
            ],
          } as never}
          isScoringMode={true}
        />,
      );
      expect(screen.getByTestId("secret-held-held-ctr")).toHaveTextContent("not scoreable now");
      expect(screen.getByTestId("secret-progress-ctr")).toHaveTextContent("4 / 6");
    });

    it("shows no secret section to a viewer who is not the holder", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["corner"]}
          players={[owner, mockPlayers[1]]}
          viewerSeat="p2"
        />,
      );
      expect(screen.queryByTestId("secret-objectives-header")).toBeNull();
      expect(screen.queryByTestId("secret-progress-ctr")).toBeNull();
    });
  });

  describe("Push Boundaries per-neighbor progress", () => {
    const progress: Record<string, Record<string, ObjectiveProgressView>> = {
      p1: {
        push_boundaries: {
          have: 1,
          threshold: 2,
          satisfied: false,
          neighbours: [
            { player: "p2", mine: 12, theirs: 10, ahead: true },
            { player: "p3", mine: 12, theirs: 13, ahead: false },
            { player: "p4", mine: 12, theirs: 12, ahead: false },
          ],
        },
      },
      p2: {
        push_boundaries: {
          have: 0,
          threshold: 2,
          satisfied: false,
          neighbours: [{ player: "p1", mine: 10, theirs: 12, ahead: false }],
        },
      },
    };

    it("shows ours/theirs per neighbor with a standing that is not colour alone", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["push_boundaries"]}
          objectiveProgress={progress}
          players={mockPlayers}
          viewerSeat="p1"
        />,
      );
      const ahead = screen.getByTestId("neighbour-row-push_boundaries-p1-p2");
      expect(ahead).toHaveTextContent("12/10");
      expect(ahead).toHaveAttribute("data-standing", "ahead");
      expect(ahead.getAttribute("aria-label")).toMatch(/12 planets against .*10: ahead/);
      const behind = screen.getByTestId("neighbour-row-push_boundaries-p1-p3");
      expect(behind).toHaveTextContent("12/13");
      expect(behind).toHaveAttribute("data-standing", "behind");
      const tied = screen.getByTestId("neighbour-row-push_boundaries-p1-p4");
      expect(tied).toHaveAttribute("data-standing", "tied");
      expect(tied.getAttribute("aria-label")).toMatch(/tied, not ahead/);
      expect(screen.getByTestId("neighbour-progress-push_boundaries-p1")).toHaveTextContent(
        "Ahead of 1 of 3 neighbors · need 2",
      );
      // Every seat's comparison is public, from that seat's side.
      expect(screen.getByTestId("neighbour-row-push_boundaries-p2-p1")).toHaveTextContent("10/12");
      // The viewer's own copy in the card column (shown on phones).
      expect(screen.getByTestId("neighbour-progress-push_boundaries-card")).toHaveTextContent(
        "You:",
      );
    });

    it("drops the comparison once the seat has scored it, and for other objectives", () => {
      render(
        <ObjectivesModal
          isOpen={true}
          onClose={vi.fn()}
          revealedObjectives={["push_boundaries", "corner"]}
          scoredObjectives={{ p1: ["push_boundaries"] }}
          objectiveProgress={{
            ...progress,
            p1: { ...progress.p1, corner: { have: 2, threshold: 4, satisfied: false } },
          }}
          players={mockPlayers}
          viewerSeat="p1"
        />,
      );
      expect(screen.queryByTestId("neighbour-progress-push_boundaries-p1")).toBeNull();
      expect(screen.queryByTestId("neighbour-progress-push_boundaries-card")).toBeNull();
      expect(screen.queryByTestId("neighbour-progress-corner-p1")).toBeNull();
      expect(screen.getByTestId("neighbour-progress-push_boundaries-p2")).toBeInTheDocument();
    });
  });

  it("names the systems Learn the Secrets of the Cosmos counts", () => {
    const owner: PlayerView = {
      ...mockPlayers[0],
      held_secret_objectives: ["lsc"],
      secret_objective_progress: {
        lsc: { have: 2, threshold: 3, satisfied: false, systems: ["26", "25"] },
      },
    };
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["corner"]}
        players={[owner, mockPlayers[1]]}
        viewerSeat="p1"
        board={{
          map_tiles: [
            { system_id: "26", label: "Lodor", q: 0, r: 1, planets: [{ id: "lodor" }] },
            { system_id: "25", label: "Quann", q: 1, r: 0, planets: [{ id: "quann" }] },
          ],
        } as never}
      />,
    );
    expect(screen.getByTestId("secret-progress-lsc")).toHaveTextContent(
      "2 / 3 systems adjacent to an anomaly with your ships",
    );
    expect(screen.getByTestId("secret-progress-systems-lsc")).toHaveTextContent(
      "Counting: #26 [Lodor], #25 [Quann]",
    );
  });

  it("marks met objectives blocked and explains it while a home planet is lost (61.16)", () => {
    render(
      <ObjectivesModal
        isOpen={true}
        onClose={vi.fn()}
        revealedObjectives={["unify_colonies"]}
        objectiveProgress={{
          p1: { unify_colonies: { have: 6, threshold: 6, satisfied: true } },
          p2: { unify_colonies: { have: 6, threshold: 6, satisfied: true } },
        }}
        homeSystemLost={{ p1: ["archonren"] }}
        board={{
          systems: {
            "14": {
              system_id: "14",
              command_tokens: [],
              units: [],
              planets: {
                archonren: { planet_id: "archonren", controlled_by: "p2", exhausted: true },
                archontau: { planet_id: "archontau", controlled_by: "p1", exhausted: false },
              },
            },
          },
        }}
        players={mockPlayers}
        viewerSeat="p1"
      />,
    );
    expect(screen.getByTestId("home-system-lost")).toHaveTextContent(
      /cannot score public objectives: you do not control Archon Ren \(held by .+\) in your home system/,
    );
    expect(screen.getByTestId("status-blocked-unify_colonies-p1")).toHaveTextContent(
      "Met (6/6) · home system lost",
    );
    expect(screen.queryByTestId("status-ready-unify_colonies-p1")).toBeNull();
    // The other seat holds its home: still ready.
    expect(screen.getByTestId("status-ready-unify_colonies-p2")).toBeInTheDocument();
  });
});
