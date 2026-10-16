import { describe, it, expect } from "vitest";
import type { BoardView, GameEvent } from "../protocol/types.ts";
import {
  actionToastFromEvent,
  actionToastText,
  autoResolvedToast,
  isCardOrScore,
  mergeActionToast,
  victoryPointToasts,
} from "./actionToasts.ts";

const entry = (over: Partial<GameEvent> & { detail?: string }): GameEvent =>
  ({
    id: over.id ?? "e1",
    timestamp: "10:00",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    actor: "p2",
    ...over,
  }) as GameEvent;

describe("actionToastFromEvent", () => {
  it("names another player's card play, activation, research and scoring", () => {
    expect(actionToastFromEvent(entry({ detail: "p2 played Sabotage" }), "p1")).toMatchObject({
      actor: "p2",
      category: "card",
      parts: ["played Sabotage"],
    });
    expect(actionToastFromEvent(entry({ detail: "p2 activated #22" }), "p1")?.parts).toEqual([
      "activated system #22",
    ]);
    expect(actionToastFromEvent(entry({ detail: "p2 researched Neural Motivator" }), "p1")?.category).toBe(
      "tech",
    );
    expect(
      actionToastFromEvent(entry({ detail: "p2 scored public objective Corner the Market with Imperial" }), "p1")
        ?.category,
    ).toBe("score");
  });

  it("names activated and moved-between systems with tile number and name when the board knows them", () => {
    const board = {
      map_tiles: [
        { system_id: "1", label: "Jord - Sol", q: 0, r: 0, planets: [{ id: "jord" }] },
        { system_id: "18", label: "Mecatol Rex", q: 1, r: 0, planets: [{ id: "mecatol_rex" }] },
      ],
    } as unknown as BoardView;
    expect(actionToastFromEvent(entry({ detail: "p2 activated #18" }), "p1", board)?.parts).toEqual([
      "activated system #18 [Mecatol Rex]",
    ]);
    expect(
      actionToastFromEvent(entry({ detail: "p2 moved cruiser from #1 to #18" }), "p1", board)?.parts,
    ).toEqual(["moved cruiser from #01 [Sol home system] to #18 [Mecatol Rex]"]);
  });

  it("maps the action selection to a turn toast", () => {
    const e = entry({ stage: "action selection", action_type: "pass" });
    expect(actionToastFromEvent(e, "p1")?.parts).toEqual(["passed"]);
    expect(
      actionToastFromEvent(entry({ stage: "action selection", action_type: "tactical" }), "p1")?.parts,
    ).toEqual(["started a tactical action"]);
  });

  it("never toasts the viewer's own actions", () => {
    expect(actionToastFromEvent(entry({ detail: "p2 played Sabotage" }), "p2")).toBeNull();
  });

  it("toasts every player for a spectator", () => {
    expect(actionToastFromEvent(entry({ detail: "p2 played Sabotage" }), undefined)).not.toBeNull();
    expect(actionToastFromEvent(entry({ detail: "p2 played Sabotage" }), null)).not.toBeNull();
  });

  it("ignores private, referee, non-decision and bookkeeping entries", () => {
    expect(
      actionToastFromEvent(entry({ visibility: { visibility: "seat", seat: "p1" }, detail: "p2 played Sabotage" } as never), "p1"),
    ).toBeNull();
    expect(actionToastFromEvent(entry({ visibility: { visibility: "referee" }, detail: "p2 played X" } as never), "p1")).toBeNull();
    expect(
      actionToastFromEvent(entry({ event: { kind: "phase_transition", phase: "status", round: 2 } as never }), "p1"),
    ).toBeNull();
    for (const detail of ["p2 spent a trade good", "p2 exhausted jord", "Done moving", "p2 lost a cruiser in #22"])
      expect(actionToastFromEvent(entry({ detail }), "p1")).toBeNull();
    expect(actionToastFromEvent(entry({}), "p1")).toBeNull();
    expect(actionToastFromEvent(entry({ actor: undefined, detail: "p2 played X" }), "p1")).toBeNull();
  });
});

describe("mergeActionToast", () => {
  const t = (id: string, actor: string, category: "card" | "move" | "produce", part: string, count = 1) => ({
    id,
    actor,
    category,
    parts: [part],
    count,
  });

  it("merges a burst by the same player and category into one toast", () => {
    let list = mergeActionToast([], t("a", "p2", "move", "moved cruiser from #1 to #22"));
    list = mergeActionToast(list, t("b", "p2", "move", "moved cruiser from #1 to #22"));
    list = mergeActionToast(list, t("c", "p2", "move", "moved carrier from #3 to #22"));
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe("a");
    expect(actionToastText(list[0])).toBe("moved 3 units");
  });

  it("sums produced units and counts repeated cards", () => {
    const produce = mergeActionToast(
      mergeActionToast([], t("a", "p2", "produce", "produced 2 fighter", 2)),
      t("b", "p2", "produce", "produced 1 cruiser", 1),
    );
    expect(actionToastText(produce[0])).toBe("produced 3 units");
    const cards = mergeActionToast(
      mergeActionToast([], t("a", "p2", "card", "played Sabotage")),
      t("b", "p2", "card", "played Sabotage"),
    );
    expect(actionToastText(cards[0])).toBe("played Sabotage ×2");
  });

  it("keeps different players apart and limits the stack, dropping the oldest", () => {
    let list = mergeActionToast([], t("a", "p2", "card", "played A"));
    list = mergeActionToast(list, t("b", "p3", "card", "played B"));
    list = mergeActionToast(list, t("c", "p4", "card", "played C"));
    list = mergeActionToast(list, t("d", "p5", "card", "played D"));
    expect(list.map((x) => x.id)).toEqual(["b", "c", "d"]);
  });

  it("summarises a long list of different cards", () => {
    let list = mergeActionToast([], t("a", "p2", "card", "played A"));
    for (const [i, n] of ["B", "C", "D", "E"].entries())
      list = mergeActionToast(list, t(`x${i}`, "p2", "card", `played ${n}`));
    expect(actionToastText(list[0])).toBe("played A; played B; played C; and 2 more");
  });
});

describe("victoryPointToasts", () => {
  it("reports other players' gains only", () => {
    const before = [
      { id: "p1", victory_points: 1 },
      { id: "p2", victory_points: 2 },
      { id: "p3", victory_points: 0 },
    ];
    const after = [
      { id: "p1", victory_points: 2 },
      { id: "p2", victory_points: 3 },
      { id: "p3", victory_points: 0 },
    ];
    const toasts = victoryPointToasts(before, after, "p1");
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ actor: "p2", category: "score", parts: ["gained 1 VP (now 3)"] });
  });
});

describe("autoResolvedToast", () => {
  const choice = (auto: boolean, extra = 0) => ({
    prompt: "Strategy Card",
    actor: "p1",
    nonce: "n1",
    options: [
      { id: "technology", label: "Technology", kind: "strategy_card", auto_resolved: auto },
      ...Array.from({ length: extra }, (_, i) => ({ id: `o${i}`, label: "x", kind: "k" })),
    ],
  });
  it("reports the viewer's single auto-resolved option", () => {
    expect(autoResolvedToast(choice(true), "p1")).toEqual({
      decisionType: "Strategy Card",
      selectedValue: "Technology",
    });
  });
  it("stays quiet for other seats, several options, or an unflagged option", () => {
    expect(autoResolvedToast(choice(true), "p2")).toBeNull();
    expect(autoResolvedToast(choice(true, 1), "p1")).toBeNull();
    expect(autoResolvedToast(choice(false), "p1")).toBeNull();
    expect(autoResolvedToast(null, "p1")).toBeNull();
  });
});

describe("scoring and card toasts", () => {
  it("says a scored objective by its name rather than its id", () => {
    const toast = actionToastFromEvent(entry({ detail: "p2 scored objective expand_borders" }), "p1");
    expect(toast?.category).toBe("score");
    expect(toast?.parts[0]).not.toContain("expand_borders");
  });

  it("recognises card plays and scoring for the always-immediate path, but not the turn's own selection", () => {
    expect(isCardOrScore(entry({ detail: "p2 played Sabotage", stage: "reactions" }))).toBe(true);
    expect(isCardOrScore(entry({ detail: "p2 scored objective te", stage: "status" }))).toBe(true);
    expect(isCardOrScore(entry({ detail: "p2 played Construction", stage: "action selection" }))).toBe(false);
    expect(isCardOrScore(entry({ detail: "p2 moved carrier from #1 to #2" }))).toBe(false);
  });
});
