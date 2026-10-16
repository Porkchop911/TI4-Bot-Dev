import { describe, expect, it } from "vitest";
import { examples } from "./data";
import { EXAMPLE_IDS } from "./exampleList";
import { selectShell } from "./select/shell";
import { createWorld, currentState, loadExample, reduce, setViewer } from "./world";

/**
 * Plays the open space battle with the first choice that is allowed at every stop. `stop` sees the
 * stage before each move; true ends the play there. Returns that stage, or null when the battle
 * ended or did not end in 60 moves.
 */
function playBattle(
  world: ReturnType<typeof createWorld>,
  stop: (stage: string) => boolean = () => false,
): string | null {
  const state = currentState(world);
  for (let turn = 0; turn < 60 && state.frontier === 2; turn++) {
    const stage: string = state.battle.stage;
    if (stop(stage)) {
      return stage;
    }
    if (stage === "pre") {
      reduce(world, { type: "reaction", action: "pass" });
      reduce(world, { type: "battle", action: "roll" });
    } else if (stage === "reaction") {
      reduce(world, { type: "reaction", action: "pass" });
    } else if (stage === "retreat") {
      const [system] = Object.keys(selectShell(world).board.task!.values);
      reduce(world, { type: "toggleSystem", system });
      reduce(world, { type: "battle", action: "retreat" });
    } else if (stage === "assign") {
      const view = selectShell(world).action;
      if (view.kind !== "tactical" || view.task.content.kind !== "combat") {
        throw new Error("not in combat");
      }
      const offer = view.task.content.records
        .at(-1)!
        .table.sides.flatMap((side) => side.rows)
        .find((row) => row.assign.some((pick) => !pick.disabled));
      if (offer) {
        reduce(world, { type: "stageHit", unit: offer.unit!, kind: offer.assign[0].kind });
      } else {
        reduce(world, { type: "battle", action: "assignHits" });
      }
    } else {
      reduce(world, { type: "simulate" });
    }
    selectShell(world);
  }
  return null;
}

describe("mock engine", () => {
  it("the demo bar lists only examples that exist", () => {
    expect(EXAMPLE_IDS.filter((id) => !examples[id])).toEqual([]);
  });

  it.each(Object.keys(examples))("builds and selects %s", (example) => {
    const world = createWorld();
    loadExample(world, example);
    const view = selectShell(world);
    expect(view.board.tiles).toHaveLength(61);
    expect(view.action.title).toBeTruthy();
  });

  it.each(["hacan", "observer"])("selects every live example as %s", (viewer) => {
    const world = createWorld();
    for (const example of Object.keys(examples).filter((key) => examples[key].mode === "live")) {
      loadExample(world, example);
      setViewer(world, viewer);
      expect(selectShell(world).action.contentKey).toBeTruthy();
    }
  });

  it("applies a draft and plays space combat to the end", () => {
    const world = createWorld();
    loadExample(world, "draft-combat");
    expect(selectShell(world).apply?.items.length).toBeGreaterThan(1);
    reduce(world, { type: "confirmApply" });
    expect(world.mode).toBe("live");
    const state = currentState(world);
    expect(state.frontier).toBe(2);
    playBattle(world);
    expect(state.battle.stage).toBe("done");
  });

  it("shows no closing state in history or to another seat", () => {
    const world = createWorld();
    loadExample(world, "summary");
    expect(selectShell(world).action.closing).toHaveLength(1);
    setViewer(world, "hacan");
    expect(selectShell(world).action.closing).toHaveLength(0);
  });

  // Reaction windows of a battle.
  const toDirectHit = (world: ReturnType<typeof createWorld>) => {
    loadExample(world, "draft-combat");
    reduce(world, { type: "confirmApply" });
    return playBattle(world, (stage) => stage === "reaction") === "reaction";
  };

  it("offers Morale Boost in the same reaction window at the start of a round", () => {
    const world = createWorld();
    loadExample(world, "live-combat");
    const footer = () => {
      const view = selectShell(world).action;
      if (view.kind !== "tactical") {
        throw new Error("not tactical");
      }
      return { labels: view.task.footer.actions.map((action) => action.label), view };
    };
    expect(footer().view.interrupt?.eyebrow).toBe("Reaction · Start of combat round 1");
    expect(footer().labels).toEqual(["Pass"]);
    // The roll waits for the answer.
    const rolls = currentState(world).battle.records.length;
    reduce(world, { type: "battle", action: "roll" });
    expect(currentState(world).battle.records).toHaveLength(rolls);
    reduce(world, { type: "stageReaction", card: "Morale Boost" });
    expect(footer().labels).toEqual(["Clear choice", "Play Morale Boost"]);
    reduce(world, { type: "reaction", action: "play" });
    expect(currentState(world).hands.sol).not.toContain("Morale Boost");
    expect(currentState(world).battle.boost.att).toBe(true);
    expect(footer().view.interrupt).toBe(null);
    expect(footer().labels).toContain("Roll combat dice");
  });

  it("chooses the retreat destination on the board", () => {
    const world = createWorld();
    loadExample(world, "live-combat");
    reduce(world, { type: "reaction", action: "pass" });
    reduce(world, { type: "battle", action: "announceRetreat" });
    const state = currentState(world);
    expect(playBattle(world, (stage) => stage === "retreat")).toBe("retreat");
    const task = selectShell(world).board.task!;
    expect(task).toMatchObject({ target: "system", kind: "pick", interactive: true });
    expect(Object.keys(task.values).sort()).toEqual(["1", "23"]);
    reduce(world, { type: "battle", action: "retreat" });
    expect(state.battle.stage).toBe("retreat");
    reduce(world, { type: "toggleSystem", system: "27" });
    expect(state.battle.pick).toBe(null);
    reduce(world, { type: "toggleSystem", system: "23" });
    expect(selectShell(world).board.task!.chosen).toEqual({ "23": true });
    reduce(world, { type: "clearChoice" });
    expect(state.battle.pick).toBe(null);
    reduce(world, { type: "toggleSystem", system: "1" });
    reduce(world, { type: "battle", action: "retreat" });
    expect(state.battle.retreat.to).toBe("Jord · #1");
    expect(selectShell(world).board.task).toBe(null);
  });

  it("shows Direct Hit as the same reaction window in a battle", () => {
    const world = createWorld();
    expect(toDirectHit(world)).toBe(true);
    const view = selectShell(world).action;
    if (view.kind !== "tactical") {
      throw new Error("not tactical");
    }
    expect(view.interrupt?.eyebrow).toMatch(/^Reaction · After Alex’s .* used Sustain Damage$/);
    expect(view.task.footer.actions.map((action) => action.label)).toEqual(["Pass"]);
    reduce(world, { type: "stageReaction", card: "Direct Hit" });
    const staged = selectShell(world).action;
    if (staged.kind !== "tactical") {
      throw new Error("not tactical");
    }
    expect(staged.task.footer.actions.map((action) => action.label)).toEqual([
      "Clear choice",
      "Play Direct Hit",
    ]);
    reduce(world, { type: "reaction", action: "play" });
    expect(currentState(world).hands.sol).not.toContain("Direct Hit");
  });

  it("skips the window of a card that is set to Never offer, and says so", () => {
    const world = createWorld();
    reduce(world, { type: "setOffer", key: "Direct Hit", name: "Direct Hit", mode: "never" });
    expect(
      selectShell(world).reference.actionCards?.hand.find((item) => item.name === "Direct Hit"),
    ).toMatchObject({ offer: { mode: "never" } });
    world.toast = null;
    const toasts: string[] = [];
    loadExample(world, "draft-combat");
    reduce(world, { type: "confirmApply" });
    const collect = () => {
      const shown = world.toast as { text: string } | null;
      if (shown) {
        toasts.push(shown.text);
      }
    };
    playBattle(world, (stage) => {
      collect();
      expect(stage).not.toBe("reaction");
      return false;
    });
    collect();
    expect(toasts).toContain("Direct Hit was not offered: it is set to Never offer.");
  });
});
