import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { GameProvider } from "../game/context";
import { ReferenceDrawer } from "../game/shell/Reference";
import menuUpdate from "../../../fixtures/protocol/update-menu.json";
import { selectShell } from "../session/select/shell";
import type { SessionUpdate } from "../session/wire";

// The Objectives sheet over an update of the real engine with eight seats, viewed by seat a
// (Sol), with the points written here. The test is here because `dev` is the layer that may
// import both the session and the components.
const recorded = menuUpdate as unknown as SessionUpdate;
const update: SessionUpdate = {
  ...recorded,
  // Nobody has a point yet in the recorded game: L1z1x (f) and Yin (h) get one here.
  view: {
    ...recorded.view,
    players: recorded.view.players.map((player) =>
      player.id === "f" || player.id === "h" ? { ...player, victory_points: 1 } : player,
    ),
  },
  score: {
    target: 10,
    public: [
      {
        id: "expand_borders",
        name: "Expand Borders",
        points: 1,
        text: "Control 6 planets in non-home systems.",
        scored: ["a", "b"],
      },
      {
        id: "master_science",
        name: "Master the Sciences",
        points: 2,
        text: "Own 2 technologies in each of 4 colors.",
        scored: ["d"],
      },
    ],
    sources: [
      { kind: "secrets", name: "", points: { a: 2, d: 1 } },
      { kind: "custodians", name: "", points: { b: 1 } },
      { kind: "imperial", name: "", points: {} },
      { kind: "support", name: "", points: { a: 1 }, from: { a: ["b"] } },
      { kind: "agenda", name: "Mutiny", points: { d: -1 } },
    ],
  },
};
const local = {
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { builds: [], planets: [], goods: 0 },
  productionStage: "build" as const,
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
};

function open(shown = update) {
  const view = selectShell(shown, local, null);
  render(
    <GameProvider seats={view.seats} dispatch={() => {}}>
      <ReferenceDrawer
        id="objectives"
        view={view.reference}
        players={view.players.rows}
        onClose={() => {}}
      />
    </GameProvider>,
  );
}
const row = (name: string) => within(screen.getByRole("article", { name }));
const names = (items: HTMLElement[]) => items.map((item) => item.getAttribute("aria-label"));
/** The tags of a card by the faction of their seat. A seat with nothing from the source has none. */
const cells = (name: string) =>
  Object.fromEntries(
    row(name)
      .getAllByRole("listitem")
      .map((item) => [item.getAttribute("aria-label")?.split(":")[0], item]),
  );
const cellName = (name: string, faction: string) =>
  cells(name)[faction]?.getAttribute("aria-label");

it("has the viewer first in the heading, then the seats by their points", () => {
  open();
  expect(screen.getByText("10 VP to win")).toBeInTheDocument();
  const seats = within(screen.getByRole("group", { name: "Victory points" })).getAllByRole(
    "button",
  );
  expect(seats).toHaveLength(8);
  expect(names(seats)[0]).toBe("Sol (you): 0 points");
  const points = names(seats)
    .slice(1)
    .map((name) => Number(name?.match(/: (\d+) point/)?.[1]));
  expect(points).toEqual([...points].sort((a, b) => b - a));
  expect(names(seats)).toContain("L1z1x: 1 point");
  expect(names(seats)).toContain("Yin: 1 point");
});

it("lists the public objectives by stage, as printed, with who scored each", () => {
  open();
  const first = within(screen.getByRole("region", { name: "Public objectives · stage I" }));
  expect(first.getAllByRole("article")).toHaveLength(1);
  expect(row("Expand Borders").getByText("1 VP")).toBeInTheDocument();
  expect(
    row("Expand Borders").getByText("Control 6 planets in non-home systems."),
  ).toBeInTheDocument();
  expect(cellName("Expand Borders", "Sol")).toBe("Sol: scored");
  expect(cellName("Expand Borders", "Hacan")).toBe("Hacan: scored");
  expect(Object.keys(cells("Expand Borders"))).toHaveLength(2);
  const second = within(screen.getByRole("region", { name: "Public objectives · stage II" }));
  expect(second.getByRole("article", { name: "Master the Sciences" })).toBeInTheDocument();
  expect(row("Master the Sciences").getByText("2 VP")).toBeInTheDocument();
});

it("has of the other sources only the points that are still to get or to lose", () => {
  open();
  // The custodians are taken; the secret objectives and Imperial are in the player sheets.
  const other = within(screen.getByRole("region", { name: "Other points" }));
  expect(other.getAllByRole("article").map((item) => item.getAttribute("aria-label"))).toEqual([
    "Support for the Throne",
    "Agenda · Mutiny",
  ]);
  // An agenda stays: a law can be repealed. A loss has the minus sign.
  expect(cellName("Agenda · Mutiny", "Xxcha")).toBe("Xxcha: −1 point");
  // Support for the Throne shows whose note it is.
  expect(cellName("Support for the Throne", "Sol")).toBe("Sol: 1 point from Hacan");
  expect(
    within(cells("Support for the Throne").Sol).getByRole("img", { name: "Hacan" }),
  ).toBeInTheDocument();
});

it("has the custodians while no seat has them, and no objective that every seat scored", () => {
  const score = update.score!;
  const seats = update.view.players.map((player) => player.id);
  open({
    ...update,
    score: {
      ...score,
      public: [{ ...score.public[0], scored: seats }, score.public[1]],
      sources: score.sources.map((source) =>
        source.kind === "custodians" ? { ...source, points: {} } : source,
      ),
    },
  });
  expect(screen.queryByRole("article", { name: "Expand Borders" })).not.toBeInTheDocument();
  expect(screen.getByText("All scored by every seat")).toBeInTheDocument();
  expect(row("Custodians of Mecatol Rex").getByText("Nobody yet")).toBeInTheDocument();
});

it("lists in the sheet of a player every point of the seat with its reason", () => {
  const view = selectShell(update, local, null);
  const sheet = (seat: string) => {
    const shown = render(
      <GameProvider seats={view.seats} dispatch={() => {}}>
        <ReferenceDrawer
          id={`player:${seat}`}
          view={view.reference}
          players={view.players.rows}
          onClose={() => {}}
        />
      </GameProvider>,
    );
    const points = within(screen.getByRole("region", { name: "Victory points" }));
    // The printed text of an objective is behind its name.
    expect(points.getAllByRole("button", { name: /, printed text$/ }).length).toBeGreaterThan(0);
    const rows = points.getAllByRole("listitem").map((item) => item.textContent);
    shown.unmount();
    return rows;
  };
  expect(sheet("a")).toEqual(["Expand Borders1", "Secret objectives2", "Support for the Throne←1"]);
  // A loss has the minus sign, and an objective of stage II its two points.
  expect(sheet("d")).toEqual(["Master the Sciences2", "Secret objectives1", "Agenda · Mutiny−1"]);
});

it("says when no objective of a stage is revealed, and shows nothing without the fact", () => {
  open({ ...update, score: { ...update.score!, public: [] } });
  expect(screen.getAllByText("None revealed")).toHaveLength(2);
});

it("shows for a chosen seat what it can still score or lose", () => {
  open();
  const seat = (name: RegExp) =>
    within(screen.getByRole("group", { name: "Victory points" })).getByRole("button", { name });
  fireEvent.click(seat(/^Hacan/));
  expect(seat(/^Hacan/)).toHaveAttribute("aria-pressed", "true");
  // Hacan scored Expand Borders, and not Master the Sciences.
  expect(screen.queryByRole("article", { name: "Expand Borders" })).not.toBeInTheDocument();
  expect(screen.getByText("All scored for Hacan")).toBeInTheDocument();
  expect(screen.getByRole("article", { name: "Master the Sciences" })).toBeInTheDocument();
  expect(screen.getByText("1 of 1 open for Hacan")).toBeInTheDocument();
  // Hacan has no Support for the Throne and no point of an agenda.
  const other = within(screen.getByRole("region", { name: "Other points" }));
  expect(other.queryByRole("article")).not.toBeInTheDocument();
  expect(other.getByText("None")).toBeInTheDocument();
  // A second tap shows every seat again.
  fireEvent.click(seat(/^Hacan/));
  expect(screen.getByRole("article", { name: "Expand Borders" })).toBeInTheDocument();
});
