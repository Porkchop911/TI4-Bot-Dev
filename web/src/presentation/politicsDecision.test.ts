import { describe, expect, it } from "vitest";
import {
  describeAgendaPlacement,
  describePickCard,
  isPlayerPick,
  optionNote,
  playerPickSeat,
  replenishReason,
  seatStanding,
  type DecisionTable,
} from "./politicsDecision.ts";

const player = (id: string, faction: string, vp: number, commodities: number) =>
  ({
    id,
    faction,
    victory_points: vp,
    commodities,
    trade_goods: 2,
    action_cards_count: 3,
    tactic_tokens: 4,
  }) as DecisionTable["players"][number];
const table: DecisionTable = {
  players: [player("a", "sol", 3, 1), player("b", "hacan", 5, 0), player("c", "xxcha", 2, 3)],
  seating_order: ["a", "b", "c"],
  speaker: "b",
};

describe("politicsDecision", () => {
  it("describes the agenda with its text and falls back to the prompt", () => {
    const view = describeAgendaPlacement({
      prompt: "place x where",
      context: { subtype: "politics_place_agenda" } as never,
      details: { agenda: { name: "Mutiny", type: "Law", target: "For/Against", text1: "Vote.", text2: "" } },
    });
    expect(view).toMatchObject({ name: "Mutiny", kind: "Law", text: ["Vote."] });
    const bare = describeAgendaPlacement({
      prompt: "place minister_of_commerce where",
      context: { subtype: "politics_place_agenda" } as never,
    });
    expect(bare?.name).toBe("Minister Of Commerce");
    expect(describeAgendaPlacement({ prompt: "p", context: { subtype: "other" } as never })).toBeNull();
  });

  it("computes standing and speaker order from the current speaker", () => {
    expect(seatStanding(table, "c")).toMatchObject({ speakerOrder: 2, victoryPoints: 2, faction: "Xxcha" });
    expect(seatStanding(table, "b")?.isSpeaker).toBe(true);
    expect(seatStanding(table, "zz")).toBeNull();
  });

  it("notes speaker, replenish and hacan options without raw ids", () => {
    const speaker = { context: { subtype: "politics_choose_speaker" } as never, actor: "a", details: { seats: { Xxcha: "c" } } };
    const note = optionNote(speaker, { id: "Xxcha", label: "x", payload: {}, kind: "" }, table);
    expect(note?.seat).toBe("c");
    expect(note?.text).toContain("2 VP");
    const hacan = { context: { subtype: "leader_hacanagent_branch" } as never, actor: "a", options: [{ id: "self", label: "s", kind: "", payload: {} }, { id: "b", label: "b", kind: "", payload: {} }, { id: "c", label: "c", kind: "", payload: {} }] };
    expect(optionNote(hacan, hacan.options[0], table)?.text).toContain("you hold 1");
    expect(optionNote(hacan, hacan.options[1], table)?.text).toContain("0 commodities held");
    expect(replenishReason(hacan, table)).toContain("holds 0");
    const trade = { context: { subtype: "trade_choose_replenish" } as never, actor: "a", details: { seats: {} } };
    expect(optionNote(trade, { id: "done", label: "d", kind: "decline", payload: {} }, table)?.text).toMatch(/Nobody/);
  });

  describe("player picks (Spy, Insubordination, Signal Jamming ...)", () => {
    const pick = (subtype: string) => ({
      context: { subtype, source: { ActionCard: "spy" } } as never,
      actor: "a",
    });

    it("recognises a card asking for a player", () => {
      expect(isPlayerPick(pick("spy_pick_player"))).toBe(true);
      expect(isPlayerPick(pick("politics_choose_speaker"))).toBe(false);
    });

    it("resolves a seat id or the faction a seat plays (Signal Jamming names factions)", () => {
      expect(playerPickSeat({ id: "b", label: "" }, table)?.id).toBe("b");
      expect(playerPickSeat({ id: "Hacan", label: "" }, table)?.id).toBe("b");
      expect(playerPickSeat({ id: "nobody", label: "" }, table)).toBeUndefined();
    });

    it("notes standing, with what Spy and Insubordination take", () => {
      const spy = optionNote(pick("spy_pick_player"), { id: "b", label: "", payload: {}, kind: "" }, table);
      expect(spy?.seat).toBe("b");
      expect(spy?.text).toContain("5 VP, 2 trade goods, 0 commodities");
      expect(spy?.text).toContain("Holds 3 action cards; you take one at random");
      const insub = optionNote(pick("insubordination_pick_player"), { id: "c", label: "", payload: {}, kind: "" }, table);
      expect(insub?.text).toContain("Has 4 tactic tokens; you take one");
      const jam = optionNote(pick("jamming_pick_player"), { id: "Xxcha", label: "", payload: {}, kind: "" }, table);
      expect(jam?.seat).toBe("c");
      expect(jam?.text).not.toContain("take");
      expect(optionNote(pick("spy_pick_player"), { id: "b", label: "", payload: {}, kind: "" }, null)).toBeNull();
    });

    it("notes the real alias of Insubordination (insub) as well as the long name", () => {
      const insub = optionNote(pick("insub_pick_player"), { id: "c", kind: "player", label: "", payload: {} }, table);
      expect(insub?.text).toContain("Has 4 tactic tokens; you take one");
    });

    it("tells Ancient Burial Sites which cultural planets would be exhausted", () => {
      const withBoard: DecisionTable = {
        ...table,
        board: {
          systems: {
            "27": {
              system_id: "27",
              command_tokens: [],
              units: [],
              planets: {
                lodor: { planet_id: "lodor", controlled_by: "c", exhausted: true },
                quann: { planet_id: "quann", controlled_by: "c", exhausted: false },
                vefut: { planet_id: "vefut", controlled_by: "a", exhausted: false },
              },
            },
          },
          map_tiles: [
            {
              system_id: "27",
              label: "x",
              q: 0,
              r: 0,
              planets: [
                { id: "lodor", label: "Lodor", resources: 3, influence: 1, traits: ["cultural"] },
                { id: "quann", label: "Quann", resources: 2, influence: 1, traits: ["cultural"] },
                { id: "vefut", label: "Vefut II", resources: 2, influence: 2, traits: ["hazardous"] },
              ],
            },
          ],
        },
      };
      const note = optionNote(pick("abs_pick_player"), { id: "c", kind: "player", label: "", payload: {} }, withBoard);
      expect(note?.text).toContain("Exhausts 2 cultural planets: Lodor, Quann (1 already exhausted, so 1 newly)");
      expect(optionNote(pick("abs_pick_player"), { id: "a", kind: "player", label: "", payload: {} }, withBoard)?.text).toContain("no cultural planet");
    });

    it("tells Forward Supply Base, Seize Artifact and Exchange Program what they do to a player", () => {
      const rich: DecisionTable = {
        ...table,
        players: table.players.map((p) =>
          p.id === "b" ? { ...p, relic_fragments: { cultural: 2, hazardous: 1 }, fleet_tokens: 3 } : { ...p, fleet_tokens: 1 },
        ),
      };
      expect(optionNote(pick("fsb_pick_player"), { id: "b", kind: "player", label: "", payload: {} }, rich)?.text).toContain("2 now, 3 after");
      const seize = optionNote(pick("seize_pick_player"), { id: "b", kind: "player", label: "", payload: {} }, rich)?.text;
      expect(seize).toContain("Holds 3 relic fragments (2 cultural, 1 hazardous)");
      expect(optionNote(pick("seize_pick_player"), { id: "c", kind: "player", label: "", payload: {} }, rich)?.text).toContain("no relic fragments");
      expect(optionNote(pick("exchangeprogram_pick_player"), { id: "b", kind: "player", label: "", payload: {} }, rich)?.text).toContain(
        "Fleet pool 3 against your 1",
      );
    });

    it("shows the printed card above the options", () => {
      const card = describePickCard(pick("spy_pick_player"));
      expect(card?.name).toBe("Spy");
      expect(card?.text).not.toBe("");
      expect(describePickCard({ context: { subtype: "spy_pick_player" } as never })).toBeNull();
    });
  });
});
