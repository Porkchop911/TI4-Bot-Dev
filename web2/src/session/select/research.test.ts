import { expect, it } from "vitest";
import type { Choice, SessionUpdate } from "../wire";
import {
  payResearch,
  researchPlan,
  selectResearch,
  setResearchGoods,
  toggleResearchPlanet,
} from "./research";

// A paid research as the engine asks it: the price on each option, and what can pay it in
// `details.payment`, with the specialty planet that Sarween Tools keeps for its prerequisite.
const research = (payment: object | null): Choice => ({
  player: "a",
  prompt: "research a technology",
  options: [
    { id: "gd", kind: "research", label: "Gravity Drive", payload: { cost: 6 } },
    { id: "st", kind: "research", label: "Sarween Tools", payload: { cost: 6 } },
    { id: "decline", kind: "decline", label: "decline", payload: {} },
  ],
  details: payment ? { payment } : {},
});
const PAYMENT = {
  cost: 6,
  planets: [
    { id: "jord", worth: 4 },
    { id: "qucenn", worth: 1 },
    { id: "rarron", worth: 0 },
    { id: "wellon", worth: 1 },
  ],
  trade_goods: 4,
  trade_good_worth: 1,
  kept: { st: ["wellon"] },
};
const update = {
  view: {
    board: {
      map_tiles: [
        { system_id: "01", planets: [{ id: "jord", label: "Jord" }] },
        {
          system_id: "29",
          planets: [
            { id: "qucenn", label: "Qucen'n" },
            { id: "rarron", label: "Rarron" },
          ],
        },
        { system_id: "30", planets: [{ id: "wellon", label: "Wellon" }] },
      ],
    },
    players: [{ id: "a", faction: "sol" }],
  },
  research: {
    technologies: [
      {
        id: "gd",
        colour: "B",
        prerequisites: ["B"],
        unit_upgrade: false,
        faction: false,
        owned: false,
      },
      {
        id: "st",
        colour: "Y",
        prerequisites: ["Y"],
        unit_upgrade: false,
        faction: false,
        owned: false,
      },
      {
        id: "amd",
        colour: "B",
        prerequisites: [],
        unit_upgrade: false,
        faction: false,
        owned: true,
      },
    ],
    specialties: [],
  },
} as unknown as SessionUpdate;

it("pays a paid research on the map once its technology is chosen", () => {
  const choice = research(PAYMENT);
  expect(selectResearch(update, choice, null, null, null)?.payment).toBeNull();
  const chosen = selectResearch(update, choice, "gd", null, null)!;
  expect(chosen.task?.values).toEqual({ jord: 4, qucenn: 1, rarron: 0, wellon: 1 });
  expect(chosen.payment).toMatchObject({ cost: 6, paid: 0, unit: "resources" });
  expect(chosen.problem).toBe("Pay 6 more resources.");
  expect(researchPlan(choice, "gd", null)).toBeNull();

  const jord = toggleResearchPlanet(choice, "gd", null, "jord")!;
  const paying = setResearchGoods(choice, "gd", jord, 2)!;
  const paid = selectResearch(update, choice, "gd", null, paying)!;
  expect(paid.payment).toMatchObject({ paid: 6, summary: "Jord 4 + 2 trade goods" });
  expect(paid.problem).toBeNull();
  expect(paid.change).toBe("Technologies 1 → 2 · Trade goods 4 → 2");
  expect(researchPlan(choice, "gd", paying)).toEqual([
    { kind: "pay", source: "exhaust|jord" },
    { kind: "pay", source: "trade_good" },
    { kind: "pay", source: "trade_good" },
  ]);
  // More than the price is allowed.
  const more = setResearchGoods(choice, "gd", jord, 4)!;
  expect(selectResearch(update, choice, "gd", null, more)?.payment?.paid).toBe(8);
  expect(researchPlan(choice, "gd", more)).toHaveLength(5);
});

it("keeps the specialty planet of a prerequisite out of the payment of its technology", () => {
  const choice = research(PAYMENT);
  const view = selectResearch(update, choice, "st", null, null)!;
  expect(Object.keys(view.task!.values)).not.toContain("wellon");
  expect(toggleResearchPlanet(choice, "st", null, "wellon")).toBeNull();
  // A planet staged for another technology does not pay for this one.
  const staged = toggleResearchPlanet(choice, "gd", null, "wellon")!;
  expect(selectResearch(update, choice, "st", null, staged)?.payment?.paid).toBe(0);
});

it("auto-pays with the least waste, then clears", () => {
  const choice = research(PAYMENT);
  const auto = payResearch(choice, "gd", null, "auto")!;
  expect(selectResearch(update, choice, "gd", null, auto)?.payment?.paid).toBe(6);
  expect(payResearch(choice, "gd", auto, "reset")).toEqual({ planets: [], goods: 0 });
});

it("has no payment for a free research, nor in a draft", () => {
  const free = research(null);
  expect(selectResearch(update, free, "gd", null, null)?.payment).toBeNull();
  expect(researchPlan(free, "gd", null)).toBeNull();
  const recorded = selectResearch(update, research(PAYMENT), "gd", null)!;
  expect(recorded.payment).toBeNull();
  expect(recorded.problem).toBeNull();
});
