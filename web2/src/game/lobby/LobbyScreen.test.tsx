import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type {
  FactionSheetView,
  LobbyFactionOptionView,
  LobbyIntent,
  LobbyRowView,
  LobbyView,
} from "../../model";
import { GameProvider } from "../context";
import { LobbyScreen, TableSheet } from "./LobbyScreen";

beforeEach(() => {
  // jsdom has no layout: the page is a desktop.
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
});

const SHEET: FactionSheetView = {
  title: "The Emirates of Hacan",
  facts: [{ label: "Commodities", value: "6" }],
  abilities: [
    { name: "Masters of Trade", kind: "", window: "", text: "Printed.", state: null, unlock: "" },
  ],
  technologies: [],
  leaders: [],
};
const option = (
  id: string,
  name: string,
  change: Partial<LobbyFactionOptionView> = {},
): LobbyFactionOptionView => ({
  id,
  name,
  chosen: false,
  intent: { type: "faction", player: "ada", faction: id || null },
  taken: "",
  sheet: null,
  ...change,
});
const row = (id: string, name: string, change: Partial<LobbyRowView> = {}): LobbyRowView => ({
  id,
  player: id,
  sign: null,
  name,
  you: false,
  rename: false,
  become: null,
  faction: "Random",
  sheet: null,
  editor: null,
  buttons: [],
  remove: null,
  ...change,
});
const VIEW: LobbyView = {
  title: "Lobby · 2 players",
  status: "",
  board: { tiles: [] } as unknown as LobbyView["board"],
  seats: {},
  setup: [],
  filled: "2 of 2",
  help: [],
  rows: [
    row("slot_1", "Ada", {
      you: true,
      editor: {
        label: "Faction of Ada",
        random: option("", "Random", { chosen: true, intent: null }),
        groups: [
          {
            name: "Base game",
            options: [
              option("hacan", "Hacan", { sheet: SHEET }),
              option("sol", "Sol", { intent: null, taken: "Bo" }),
            ],
          },
        ],
      },
    }),
    row("slot_2", "Bo", { faction: "Sol", sheet: { ...SHEET, title: "The Federation of Sol" } }),
  ],
  fill: null,
  footer: { note: "", main: null, nickname: false },
};

function lobby() {
  const sent: LobbyIntent[] = [];
  render(
    <LobbyScreen
      view={VIEW}
      dispatch={(intent) => sent.push(intent)}
      nickname="Ada"
      onNickname={() => {}}
      link="http://table"
      toast={null}
    />,
  );
  return sent;
}
const opener = () => screen.getByRole("button", { name: "Faction of Ada: Random" });
const editor = () => screen.queryByRole("group", { name: "Faction of Ada" });

it("the faction of the viewer opens its choice in the row, and a row sends its choice at once", () => {
  const sent = lobby();
  expect(editor()).toBeNull();
  fireEvent.click(opener());
  expect(opener()).toHaveAttribute("aria-expanded", "true");
  const list = within(editor()!);
  expect(list.getByRole("button", { name: "Random" })).toHaveAttribute("aria-pressed", "true");
  expect(
    within(list.getByRole("region", { name: "Base game" })).getAllByRole("listitem"),
  ).toHaveLength(2);
  fireEvent.click(list.getByRole("button", { name: "Hacan" }));
  expect(sent).toEqual([{ type: "faction", player: "ada", faction: "hacan" }]);
  // The choice stays open: the table says the new faction, and it can be changed again.
  expect(editor()).not.toBeNull();
  fireEvent.click(opener());
  expect(editor()).toBeNull();
});

it("a faction of another seat stays in its place, says who has it and sends nothing", () => {
  const sent = lobby();
  fireEvent.click(opener());
  const sol = within(editor()!).getByRole("button", { name: /Sol/ });
  expect(sol).toHaveAttribute("aria-disabled", "true");
  expect(sol).toHaveTextContent("Bo");
  fireEvent.click(sol);
  expect(sent).toEqual([]);
});

it("Esc closes the sheet of a faction first, then the choice", () => {
  lobby();
  fireEvent.click(opener());
  fireEvent.click(within(editor()!).getByRole("button", { name: "About Hacan" }));
  expect(screen.getByRole("note")).toHaveTextContent("The Emirates of Hacan");
  expect(screen.getByRole("note")).toHaveTextContent("Masters of Trade");
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("note")).toBeNull();
  expect(editor()).not.toBeNull();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(editor()).toBeNull();
});

it("a row that is no control has the sheet of its faction behind the name", () => {
  lobby();
  const sol = screen.getByRole("button", { name: "About Sol" });
  fireEvent.click(sol);
  expect(screen.getByRole("note")).toHaveTextContent("The Federation of Sol");
  // The row of the viewer has the control in that place, and "Random" has no sheet.
  expect(screen.queryByRole("button", { name: "About Random" })).toBeNull();
});

it("the table of a running game is read in a sheet: the seats with their signs, and the wait for the host", () => {
  const sent: LobbyIntent[] = [];
  const step = (label: string, waitMs: number) => ({
    label,
    reason: "",
    intent: { type: "set" as const, change: { waitMs } },
  });
  const view: LobbyView = {
    ...VIEW,
    title: "Table · 3 players",
    rows: [
      row("slot_1", "Ada", { player: "p1", you: true, faction: "Hacan", sheet: SHEET }),
      row("slot_2", "Bo", {
        player: "p2",
        faction: "Sol",
        sign: { kind: "away", word: "Not connected" },
        buttons: [
          {
            label: "Rejoin",
            intent: { type: "takeOver", player: "p2", name: "Bo" },
          },
        ],
      }),
      row("slot_3", "Rule bot 1", {
        player: "p3",
        faction: "Jol-Nar",
        sign: { kind: "rule", word: "Rule bot: played by the server" },
      }),
    ],
    setup: [
      { id: "map", label: "Map", value: "Standard · 37 systems", control: null, help: [] },
      {
        id: "wait",
        label: "Bots wait",
        value: "1 s",
        control: {
          kind: "counter",
          less: step("Wait less", 500),
          more: step("Wait more", 1500),
        },
        help: [],
      },
    ],
  };
  render(
    <GameProvider
      seats={{ p1: { id: "p1", name: "Ada", faction: "Hacan", symbol: "●", color: "#fff" } }}
      dispatch={() => {}}
    >
      <TableSheet view={view} dispatch={(intent) => sent.push(intent)} />
    </GameProvider>,
  );
  const rows = within(screen.getByRole("region", { name: "Players" })).getAllByRole("listitem");
  expect(rows.map((item) => item.textContent)).toEqual([
    "Ada (you)Hacan",
    "◇BoSolRejoin",
    "⚙Rule bot 1Jol-Nar",
  ]);
  expect(rows[0]).toHaveAttribute("aria-current", "true");
  // The seat of the game has its symbol, and a state has its sign with the word.
  expect(within(rows[0]).getByRole("img", { name: "Hacan" })).toBeInTheDocument();
  expect(within(rows[1]).getByRole("img", { name: "Not connected" })).toBeInTheDocument();
  expect(
    within(rows[2]).getByRole("img", { name: "Rule bot: played by the server" }),
  ).toBeInTheDocument();
  // Nothing is edited: no name, no faction, no removal.
  expect(screen.queryByRole("button", { name: /Change your name|Faction of|Remove/ })).toBeNull();
  const setup = screen.getByRole("region", { name: "Setup" });
  expect(within(setup).getByText("Standard · 37 systems")).toBeInTheDocument();
  fireEvent.click(within(setup).getByRole("button", { name: "Wait more" }));
  fireEvent.click(within(rows[1]).getByRole("button", { name: "Rejoin" }));
  expect(sent).toEqual([
    { type: "set", change: { waitMs: 1500 } },
    { type: "takeOver", player: "p2", name: "Bo" },
  ]);
});
