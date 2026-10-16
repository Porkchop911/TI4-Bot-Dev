import { describe, expect, it } from "vitest";
import type { Catalog, Choice, SessionUpdate } from "../wire";
import { chooseScoring, clearLastScoring, selectScoring } from "./scoring";

// The questions of Imperial as the engine asks them (the scenario "strategy_imperial").
const primary: Choice = {
  player: "a",
  prompt: "score a public objective with Imperial",
  options: [
    { id: "lead", kind: "objective", label: "lead", payload: {} },
    { id: "trade_routes", kind: "objective", label: "trade_routes", payload: {} },
    { id: "decline", kind: "decline", label: "decline", payload: {} },
  ],
  details: { kind: "imperial", controls_mecatol: false, secrets_held: 1, secrets_max: 3 },
};
const window: Choice = {
  player: "b",
  prompt: "spend a strategy token to draw a secret objective",
  options: [
    { id: "no", kind: "strategy", label: "decline", payload: {} },
    { id: "yes", kind: "strategy", label: "draw", payload: {} },
  ],
  details: {
    kind: "strategy_secondary",
    secrets_held: 1,
    secrets_max: 3,
    tokens_left: 2,
    costs_token: true,
  },
};
const update = {
  viewer: { role: "player", seat: "a" },
  view: {
    players: [
      { id: "a", victory_points: 2 },
      { id: "b", victory_points: 0 },
    ],
    board: { map_tiles: [] },
  },
} as unknown as SessionUpdate;
const printed = {
  records: {
    public_objectives: [
      { id: "lead", name: "Lead From the Front", phase: "Status", points: 1, text: "Spend 3." },
      {
        id: "trade_routes",
        name: "Negotiate Trade Routes",
        phase: "Status",
        points: 1,
        text: "Spend 5 trade goods.",
      },
    ],
  },
} as unknown as Catalog;

describe("selectScoring", () => {
  it("is null for a question with no secret objectives", () => {
    expect(selectScoring(update, { ...primary, details: { kind: "imperial" } }, null)).toBeNull();
    expect(
      selectScoring(update, { ...window, details: { kind: "strategy_secondary" } }, null),
    ).toBeNull();
  });

  it("lists the objectives as printed, then what follows, and scores nothing by default", () => {
    const view = selectScoring(update, primary, null, printed)!;
    expect(view.blocks[0]).toMatchObject({
      kind: "menu",
      title: "Public objectives",
      rows: [
        { key: "1", title: "Lead From the Front", text: "Spend 3.", state: "Victory points 2 → 3" },
        { key: "2", title: "Negotiate Trade Routes", state: "Victory points 2 → 3" },
      ],
    });
    expect(view.blocks[1]).toMatchObject({
      kind: "card",
      title: "Then",
      rows: [
        {
          title: ["Draw 1 secret objective"],
          subtitle: "You do not control Mecatol Rex · Secret objectives 1 → 2 of 3",
        },
      ],
    });
    expect(view.label).toBe("Resolve primary · score nothing");
    expect(view.answers).toEqual(["decline"]);
  });

  it("counts the staged objective into the victory point of Mecatol Rex", () => {
    const mecatol = { ...primary, details: { ...primary.details, controls_mecatol: true } };
    const draft = chooseScoring(mecatol, null, "objective", "lead");
    const view = selectScoring(update, mecatol, draft, printed)!;
    expect(view.blocks[1]).toMatchObject({
      rows: [
        {
          title: ["Gain 1 victory point"],
          subtitle: "You control Mecatol Rex · Victory points 3 → 4",
        },
      ],
    });
    expect(view.note).toBe("Score Lead From the Front, then gain 1 victory point");
    expect(view.label).toBe("Resolve primary");
    expect(view.answers).toEqual(["lead"]);
    // The same objective again takes it back; Esc clears it.
    expect(chooseScoring(mecatol, draft, "objective", "lead")?.objective).toBeNull();
    expect(clearLastScoring(mecatol, draft)?.objective).toBeNull();
    expect(chooseScoring(mecatol, draft, "objective", "corner")).toBeNull();
  });

  it("is Follow or Pass in the window, with the secret objectives and the cost", () => {
    const empty = selectScoring(update, window, null)!;
    expect(empty.blocks[0]).toMatchObject({
      kind: "menu",
      rows: [
        {
          key: "1",
          title: "Draw 1 secret objective",
          state: "Secret objectives 1 → 2 of 3 · Strategy pool 2 → 1",
        },
        { key: "2", title: "Pass" },
      ],
    });
    expect(empty.problem).not.toBeNull();
    const follow = chooseScoring(window, null, "follow", "yes");
    expect(selectScoring(update, window, follow)).toMatchObject({
      label: "Follow · draw 1 secret objective",
      answers: ["yes"],
    });
    const pass = chooseScoring(window, follow, "follow", "no");
    expect(selectScoring(update, window, pass)?.answers).toEqual(["no"]);
    expect(clearLastScoring(window, pass)?.follow).toBeNull();
  });
});
