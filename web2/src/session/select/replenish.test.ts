import { describe, expect, it } from "vitest";
import type { Choice, SessionUpdate } from "../wire";
import {
  chooseReplenish,
  clearLastReplenish,
  clearReplenishPart,
  selectReplenish,
  toggleReplenishSeat,
} from "./replenish";

// The questions of Trade as the engine asks them (the scenario "strategy_trade"): Sol gained 3
// trade goods and replenished; Hacan and Letnev can replenish, by the faction names of the options.
const primary: Choice = {
  player: "a",
  prompt: "let another player replenish commodities",
  options: [
    { id: "Hacan", kind: "replenish", label: "Hacan replenishes commodities", payload: {} },
    { id: "Letnev", kind: "replenish", label: "Letnev replenishes commodities", payload: {} },
    { id: "done", kind: "decline", label: "nobody else replenishes", payload: {} },
  ],
  details: {
    seats: { Hacan: "b", Letnev: "c" },
    commodities: { Hacan: { have: 0, max: 6 }, Letnev: { have: 1, max: 2 } },
    own: {
      trade_goods: { before: 4, after: 7 },
      commodities: { before: 1, after: 4 },
    },
  },
};
const window: Choice = {
  player: "b",
  prompt: "spend a strategy token to replenish commodities",
  options: [
    { id: "no", kind: "strategy", label: "decline", payload: {} },
    { id: "yes", kind: "strategy", label: "replenish", payload: {} },
  ],
  details: {
    kind: "strategy_secondary",
    commodities: { have: 0, max: 6, trade_goods: 3 },
    tokens_left: 2,
    costs_token: true,
  },
};
const update = {
  viewer: { role: "player", seat: "a" },
  seats: {
    a: { nickname: "Sol" },
    b: { nickname: "Hacan" },
    c: { nickname: "Letnev" },
  },
  view: { players: [{ id: "a" }, { id: "b" }, { id: "c" }], board: { map_tiles: [] } },
} as unknown as SessionUpdate;

describe("selectReplenish", () => {
  it("is null for a question with no commodities", () => {
    expect(selectReplenish(update, { ...primary, details: { seats: {} } }, null)).toBeNull();
    // The speaker of Politics has seats and no commodities.
    expect(
      selectReplenish(
        update,
        {
          ...primary,
          options: [{ id: "Hacan", kind: "speaker", label: "Hacan", payload: {} }],
          details: { seats: { Hacan: "b" } },
        },
        null,
      ),
    ).toBeNull();
    expect(
      selectReplenish(update, { ...window, details: { kind: "strategy_secondary" } }, null),
    ).toBeNull();
  });

  it("shows the own gain, and chooses the other seats in the player table", () => {
    const empty = selectReplenish(update, primary, null)!;
    expect(empty.blocks[0]).toMatchObject({
      kind: "card",
      title: "Your seat",
      rows: [
        { title: ["Gain 3 trade goods"], subtitle: "Trade goods 4 → 7" },
        { title: ["Replenish commodities"], subtitle: "Commodities 1 → 4" },
      ],
    });
    expect(empty.blocks[1]).toMatchObject({
      title: "Other players",
      aside: "0 / 2 chosen · Choose in the player table",
      rows: [{ title: ["No player chosen"] }],
    });
    expect(empty.pick).toEqual({
      verb: "Choose",
      seats: {
        b: { reason: null, note: "Commodities 0 → 6" },
        c: { reason: null, note: "Commodities 1 → 2" },
      },
      chosen: [],
    });
    expect(empty.label).toBe("Resolve primary · nobody else");
    expect(empty.answers).toEqual(["done"]);
    expect(empty.changed).toBe(false);

    const one = toggleReplenishSeat(primary, null, "c")!;
    const letnev = selectReplenish(update, primary, one)!;
    expect(letnev.pick?.chosen).toEqual(["c"]);
    expect(letnev.blocks[1]).toMatchObject({
      aside: "1 / 2 chosen · Choose in the player table",
      rows: [{ subtitle: "Commodities 1 → 2 · no strategy token" }],
    });
    expect(letnev.note).toBe("Letnev replenishes commodities");
    // A seat that is not chosen is declined after the chosen ones.
    expect(letnev.answers).toEqual(["Letnev", "done"]);

    // Every seat chosen: the game asks no more, so nothing is declined; the order is the options'.
    const both = toggleReplenishSeat(primary, one, "b")!;
    const all = selectReplenish(update, primary, both)!;
    expect(all.answers).toEqual(["Hacan", "Letnev"]);
    expect(all.label).toBe("Resolve primary");
    expect(all.note).toBe("Hacan and Letnev replenish commodities");

    // A seat that the question does not offer is not a choice.
    expect(toggleReplenishSeat(primary, null, "a")).toBeNull();
    // The same seat again gives it back.
    expect(toggleReplenishSeat(primary, one, "c")).toEqual({ seats: [], follow: null });
  });

  it("gives a seat back with Change, and Esc clears the last seat", () => {
    const both = { seats: ["Letnev", "Hacan"], follow: null };
    expect(clearReplenishPart(primary, both, "seat:Letnev")).toEqual({
      seats: ["Hacan"],
      follow: null,
    });
    expect(clearReplenishPart(primary, both, "seat:Sol")).toBeNull();
    expect(clearLastReplenish(primary, both)).toEqual({ seats: ["Letnev"], follow: null });
    expect(clearLastReplenish(primary, { seats: [], follow: null })).toBeNull();
  });

  it("answers the window of the secondary with Follow or Pass", () => {
    const empty = selectReplenish(update, window, null)!;
    expect(empty.pick).toBeNull();
    expect(empty.blocks[0]).toMatchObject({
      kind: "menu",
      rows: [
        {
          key: "1",
          title: "Replenish commodities",
          state: "Commodities 0 → 6 · Strategy pool 2 → 1",
          selected: false,
        },
        { key: "2", title: "Pass", selected: false },
      ],
    });
    expect(empty.problem).not.toBeNull();
    expect(empty.answers).toBeNull();

    const follow = chooseReplenish(window, null, "follow", "yes")!;
    const followed = selectReplenish(update, window, follow)!;
    expect(followed.label).toBe("Follow · replenish commodities");
    expect(followed.answers).toEqual(["yes"]);
    const pass = chooseReplenish(window, follow, "follow", "no")!;
    expect(selectReplenish(update, window, pass)!.answers).toEqual(["no"]);
    expect(selectReplenish(update, window, pass)!.label).toBe("Pass");
    // The same answer again takes it back; Esc clears it.
    expect(chooseReplenish(window, pass, "follow", "no")).toEqual({ seats: [], follow: null });
    expect(clearLastReplenish(window, follow)).toEqual({ seats: [], follow: null });
    // The primary has no window.
    expect(chooseReplenish(primary, null, "follow", "yes")).toBeNull();
  });

  it("says that a free secondary spends no strategy token", () => {
    const free = { ...window, details: { ...window.details, costs_token: false } };
    expect(selectReplenish(update, free, null)!.blocks[0]).toMatchObject({
      rows: [{ state: "Commodities 0 → 6 · no strategy token" }, {}],
    });
  });
});
