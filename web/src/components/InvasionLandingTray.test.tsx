import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import { galleryBoard, galleryPlayers } from "../dev/galleryBoard.ts";
import { actor } from "../dev/decisionGalleryCases.ts";

const choice = {
  actor,
  nonce: "landing-1",
  prompt: "Land on Jord",
  context: { subtype: "commit_ground_forces", target: { System: "18" } },
  options: [
    {
      id: "land|jord|infantry",
      label: "Land infantry on Jord",
      kind: "land",
      payload: { planet: "jord", unit: "infantry" },
    },
    { id: "done_landing", label: "Done landing", kind: "decline" },
  ],
};

afterEach(() => vi.unstubAllGlobals());

it("uses the engine's classified defender, all standing guns and legal Harrow in a draft-keyed request", async () => {
  const fetch = vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({ attacker_win_rate: 0.4, simulations: 2000 }),
  });
  vi.stubGlobal("fetch", fetch);
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
          { owner: actor, unit_type: "spec_ops", damaged: false, planet: "jord" },
          { owner: actor, unit_type: "infantry", damaged: false },
          { owner: "defender", unit_type: "infantry", damaged: false, planet: "jord" },
          { owner: "defender", unit_type: "pds", damaged: false, planet: "jord" },
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
  fireEvent.click(screen.getByRole("button", { name: "jord" }));
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
  const first = JSON.parse(fetch.mock.calls[0][1].body);
  expect(first.attacker.units).toEqual({ spec_ops: 1 });
  expect(first.defender.guns).toEqual({ pds: 1 });
  expect(first.harrow).toEqual({ dreadnought: 1 });
  fireEvent.click(screen.getByRole("button", { name: /Land infantry on Jord/ }));
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
  expect(JSON.parse(fetch.mock.calls[1][1].body).attacker.units).toEqual({
    spec_ops: 1,
    infantry: 1,
  });
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
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "jord" }));
  fireEvent.click(screen.getByRole("button", { name: /Land infantry on Jord/ }));
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm landings" }));
  await waitFor(() => expect(onSubmit).toHaveBeenCalledExactlyOnceWith("land|jord|infantry"));
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
    },
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...galleryBoard.systems["18"],
        units: [
          { owner: actor, unit_type: "infantry", damaged: false },
          { owner: actor, unit_type: "infantry", damaged: false },
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
