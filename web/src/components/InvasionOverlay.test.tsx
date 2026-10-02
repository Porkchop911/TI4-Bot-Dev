import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { galleryBoard } from "../dev/galleryBoard.ts";
import { InvasionOverlay } from "./InvasionOverlay.tsx";

it("shows the same planet-local resolved ground round without offering spectator controls", () => {
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "18",
      invasion_seq: 4,
      invader: "attacker",
      phase: "ground_battle",
      planets: ["jord"],
      current_planet: "jord",
      defender: "defender",
      ground_round: 2,
      last_step: {
        planet: "jord",
        kind: "ground_round",
        round: 1,
        harrow_hits: 0,
        before: [{ owner: "attacker", unit_type: "infantry", planet: "jord", damaged: false }],
        after: [],
        hits: { attacker: 1, defender: 1 },
        dice: [
          {
            planet: "jord",
            player: "attacker",
            group: "combat value 8",
            face: 9,
            target: 8,
            hit: true,
          },
        ],
      },
    },
  };
  render(
    <InvasionOverlay
      board={board}
      choice={null}
      viewerSeat={null}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.getByTestId("invasion-step")).toHaveTextContent("jord · ground round · round 1");
  expect(screen.getByTestId("invasion-step")).toHaveTextContent(
    "attacker · combat value 8: 9 / 8 hit",
  );
  expect(screen.getByTestId("invasion-step")).toHaveTextContent("Before: attacker infantry");
  expect(screen.getByTestId("invasion-step")).toHaveTextContent("After: None");
  expect(screen.queryByRole("button", { name: "Fight next round" })).not.toBeInTheDocument();
});

it("renders planet exploration effect styling with trait badge, description, flavor text and styled options", async () => {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "72",
      invasion_seq: 2,
      invader: "player_sol",
      phase: "landing",
      planets: ["lisis", "velnor"],
      current_planet: "lisis",
      defender: null,
      ground_round: 0,
    },
  };
  const exploreChoice = {
    actor: "player_sol",
    nonce: "test-nonce-1",
    prompt: "Abandoned Warehouses",
    context: {
      subtype: "abandoned_warehouses_choose_reward",
      target: { Planet: { system: "72", planet: "lisis" } },
    },
    options: [
      { id: "gain", kind: "explore", label: "gain 2 commodities" },
      { id: "convert", kind: "explore", label: "convert up to 2 commodities to trade goods" },
    ],
  };

  render(
    <InvasionOverlay
      board={board}
      choice={exploreChoice}
      viewerSeat="player_sol"
      onSubmit={onSubmit}
      onClose={vi.fn()}
    />,
  );

  const card = screen.getByTestId("invasion-effect-card");
  expect(card).toBeInTheDocument();
  expect(card).toHaveClass("invasion-effect-card--industrial");
  expect(screen.getByText("Planet Exploration · Industrial")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Abandoned Warehouses" })).toBeInTheDocument();
  expect(
    screen.getByText(
      "You may gain 2 commodities, or you may convert up to 2 of your commodities to trade goods.",
    ),
  ).toBeInTheDocument();

  const gainButton = screen.getByRole("button", { name: "gain 2 commodities" });
  const convertButton = screen.getByRole("button", {
    name: "convert up to 2 commodities to trade goods",
  });
  expect(gainButton).toBeInTheDocument();
  expect(convertButton).toBeInTheDocument();

  fireEvent.click(gainButton);
  expect(onSubmit).toHaveBeenCalledWith("gain");
});

it("renders waiting state on the effect card for spectators and other players", () => {
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "72",
      invasion_seq: 2,
      invader: "player_sol",
      phase: "landing",
      planets: ["lisis"],
      current_planet: "lisis",
      defender: null,
      ground_round: 0,
    },
  };
  const exploreChoice = {
    actor: "player_sol",
    nonce: "test-nonce-1",
    prompt: "Abandoned Warehouses",
    context: { subtype: "abandoned_warehouses" },
    options: [{ id: "gain", kind: "explore", label: "gain 2 commodities" }],
  };

  render(
    <InvasionOverlay
      board={board}
      choice={exploreChoice}
      viewerSeat="other_player"
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );

  expect(screen.getByTestId("invasion-effect-card")).toBeInTheDocument();
  expect(screen.getByText(/Waiting for/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "gain 2 commodities" })).not.toBeInTheDocument();
});

it("renders action card effect styling for invasion reactions", () => {
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "18",
      invasion_seq: 3,
      invader: "player_sol",
      phase: "landing",
      planets: ["jord"],
      current_planet: "jord",
      defender: "player_letnev",
      ground_round: 0,
    },
  };
  const reactionChoice = {
    actor: "player_letnev",
    nonce: "test-nonce-2",
    prompt: "Play Parley or pass",
    context: { subtype: "parley" },
    options: [
      {
        id: "play_parley",
        kind: "card",
        label: "Parley",
        payload: { card: "parley" },
      },
      { id: "pass", kind: "decline", label: "Pass" },
    ],
  };

  render(
    <InvasionOverlay
      board={board}
      choice={reactionChoice}
      viewerSeat="player_letnev"
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );

  const card = screen.getByTestId("invasion-effect-card");
  expect(card).toBeInTheDocument();
  expect(card).toHaveClass("invasion-effect-card--action");
  expect(screen.getByText("Action Card · Reaction")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Parley" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Parley" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Pass" })).toBeInTheDocument();
});

it("renders embedded landing tray with planet cards and + / − steppers for the invader without duplicate headers", async () => {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "72",
      invasion_seq: 1,
      invader: "player_sol",
      phase: "landing",
      planets: ["lisis", "velnor"],
      current_planet: null,
      defender: null,
      ground_round: 0,
    },
    systems: {
      ...galleryBoard.systems,
      "72": {
        ...galleryBoard.systems["72"],
        units: [
          { owner: "player_sol", unit_type: "sol_infantry", damaged: false },
          { owner: "player_sol", unit_type: "sol_infantry", damaged: false },
        ],
      },
    },
  };
  const landingChoice = {
    actor: "player_sol",
    nonce: "test-nonce-landing",
    prompt: "commit ground forces in 72",
    context: { subtype: "commit_ground_forces" },
    options: [
      {
        id: "land_lisis",
        kind: "land",
        label: "land sol_infantry on lisis",
        payload: { planet: "lisis", unit: "sol_infantry", damaged: false },
      },
      {
        id: "land_velnor",
        kind: "land",
        label: "land sol_infantry on velnor",
        payload: { planet: "velnor", unit: "sol_infantry", damaged: false },
      },
    ],
  };

  render(
    <InvasionOverlay
      board={board}
      choice={landingChoice}
      viewerSeat="player_sol"
      onSubmit={onSubmit}
      onClose={vi.fn()}
    />,
  );

  // Redundant nav should be omitted when landing
  expect(screen.queryByRole("navigation", { name: "Invasion planets" })).not.toBeInTheDocument();

  // Landing tray embedded should be present
  const tray = screen.getByTestId("invasion-landing-tray");
  expect(tray).toBeInTheDocument();
  expect(tray).toHaveClass("invasion-landing-tray-embedded");

  // Should have exactly 1 minimize button (from the overlay itself, not a second one in the tray)
  expect(screen.getAllByRole("button", { name: "Minimize decision" })).toHaveLength(1);

  // Both planets should be displayed directly as cards (no tabs needed)
  expect(screen.getByRole("button", { name: "lisis" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "velnor" })).toBeInTheDocument();

  // Staging with + button
  const addLisis = screen.getByRole("button", { name: /land sol_infantry on lisis/i });
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeDisabled();
  act(() => {
    fireEvent.click(addLisis);
  });
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeEnabled();

  // Staging on second planet directly without switching tabs
  const addVelnor = screen.getByRole("button", { name: /land sol_infantry on velnor/i });
  act(() => {
    fireEvent.click(addVelnor);
  });

  // Removing from lisis with − button
  const removeLisis = screen.getByRole("button", { name: "Remove sol_infantry from lisis" });
  act(() => {
    fireEvent.click(removeLisis);
  });

  // Confirm remaining landing
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Confirm landings" }));
  });
  expect(onSubmit).toHaveBeenCalledWith("land_velnor");
});
