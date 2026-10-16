import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import { galleryBoard } from "../dev/galleryBoard.ts";
import { actor } from "../dev/decisionGalleryCases.ts";

const splitBoard = (rivals: { planet: string; unit_type: string }[] = [], infantry = 6) =>
  ({
    ...galleryBoard,
    active_system: "18",
    map_tiles: [
      {
        system_id: "18",
        label: "18",
        q: 0,
        r: 0,
        planets: [
          { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
          { id: "moll", label: "Moll", resources: 1, influence: 1, traits: ["hazardous"] },
          { id: "quann", label: "Quann", resources: 2, influence: 1 },
        ],
      },
    ],
    invasion: undefined,
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...galleryBoard.systems["18"],
        planets: {
          jord: { planet_id: "jord", exhausted: false },
          moll: { planet_id: "moll", exhausted: false },
          quann: { planet_id: "quann", exhausted: false },
        },
        units: [
          ...Array.from({ length: infantry }, () => ({
            owner: actor,
            unit_type: "infantry",
            damaged: false,
          })),
          ...rivals.map((r) => ({ owner: "defender", damaged: false, ...r })),
        ],
      },
    },
  }) as unknown as typeof galleryBoard;

const choice = {
  actor,
  nonce: "landing-split",
  prompt: "Land",
  context: { subtype: "commit_ground_forces", target: { System: "18" } },
  options: [
    ...["jord", "moll", "quann"].map((planet) => ({
      id: `land|${planet}|infantry`,
      label: `Land infantry on ${planet}`,
      kind: "land",
      payload: { planet, unit: "infantry" },
    })),
    { id: "done_committing", label: "Done", kind: "decline" },
  ],
};

const renderSplit = (board: typeof galleryBoard) =>
  render(
    <InvasionLandingTray
      choice={choice}
      board={board}
      viewerSeat={actor}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
  );
const staged = () =>
  screen
    .getAllByText(/Staged: \d+/)
    .map((node) => Number(/Staged: (\d+)/.exec(node.textContent ?? "")![1]));

it("splits six infantry over three uninhabited planets and says so", () => {
  renderSplit(splitBoard());
  expect(staged()).toEqual([2, 2, 2]);
  const hint = screen.getByTestId("invasion-split-hint");
  expect(hint).toHaveTextContent("Split across uninhabited planets — adjust below");
  expect(hint).toHaveTextContent("jord 2");
  expect(screen.getByRole("button", { name: "Confirm landings" })).toBeEnabled();
});

it("keeps the main force on the defended planet and spreads the rest", () => {
  renderSplit(splitBoard([{ planet: "moll", unit_type: "infantry" }]));
  // jord and quann are uninhabited (one each); moll is defended and keeps the rest
  expect(staged()).toEqual([1, 4, 1]);
});

it("a defended-everywhere system is no split and shows no hint", () => {
  renderSplit(
    splitBoard([
      { planet: "jord", unit_type: "pds" },
      { planet: "moll", unit_type: "pds" },
      { planet: "quann", unit_type: "pds" },
    ]),
  );
  expect(screen.queryByTestId("invasion-split-hint")).toBeNull();
  expect(staged().filter((n) => n > 0)).toEqual([6]);
});

it("editing drops the hint and Reset restores the split default", () => {
  renderSplit(splitBoard());
  fireEvent.click(screen.getByRole("button", { name: "Remove infantry from jord" }));
  expect(screen.queryByTestId("invasion-split-hint")).toBeNull();
  expect(staged()).toEqual([1, 2, 2]);
  fireEvent.click(screen.getByRole("button", { name: "Reset to Defaults" }));
  expect(staged()).toEqual([2, 2, 2]);
  expect(screen.getByTestId("invasion-split-hint")).toBeInTheDocument();
});

it("shows the explore line per planet and updates it with the staged count", () => {
  renderSplit(splitBoard());
  const lists = screen.getAllByTestId("invasion-planet-effects");
  expect(lists[0]).toHaveTextContent("Explores on landing: cultural");
  expect(lists[1]).toHaveTextContent("Explores on landing: hazardous");
  expect(lists[2]).toHaveTextContent("No exploration (planet has no trait)");
  fireEvent.click(screen.getByRole("button", { name: "Remove infantry from moll" }));
  fireEvent.click(screen.getByRole("button", { name: "Remove infantry from moll" }));
  expect(screen.getAllByTestId("invasion-planet-effects")[1]).toHaveTextContent(
    "No units assigned (would explore hazardous)",
  );
});

it("a contested planet explores only if the ground combat is won", () => {
  renderSplit(splitBoard([{ planet: "jord", unit_type: "infantry" }]));
  expect(screen.getAllByTestId("invasion-planet-effects")[0]).toHaveTextContent(
    "If you win the ground combat: explore cultural",
  );
});
