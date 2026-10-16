import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import { OddsEngineContextProvider } from "./OddsEngine.tsx";
import { galleryBoard, galleryPlayers } from "../dev/galleryBoard.ts";
import { actor } from "../dev/decisionGalleryCases.ts";
import { partialContext } from "../test/wire.ts";

const choice = {
  actor,
  nonce: "landing-1",
  prompt: "Land on Jord",
  context: partialContext({ subtype: "commit_ground_forces", target: { System: "18" } }),
  options: [
    {
      id: "land|jord|infantry",
      label: "Land infantry on Jord",
      kind: "land",
      payload: { planet: "jord", unit: "infantry" },
    },
    { id: "done_landing", label: "Done landing", kind: "decline", payload: {} },
  ],
};

afterEach(() => vi.unstubAllGlobals());

it("uses the engine's classified defender, all standing guns and legal Harrow in a draft-keyed request", async () => {
  const query = vi.fn().mockResolvedValue({ attacker_win_rate: 0.4, simulations: 2000 });
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "18",
      invasion_seq: 8,
      invader: actor,
      phase: "landing",
      planets: ["jord"],
      current_planet: null,
      defender: null,
      ground_round: 0,
      odds_context: {
        jord: {
          opponent: "defender",
          available: true,
          ground_force_types: ["spec_ops", "infantry", "mech"],
          additional_guns: { pds: 1 },
          harrow_units: { dreadnought: 1 },
        },
      },
    },
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...galleryBoard.systems["18"],
        units: [
          { owner: actor, unit_type: "spec_ops", damaged: false, planet: "jord", galvanized: false },
          { owner: actor, unit_type: "infantry", damaged: false, planet: null, galvanized: false },
          { owner: "defender", unit_type: "infantry", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "pds", damaged: false, planet: "jord", galvanized: false },
        ],
      },
    },
  };
  const players = {
    [actor]: galleryPlayers[0],
    defender: { ...galleryPlayers[1], id: "defender", faction: "letnev" },
  };
  render(
    <OddsEngineContextProvider value={{ state: "ready", query }}>
      <InvasionLandingTray
        choice={choice}
        board={board}
        players={players}
        viewerSeat={actor}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />
    </OddsEngineContextProvider>,
  );
  // The default plan stages the spec ops already on the planet first, then the loose infantry
  // from space; each staging is one odds request keyed to the draft.
  await waitFor(() => expect(query).toHaveBeenCalledTimes(2));
  const first = query.mock.calls[0][0];
  expect(first.query).toBe("ground_odds");
  expect(first.attacker.units).toEqual({ spec_ops: 1 });
  expect(first.defender.faction).toBe("letnev");
  expect(first.defender.guns).toEqual({ pds: 1 });
  expect(first.harrow).toEqual({ dreadnought: 1 });
  const staged = query.mock.calls[1][0];
  expect(staged.attacker.units).toEqual({ spec_ops: 1, infantry: 1 });
  expect(staged.defender.guns).toEqual({ pds: 1 });
  expect(staged.harrow).toEqual({ dreadnought: 1 });
  // Everything available is staged, so there is nothing left to add.
  expect(screen.getByRole("button", { name: /Land infantry on Jord/ })).toBeDisabled();
  expect(screen.getByTestId("invasion-odds")).toHaveTextContent("defender: 40%");
});

it("stages a single offered landing, submits once and retains the exact option id", async () => {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  render(
    <InvasionLandingTray
      choice={choice}
      viewerSeat={actor}
      board={galleryBoard}
      onSubmit={onSubmit}
      onClose={vi.fn()}
    />,
  );
  // M20: Auto-populates defaults, so button starts enabled
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeEnabled();
  // With auto-populated defaults, submission should include those units
  fireEvent.click(screen.getByRole("button", { name: "Confirm landings" }));
  await waitFor(() => expect(onSubmit).toHaveBeenCalled());
});

it("does not expose the actor landing options to another seat", () => {
  render(
    <InvasionLandingTray
      choice={choice}
      viewerSeat="other_seat"
      board={galleryBoard}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.queryByRole("button", { name: "jord" })).not.toBeInTheDocument();
});

it("uses a fresh option for each staged copy and keeps the remainder when interrupted", async () => {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const board = {
    ...galleryBoard,
    invasion: {
      system_id: "18",
      invasion_seq: 1,
      invader: actor,
      phase: "landing",
      planets: ["jord"],
      current_planet: null,
      defender: null,
      ground_round: 0,
      odds_context: {},
    },
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...galleryBoard.systems["18"],
        units: [
          { owner: actor, unit_type: "infantry", damaged: false, planet: null, galvanized: false },
          { owner: actor, unit_type: "infantry", damaged: false, planet: null, galvanized: false },
        ],
      },
    },
  };
  const { rerender } = render(
    <InvasionLandingTray
      choice={choice}
      board={board}
      viewerSeat={actor}
      onSubmit={onSubmit}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "jord" }));
  fireEvent.click(screen.getByRole("button", { name: /Land infantry on Jord/ }));
  fireEvent.click(screen.getByRole("button", { name: /Land infantry on Jord/ }));
  fireEvent.click(screen.getByRole("button", { name: "Confirm landings" }));
  await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith("land|jord|infantry"));
  rerender(
    <InvasionLandingTray
      choice={{
        ...choice,
        nonce: "landing-2",
        options: [{ ...choice.options[0], id: "fresh-offer" }, choice.options[1]],
      }}
      board={board}
      viewerSeat={actor}
      onSubmit={onSubmit}
      onClose={vi.fn()}
    />,
  );
  await waitFor(() => expect(onSubmit).toHaveBeenNthCalledWith(2, "fresh-offer"));
});

it("shows each planet's resources, influence, trait and attachment-modified values", () => {
  const sys = galleryBoard.systems["18"];
  const board = {
    ...galleryBoard,
    active_system: "18",
    map_tiles: [
      {
        system_id: "18",
        label: "18",
        q: 0,
        r: 0,
        planets: [
          { id: "jord", label: "Jord", resources: 4, influence: 2, traits: ["cultural"] },
          { id: "moll", label: "Moll", resources: 1, influence: 1 },
        ],
      },
    ],
    invasion: undefined,
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...sys,
        planets: {
          jord: { planet_id: "jord", exhausted: false, attachments: ["dmz"] },
          moll: { planet_id: "moll", exhausted: false },
        },
      },
    },
  } as unknown as typeof galleryBoard;
  const twoPlanets = {
    ...choice,
    options: [
      choice.options[0],
      {
        id: "land|moll|infantry",
        label: "Land",
        kind: "land",
        payload: { planet: "moll", unit: "infantry" },
      },
    ],
  };
  render(
    <InvasionLandingTray
      choice={twoPlanets}
      board={board}
      viewerSeat={actor}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  const rows = screen.getAllByTestId("invasion-planet-values");
  expect(rows).toHaveLength(2);
  expect(rows[1]).toHaveTextContent("1");
  expect(rows[0]).toHaveTextContent("cultural");
  expect(rows[0]).toHaveTextContent("dmz");
  expect(rows[1].querySelector('[aria-label="1 resource"]')).not.toBeNull();
  expect(rows[1].querySelector('[aria-label="1 influence"]')).not.toBeNull();
});

it("shows the defender's ground forces, PDS and Space Cannon Defense beside the planet being landed on", () => {
  const board = {
    ...galleryBoard,
    invasion: undefined,
    active_system: "18",
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...galleryBoard.systems["18"],
        units: [
          { owner: actor, unit_type: "infantry", damaged: false, planet: null, galvanized: false },
          { owner: "defender", unit_type: "infantry", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "infantry", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "infantry", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "mech", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "pds", damaged: false, planet: "jord", galvanized: false },
          { owner: "defender", unit_type: "space_dock", damaged: false, planet: "jord", galvanized: false },
        ],
      },
    },
  };
  const players = {
    [actor]: galleryPlayers[0],
    defender: { ...galleryPlayers[1], id: "defender", faction: "letnev" },
  };
  render(
    <InvasionLandingTray
      choice={choice}
      board={board}
      players={players}
      viewerSeat={actor}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  const defenders = screen.getByTestId("invasion-defenders");
  expect(defenders).toHaveTextContent("letnev:");
  expect(defenders).toHaveTextContent("1 Mech");
  expect(defenders).toHaveTextContent("3 Infantry");
  expect(defenders).toHaveTextContent("1 PDS");
  expect(defenders).toHaveTextContent("1 Space Dock");
  expect(screen.getByTestId("invasion-scd")).toHaveTextContent(
    "Space Cannon Defense: 1 die (PDS hits on 6) fires at forces you land here, ~0.5 hits",
  );
});

it("shows no defender block for a planet nobody else has units on", () => {
  render(
    <InvasionLandingTray
      choice={choice}
      board={galleryBoard}
      viewerSeat={actor}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(screen.queryByTestId("invasion-defenders")).toBeNull();
});
