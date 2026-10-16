import { describe, expect, it } from "vitest";
import { examples } from "./data";
import { EXAMPLE_IDS } from "./exampleList";
import { selectShell } from "./select/shell";
import { shortcuts } from "../model";
import { E } from "./loose";
import { createWorld, currentState, loadExample, reduce, setViewer } from "./world";

/** The blocks of an action; for a strategy card, the editor in the open row of the viewer. */
function editorBlocks(action: ReturnType<typeof selectShell>["action"]) {
  if (action.kind !== "flow") {
    throw new Error("not a flow");
  }
  const list = action.blocks[0];
  if (list?.kind !== "seats") {
    return action.blocks;
  }
  return list.rows.find((row) => row.open)?.open ?? [];
}

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
        reduce(world, { type: "stageHit", unit: offer.unit, kind: offer.assign[0].kind });
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

  it("records every change of the movement in a draft, and undoes one change", () => {
    const world = createWorld();
    loadExample(world, "draft-movement");
    // The movement of the example is recorded as it stands: the draft is past the step.
    expect(currentState(world)).toMatchObject({ selected: 1, done: { 1: true } });
    expect(currentState(world).frontier).not.toBe(1);
    reduce(world, { type: "setCount", key: "j-carrier#0", value: 0 });
    const state = currentState(world);
    expect(state.edit).toMatchObject({ step: 1, dirty: false });
    expect(state.data.movement["j-carrier#0"]).toBe(0);
    expect(state.selected).toBe(1);
    // Nothing to send and nothing to reset: the footer is empty.
    const action = selectShell(world).action;
    expect(action.kind === "tactical" && action.task.footer).toMatchObject({
      note: "",
      actions: [],
    });
    reduce(world, { type: "undo" });
    expect(currentState(world).data.movement["j-carrier#0"]).toBe(1);
    expect(selectShell(world).toolbar.draft?.canRedo).toBe(true);
    // The open movement holds nothing back: the next step opens when the player goes there.
    reduce(world, { type: "selectStep", step: currentState(world).frontier });
    expect(currentState(world).edit?.step).not.toBe(1);
  });

  describe("movement in the shape of the game", () => {
    const movementOf = (world: ReturnType<typeof createWorld>) => {
      const action = selectShell(world).action;
      if (action.kind !== "tactical" || action.task.content.kind !== "movement") {
        throw new Error("the movement step is not open");
      }
      return action.task.content;
    };
    const cases = () => {
      const world = createWorld();
      loadExample(world, "draft-movement-cases");
      return world;
    };
    const ship = (world: ReturnType<typeof createWorld>, key: string) =>
      movementOf(world)
        .origins.flatMap((origin) => origin.ships)
        .find((row) => row.key === key)!;
    /** One ship of a line: "l-carrier#0". */
    const unit = (world: ReturnType<typeof createWorld>, key: string) =>
      ship(world, key.split("#")[0]).units.find((row) => row.key === key)!;
    const origin = (world: ReturnType<typeof createWorld>, system: string) =>
      movementOf(world).origins.find((row) => row.system === system)!;

    it("gives Gravity Drive to one ship: the other shows who has it", () => {
      const world = cases();
      expect(unit(world, "l-carrier#0")).toMatchObject({
        moves: true,
        boost: { on: true, locked: true },
      });
      expect(unit(world, "l-dreadnought#0")).toMatchObject({ moves: false, canMove: false });
      expect(unit(world, "l-dreadnought#0").reason).toContain("the carrier from Lodor has it");
      // The player chooses the ship by taking the bonus from the other.
      reduce(world, { type: "setCount", key: "l-carrier#0", value: 0 });
      expect(unit(world, "l-dreadnought#0")).toMatchObject({ canMove: true, reason: null });
    });

    it("lets a ship take Gravity Drive in place of the gravity rift", () => {
      const world = cases();
      // The carrier of Vefut reaches Starpoint through the rift, with a roll for it and its hold.
      expect(unit(world, "v-carrier#0")).toMatchObject({
        moves: true,
        riftRoll: true,
        boost: { on: false, locked: false },
      });
      expect(unit(world, "v-carrier#0").route).toContain("#41");
      expect(unit(world, "v-carrier#0").boost!.reason).toContain("the carrier from Lodor has it");
      const rolls = movementOf(world).riftRolls;
      // With Gravity Drive it goes round the rift. The carrier of Lodor needed it, and stays.
      reduce(world, { type: "setBoost", ship: "v-carrier#0" });
      expect(unit(world, "v-carrier#0")).toMatchObject({ riftRoll: false, boost: { on: true } });
      expect(unit(world, "v-carrier#0").route).toContain("#19");
      expect(unit(world, "l-carrier#0").moves).toBe(false);
      expect(movementOf(world).riftRolls).toBe(rolls - 1);
      // On its new way it passes Wellon: the fighters there are a pickup now.
      expect(origin(world, "31").cargo.find((row) => row.id === "w-fighter")?.site?.system).toBe(
        "19",
      );
      reduce(world, { type: "setBoost", ship: null });
      expect(unit(world, "v-carrier#0")).toMatchObject({ riftRoll: true, boost: { on: false } });
      // A ship that does not need it and cannot use it has no toggle.
      expect(unit(world, "v-destroyer#0").boost).toBeNull();
    });

    it("keeps cargo in the hold of the ship that carries it", () => {
      const world = cases();
      const hold = unit(world, "l-carrier#0").hold!;
      expect(hold).toMatchObject({ capacity: 4, loaded: 3 });
      expect(hold.slots.filter((slot) => slot.site).map((slot) => slot.site)).toEqual([
        "Tar’Mann · #23",
      ]);
      // The units of the system first, then the pickups on the way of its ships.
      expect(origin(world, "26").cargo.map((row) => [row.id, row.site?.system ?? null])).toEqual([
        ["l-fighter", null],
        ["l-infantry", null],
        ["t-infantry", "23"],
        ["s-fighter", "38"],
      ]);
      // The ship stays: what it had loaded stays too.
      reduce(world, { type: "setCount", key: "l-carrier#0", value: 0 });
      expect(unit(world, "l-carrier#0").hold).toBeNull();
      expect(movementOf(world).arrival.find((row) => row.unit === "infantry")?.after).toBe(4);
      // "Fill" loads one kind of unit, until the holds are full or none is left.
      reduce(world, { type: "setCount", key: "l-carrier#0", value: 1 });
      reduce(world, { type: "fillHold", key: "origin:26", source: "l-infantry" });
      expect(unit(world, "l-carrier#0").hold!.slots.map((slot) => slot.unit)).toEqual([
        "infantry",
        "infantry",
      ]);
      reduce(world, { type: "fillHold", key: "origin:26", source: "l-fighter" });
      expect(unit(world, "l-carrier#0").hold).toMatchObject({ loaded: 4, accepts: {} });
    });

    it("has one path for a ship, and no pickup where the command token is", () => {
      const world = cases();
      reduce(world, { type: "setCount", key: "l-carrier#0", value: 0 });
      reduce(world, { type: "setCount", key: "l-dreadnought#0", value: 1 });
      expect(unit(world, "l-dreadnought#0").route).toContain("#38");
      const site = origin(world, "26").cargo.find((row) => row.site?.system === "38")!;
      expect(site.reason).toContain("command token");
      expect(unit(world, "l-dreadnought#0").hold!.accepts[site.id]).toBeUndefined();
    });

    it("names what cannot move, and shows all of it on the board", () => {
      const world = cases();
      const view = movementOf(world);
      const texts = view.unreachable!.rows.map((row) => `${row.name} · ${row.text}`);
      expect(texts.find((text) => text.includes("#19"))).toContain("No capacity on the Cruiser II");
      expect(texts.find((text) => text.includes("#36"))).toContain("No ship with capacity passes");
      expect(view.excessShips).toBeGreaterThan(0);
      expect(view.riftRolls).toBe(2);
      const board = selectShell(world).board;
      const tile = (id: string) => board.tiles.find((item) => item.id === id)!;
      // What leaves and what arrives, as "now → after" on the marks of the player.
      expect(tile("1").fleets.find((fleet) => fleet.seat === "sol")).toMatchObject({
        was: 3,
        ships: 0,
      });
      expect(tile("27").fleets.find((fleet) => fleet.seat === "sol")).toMatchObject({
        was: 0,
        ships: 8,
      });
      expect(tile("23").pickedUp).toBe(1);
      expect(tile("19").note?.sign).toBe("cargo");
      expect(tile("38").note?.sign).toBe("stay");
      // A check mark on every system that the movement takes something from.
      expect(
        board.tiles
          .filter((item) => item.handled)
          .map((item) => item.id)
          .sort(),
      ).toEqual(["1", "23", "26", "31", "34"]);
      // No path is drawn until a system is open: then the paths of its ships, and their marks.
      expect(board.routes).toEqual([]);
      const marks = (system: string) => {
        reduce(world, { type: "toggleSystem", system });
        const staged = selectShell(world).board.routes.filter((route) => route.staged);
        expect(staged.every((route) => route.path[0] === system)).toBe(true);
        return staged.flatMap((route) => route.marks ?? []).map((mark) => mark.sign);
      };
      expect(marks("26")).toEqual(["plus"]);
      expect(marks("31")).toContain("die");
    });

    it("lists what is committed to the active system, ship by ship", () => {
      const world = cases();
      reduce(world, { type: "inspectSystem", system: "27" });
      const board = selectShell(world).board;
      expect(board.origin).toBeNull();
      const arriving = board.inspector!.arriving!;
      expect(arriving.summary).toContain("8 ships");
      expect(arriving.origins.map((row) => row.system).sort()).toEqual([
        "1",
        "23",
        "26",
        "31",
        "34",
      ]);
      // Only the ships that move, each with its own hold.
      const units = arriving.origins.flatMap((row) => row.ships).flatMap((row) => row.units);
      expect(units.every((row) => row.moves)).toBe(true);
      expect(units.find((row) => row.key === "l-carrier#0")!.hold).toMatchObject({ loaded: 3 });
      // Before the movement no ship of the player is in a system with ships of another player.
      expect(board.inspector!.rows.filter((row) => row.seat === "sol")).toEqual([]);
    });

    it("shows the board after the move in every map view", () => {
      const world = cases();
      const sol = (id: string) =>
        selectShell(world)
          .board.tiles.find((item) => item.id === id)!
          .fleets.find((fleet) => fleet.seat === "sol");
      const troops = () =>
        selectShell(world).board.tiles.find((item) => item.id === "58")!.planets[0].groundForces;
      const [target, origin] = [sol("27")!.strength, sol("58")!.strength];
      expect(troops()).toBe(10);
      reduce(world, { type: "setCount", key: "e-dreadnought#0", value: 1 });
      reduce(world, { type: "setCount", key: "e-dreadnought#1", value: 1 });
      reduce(world, { type: "setCount", key: "e-dreadnought#0>e-infantry", value: 1 });
      reduce(world, { type: "setCount", key: "e-dreadnought#1>e-infantry", value: 1 });
      expect(sol("27")!.strength).toBeGreaterThan(target);
      expect(sol("58")).toMatchObject({ was: 8, ships: 6 });
      expect(sol("58")!.strength).toBeLessThan(origin);
      expect(troops()).toBe(8);
    });

    it("moves many ships of one kind, each with its own hold, and resets one system", () => {
      const world = cases();
      for (const index of [0, 1, 2, 3]) {
        reduce(world, { type: "setCount", key: `e-carrier#${index}`, value: 1 });
      }
      expect(ship(world, "e-carrier")).toMatchObject({ count: 4, total: 4 });
      // A hold takes what fits in one ship, and no more.
      reduce(world, { type: "setCount", key: "e-carrier#0>e-infantry", value: 9 });
      expect(unit(world, "e-carrier#0").hold).toMatchObject({ capacity: 4, loaded: 4 });
      expect(unit(world, "e-carrier#1").hold).toMatchObject({ capacity: 4, loaded: 0 });
      for (const source of ["e-infantry", "e-mech", "e-fighter"]) {
        reduce(world, { type: "fillHold", key: "origin:58", source });
      }
      // 8 infantry and 2 mechs, then fighters until the holds are full.
      const holds = ship(world, "e-carrier").units.map((row) => row.hold!);
      expect(holds.map((hold) => hold.loaded)).toEqual([4, 4, 4, 4]);
      expect(origin(world, "58").cargo.map((row) => row.left)).toEqual([2, 0, 0]);
      // Each ship rolls for itself at the rift, with what it carries.
      const action = selectShell(world).action;
      expect(action.kind === "tactical" && action.task.content.kind).toBe("movement");
      expect(E.riftExits(currentState(world), currentState(world).data.movement)).toContainEqual(
        expect.objectContaining({ ship: "e-carrier#0", cargo: { infantry: 4 } }),
      );
      const tile = () => selectShell(world).board.tiles.find((item) => item.id === "58")!;
      expect(tile().handled).toBe(true);
      const other = ship(world, "l-carrier").count;
      reduce(world, { type: "resetOrigin", system: "58" });
      expect(tile().handled).toBe(false);
      // A system with nothing staged gets the mark from the player. It is not a step of the draft.
      reduce(world, { type: "markOrigin", system: "58" });
      expect(tile().handled).toBe(true);
      expect(origin(world, "58").leaves).toBe("");
      expect(selectShell(world).board.origin).toBeNull();
      expect(ship(world, "e-carrier").count).toBe(0);
      expect(ship(world, "l-carrier").count).toBe(other);
      reduce(world, { type: "undo" });
      expect(unit(world, "e-carrier#3").hold).toMatchObject({ loaded: 4 });
      expect(unit(world, "e-carrier#3").hold!.slots.every((slot) => slot.unit === "fighter")).toBe(
        true,
      );
    });
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

  it("stages a payment of a production with the least waste", () => {
    const world = createWorld();
    loadExample(world, "draft-production");
    reduce(world, { type: "payment", action: "auto" });
    const state = currentState(world);
    const total = E.productionTotals(state, state.edit.value);
    expect(total.paid).toBeGreaterThanOrEqual(total.cost);
  });

  describe("strategy cards", () => {
    const open = (example: string) => {
      const world = createWorld();
      loadExample(world, example);
      return world;
    };
    const flow = (world: ReturnType<typeof createWorld>) => currentState(world).flow;
    const footer = (world: ReturnType<typeof createWorld>) => {
      const action = selectShell(world).action;
      if (action.kind !== "flow") {
        throw new Error("not a flow");
      }
      return action.footer;
    };
    const main = (world: ReturnType<typeof createWorld>) =>
      footer(world).actions.find((action) => action.tone === "primary")!;

    it("Politics: the speaker in the player table is sent first, then the agenda cards", () => {
      const world = open("live-strategy-3");
      const pick = selectShell(world).players.pick!;
      expect(pick.seats.hacan.reason).toBe("Is the speaker");
      reduce(world, { type: "pickSeat", seat: "hacan" });
      expect(flow(world).mine.seat).toBeNull();
      reduce(world, { type: "pickSeat", seat: "xxcha" });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).stage).toBe("primary");
      expect(selectShell(world).players.pick).toBeNull();
      // The first part is sent, so the action cannot be dropped.
      expect(footer(world).actions.some((action) => action.intent.type === "backToPicker")).toBe(
        false,
      );
      expect(main(world).disabled).toBe(true);
      reduce(world, shortcuts(selectShell(world).action).get("1")!);
      reduce(world, shortcuts(selectShell(world).action).get("3")!);
      expect(flow(world).mine.agenda).toEqual(["top", "top"]);
      reduce(world, { type: "chooseOption", option: "swap", group: "agenda" });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toContain("made Blair the speaker");
    });

    it("Construction: the structure in a list, the planet on the board; the second is a PDS", () => {
      const world = open("live-strategy-4");
      expect(selectShell(world).board.task).toBeNull();
      reduce(world, shortcuts(selectShell(world).action).get("1")!);
      const task = selectShell(world).board.task!;
      // Jord and Arnor have a space dock.
      expect("jord" in task.values).toBe(false);
      expect("vefut" in task.values).toBe(true);
      reduce(world, { type: "togglePlanet", planet: "jord" });
      expect(flow(world).mine.build[0].planet).toBeNull();
      reduce(world, { type: "togglePlanet", planet: "vefut" });
      expect(main(world).label).toBe("Resolve primary · 1 structure");
      reduce(world, { type: "togglePlanet", planet: "jord" });
      expect(flow(world).mine.build[1]).toEqual({ type: "pds", planet: "jord" });
      expect(selectShell(world).board.task).toMatchObject({
        interactive: false,
        chosen: { vefut: true, jord: true },
      });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toBe(
        "Jamie placed a space dock on Vefut and a PDS on Jord.",
      );
    });

    it("Trade: several players in the player table, with what each gains", () => {
      const world = open("live-strategy-5");
      const pick = selectShell(world).players.pick!;
      expect(pick.seats.jolnar.reason).toBe("Commodities full");
      expect(pick.seats.hacan.note).toBe("+6 commodities");
      expect(pick.seats.sol.reason).toBe("You");
      for (const seat of ["hacan", "jolnar", "letnev"]) {
        reduce(world, { type: "pickSeat", seat });
      }
      expect(selectShell(world).players.pick!.chosen).toEqual(["hacan", "letnev"]);
      reduce(world, { type: "clearPart", part: "seat:hacan" });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toContain("chose Casey for a free replenish");
    });

    it("Warfare: a token on the board and the pools as counters", () => {
      const world = open("live-strategy-6");
      expect(Object.keys(selectShell(world).board.task!.values).sort()).toEqual(["33", "38"]);
      reduce(world, { type: "toggleSystem", system: "33" });
      expect(footer(world).note).toContain("Place 10 tokens");
      reduce(world, { type: "flowCount", key: "pools.t", value: 4 });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toContain("Pools 3 · 4 · 2 → 4 · 4 · 2");
    });

    it("Warfare secondary: production with a payment, drafted before the seat", () => {
      const world = open("live-secondary-6");
      expect(main(world).label).toBe("Mark draft ready · pass");
      reduce(world, { type: "flowCount", key: "counts.cruiser", value: 1 });
      expect(footer(world).note).toBe("Stage 2 more resources to produce.");
      reduce(world, { type: "payment", action: "auto" });
      expect(main(world).label).toBe("Mark draft ready");
      reduce(world, { type: "flow", action: "ready" });
      for (let turn = 0; turn < 4; turn++) {
        reduce(world, { type: "simulate" });
      }
      expect(flow(world).results.sol).toBe("Followed · produced 1 cruiser in Jord for 2 resources");
    });

    it("Esc clears staged counters: the payment, then the pools, then the bought tokens", () => {
      const world = open("live-strategy-6");
      reduce(world, { type: "flowCount", key: "pools.t", value: 4 });
      reduce(world, { type: "clearChoice" });
      expect(flow(world).mine.pools).toEqual({ t: 3, f: 4, s: 2 });
      loadExample(world, "live-secondary-6");
      reduce(world, { type: "flowCount", key: "counts.cruiser", value: 1 });
      reduce(world, { type: "payment", action: "auto" });
      reduce(world, { type: "clearChoice" });
      expect(flow(world).mine.pay).toEqual({});
      expect(flow(world).mine.counts.cruiser).toBe(1);
      reduce(world, { type: "clearChoice" });
      expect(flow(world).mine.counts).toEqual({});
      expect(main(world).label).toBe("Mark draft ready · pass");
    });

    it("another viewer sees the public side only, and cannot edit", () => {
      const world = open("live-secondary-6");
      setViewer(world, "hacan");
      const view = selectShell(world);
      if (view.action.kind !== "flow") {
        throw new Error("not a flow");
      }
      const seats = view.action.blocks.find((block) => block.kind === "seats");
      if (seats?.kind !== "seats") {
        throw new Error("no seat list");
      }
      expect(seats.rows.every((row) => !row.open)).toBe(true);
      expect(seats.rows.find((row) => row.seat === "hacan")).toMatchObject({ you: true });
      const jamie = seats.rows.find((row) => row.seat === "sol")!;
      expect(jamie.you).toBe(false);
      expect(jamie.status.label).not.toContain("draft");
      expect(view.board.task).toBe(null);
      reduce(world, { type: "flowCount", key: "counts.cruiser", value: 1 });
      expect(flow(world).mine.counts).toEqual({});
    });

    it("Technology: the tree, a second technology for 6 resources that counts the first", () => {
      const world = open("live-strategy-7");
      const cell = (id: string) => {
        const tree = editorBlocks(selectShell(world).action).find(
          (block) => block.kind === "techs",
        );
        if (tree?.kind !== "techs") {
          throw new Error("no tree");
        }
        return [
          ...tree.columns.flatMap((column) => column.cells),
          ...tree.bands.flatMap((band) => band.cells),
        ].find((item) => item.id === id)!;
      };
      expect(cell("antimass-deflectors").state).toBe("owned");
      // Blue 2 and the specialty of Arnor.
      expect(cell("light-wave-deflector").state).toBe("open");
      expect(cell("graviton-laser-system").state).toBe("closed");
      expect(cell("graviton-laser-system").needs).toEqual([{ color: "Y", missing: true }]);
      reduce(world, { type: "pickTech", tech: "graviton-laser-system" });
      expect(flow(world).mine.techs).toEqual([]);
      reduce(world, { type: "pickTech", tech: "sarween-tools" });
      expect(cell("graviton-laser-system").state).toBe("open");
      reduce(world, { type: "pickTech", tech: "graviton-laser-system" });
      expect(footer(world).note).toBe("Stage 6 more resources to research.");
      reduce(world, { type: "payment", action: "auto" });
      expect(main(world).disabled).toBe(false);
      // Without the first technology the second one has no prerequisite.
      reduce(world, { type: "pickTech", tech: "sarween-tools" });
      expect(flow(world).mine.techs).toEqual([]);
      expect(flow(world).mine.pay).toEqual({});
      reduce(world, { type: "pickTech", tech: "sarween-tools" });
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toBe("Jamie researched Sarween Tools.");
    });

    it("Imperial: a fulfilled objective in a list, and the outcome that always happens", () => {
      const world = open("live-strategy-8");
      expect(main(world).label).toBe("Resolve primary · score nothing");
      expect(shortcuts(selectShell(world).action).has("1")).toBe(false);
      reduce(world, { type: "chooseOption", option: "Corner the Market", group: "objective" });
      expect(flow(world).mine.objective).toBeNull();
      reduce(world, shortcuts(selectShell(world).action).get("3")!);
      reduce(world, { type: "flow", action: "resolve" });
      expect(flow(world).primaryResult).toContain("scored Expand Borders (victory points 6 → 7)");
    });

    it("the row of the viewer closes with its result; history has no open row", () => {
      const world = open("live-strategy-5");
      reduce(world, { type: "flow", action: "resolve" });
      const seats = () => {
        const action = selectShell(world).action;
        const block = action.kind === "flow" ? action.blocks[0] : undefined;
        if (block?.kind !== "seats") {
          throw new Error("no seat list");
        }
        return block.rows;
      };
      expect(seats()[0].open).toBeUndefined();
      expect(seats()[0].text).toContain("Jamie gained 3 trade goods");
      reduce(world, { type: "inspectLogEntry", id: "h2" });
      expect(selectShell(world).toolbar.workspace).toBe("history");
      expect(seats().some((row) => row.open)).toBe(false);
      expect(seats().map((row) => row.status.label)).not.toContain("Deciding");
    });

    it("a secondary without a choice is Follow or Pass; a free one says who gave it", () => {
      const world = open("live-secondary-5");
      expect(main(world).disabled).toBe(true);
      const action = selectShell(world).action;
      const menu = editorBlocks(action)[0];
      expect(menu && menu.kind === "menu" && menu.rows[0].state).toContain("Free · chosen by Alex");
      reduce(world, shortcuts(action).get("2")!);
      expect(main(world).label).toBe("Mark draft ready · pass");
      reduce(world, shortcuts(selectShell(world).action).get("1")!);
      reduce(world, { type: "flow", action: "ready" });
      for (let turn = 0; turn < 3; turn++) {
        reduce(world, { type: "simulate" });
      }
      expect(flow(world).results.sol).toBe("Followed · replenished commodities");
    });
  });
});
