import { describe, expect, it } from "vitest";
import type { BoardView, GameEvent, TableView } from "../protocol/types.ts";
import { imperialNoScoreNotice, lostHomePlanets, ownImperialPlay } from "./publicScoringBlock.ts";

const board: BoardView = {
  systems: {
    "14": {
      system_id: "14",
      command_tokens: [],
      units: [],
      planets: {
        archonren: { planet_id: "archonren", controlled_by: "letnev", exhausted: true },
        archontau: { planet_id: "archontau", controlled_by: "xxcha", exhausted: false },
      },
    },
  },
};

const table = (over: Partial<TableView> = {}): TableView => ({
  revealed_objectives: ["push_boundaries", "unify_colonies", "corner"],
  scored_objectives: { xxcha: ["corner"] },
  objective_progress: {
    xxcha: {
      push_boundaries: { have: 3, threshold: 2, satisfied: true },
      unify_colonies: { have: 6, threshold: 6, satisfied: true },
    },
  },
  unclaimed_strategy_cards: [],
  strategy_card_goods: {},
  laws: {},
  ...over,
});

const label = (seat: string) => seat.charAt(0).toUpperCase() + seat.slice(1);

const played = (actor: string, detail: string): GameEvent => ({
  id: "e1",
  timestamp: "05:35:08",
  visibility: { visibility: "public" },
  event: { kind: "decision_resolved" },
  actor,
  stage: "action selection",
  detail,
} as GameEvent);

describe("publicScoringBlock", () => {
  it("names the lost home planet and its holder", () => {
    expect(lostHomePlanets(table({ home_system_lost: { xxcha: ["archonren"] } }), board, "xxcha")).toEqual([
      { planet: "archonren", name: "Archon Ren", holder: "letnev" },
    ]);
    expect(lostHomePlanets(table(), board, "xxcha")).toEqual([]);
  });

  it("finds only the viewer's own Imperial play", () => {
    expect(ownImperialPlay([played("xxcha", "xxcha played Imperial")], "xxcha")?.id).toBe("e1");
    expect(ownImperialPlay([played("sol", "sol played Imperial")], "xxcha")).toBeNull();
    expect(ownImperialPlay([played("xxcha", "xxcha played Warfare")], "xxcha")).toBeNull();
  });

  it("game 8261fb3f: met objectives, but Archon Ren is Letnev's", () => {
    expect(
      imperialNoScoreNotice({
        viewerSeat: "xxcha",
        table: table({ home_system_lost: { xxcha: ["archonren"] } }),
        board,
        label,
      }),
    ).toBe(
      "No public objective could be scored: you do not control Archon Ren (held by Letnev) in your home system (rule 61.16).",
    );
  });

  it("lists the progress when nothing is met, and says nothing when something is", () => {
    const unmet = table({
      objective_progress: {
        xxcha: {
          push_boundaries: { have: 1, threshold: 2, satisfied: false },
          unify_colonies: { have: 5, threshold: 6, satisfied: false },
        },
      },
    });
    expect(imperialNoScoreNotice({ viewerSeat: "xxcha", table: unmet, board, label })).toMatch(
      /^No public objective could be scored: none is met \(.+ 1\/2, .+ 5\/6\)\.$/,
    );
    expect(imperialNoScoreNotice({ viewerSeat: "xxcha", table: table(), board, label })).toBeNull();
  });

  it("stays quiet while the viewer is being asked which objective to score", () => {
    expect(
      imperialNoScoreNotice({
        viewerSeat: "xxcha",
        table: table({ home_system_lost: { xxcha: ["archonren"] } }),
        board,
        label,
        pendingChoice: { actor: "xxcha", context: { subtype: "imperial_score_objective" } } as never,
      }),
    ).toBeNull();
  });
});
