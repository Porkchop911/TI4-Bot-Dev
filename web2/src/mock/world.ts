// The dummy's world: three workspaces, the seat that looks at the screen, and the reducer that
// turns intents into engine calls. This is the body of the click handlers of the HTML dummy.
import type { Intent } from "../model";
import {
  HISTORY,
  KEYS,
  MAP,
  RETREATS,
  SEAT,
  SEATS,
  SIDES,
  STEPS,
  U,
  P,
  clone,
  examples,
} from "./data";
import { E } from "./loose";
import { strategyEditor } from "./strategy";

export type State = any;
export interface World {
  mode: "draft" | "live" | "history";
  viewer: string;
  workspaces: { draft: State; live: State; history?: State };
  /** Who plays, and what the viewer holds. `cards` is null in the game of eight: one fixed card each. */
  table: { seats: string[]; cards: Record<string, number[]> | null; actionCards: string[] };
  /** Numbers of the strategy cards that are used in this round. */
  usedCards: number[];
  /** Cards of the viewer that are set to "Never offer": their reaction windows do not open. */
  neverOffer: string[];
  /** Cards that the viewer acts as if holding. Nothing reads it yet. */
  fakedCards: string[];
  /** Counts example loads: the board frames itself again. */
  fit: number;
  toast: { id: number; text: string } | null;
  announcement: string;
}

let toastId = 0;

export const currentState = (world: World): State => world.workspaces[world.mode];
export const sysLabel = (id: string) => `${MAP[id].name} · #${id}`;
export const pastState = (entry: (typeof HISTORY)[number]): State =>
  (entry.state ||= Object.assign(E.makeState(entry.example), {
    past: { label: `R${entry.round} · ${entry.label}` },
    tip: "A past action from the log, read-only. It uses the same frame as Live and Draft.",
  }));

export function createWorld(): World {
  E.env.viewer = "sol";
  return {
    mode: "draft",
    viewer: "sol",
    workspaces: { draft: E.makeState("draft-combat"), live: E.makeState("live-combat") },
    table: tableOf("live-combat"),
    usedCards: [],
    neverOffer: [],
    fakedCards: [],
    fit: 0,
    toast: null,
    announcement: "",
  };
}

function tableOf(example: string): World["table"] {
  const table = examples[example]?.table;
  return {
    seats: table?.seats ?? Object.keys(SEATS),
    cards: table?.cards ?? null,
    actionCards: [...(table?.actionCards ?? ["Mining Initiative"])],
  };
}

/** The reaction window of a battle that the viewer must answer. */
function openReaction(state: State): { pick: string | null } | null {
  if (state.kind === "tactical") {
    const open = E.activeBattle(state);
    if (E.startWindow(state, open)) {
      return open.start;
    }
    return open?.stage === "reaction" && E.controls(state, open.reaction.side)
      ? open.reaction
      : null;
  }
  return null;
}

/** The strategy cards of the viewer, by number. */
export const heldCards = (world: World): number[] => world.table.cards?.sol ?? [1];

export function loadExample(world: World, example: string) {
  E.env.viewer = world.viewer;
  const state = E.makeState(example);
  world.toast = null;
  world.table = tableOf(example);
  world.usedCards = [...(examples[example]?.table?.used ?? [])];
  world.mode = state.mode;
  world.workspaces[state.mode as "draft" | "live"] = state;
  if (world.mode === "draft") {
    world.viewer = E.env.viewer = "sol";
  }
  world.fit++;
  E.normalize(state);
}

export function setViewer(world: World, viewer: string) {
  world.viewer = E.env.viewer = viewer;
  delete world.workspaces.history;
  // A seat change restarts the live example, so nobody inherits a half-made decision.
  world.workspaces.live = E.makeState(
    examples[world.workspaces.live.example] ? world.workspaces.live.example : "live-combat",
  );
  if (viewer !== "sol" || world.mode === "history") {
    world.mode = "live";
  }
  E.normalize(currentState(world));
}

export function resetExample(world: World) {
  const example = currentState(world).example;
  loadExample(world, examples[example] ? example : "live-combat");
  world.toast = { id: ++toastId, text: "This example has been reset." };
}

const battleActions: Record<string, (state: State, battle: any, data: any) => void> = {
  roll: (state, battle) => {
    // The window at the start of the round is answered first.
    if (E.startWindow(state, battle)) {
      return;
    }
    const side = SIDES.find((name) => E.controls(state, name));
    if (
      side &&
      battle.kind === "space" &&
      battle.start.asked !== battle.round &&
      state.hands[SEAT[side]].includes("Morale Boost")
    ) {
      E.env.toasts.push("Morale Boost was not offered: it is set to Never offer.");
    }
    E.battleRoll(state, battle);
  },
  announceRetreat: (state, battle) => {
    battle.announced[SIDES.find((side) => E.controls(state, side))!] = true;
  },
  stageHit: (_state, battle, data) => {
    if (battle.staged.length < battle.owed[battle.side]) {
      battle.staged.push({ type: data.unit, kind: data.kind });
    }
  },
  resetHits: (_state, battle) => {
    battle.staged = [];
  },
  assignHits: (state, battle) => {
    E.applyActions(battle.forces[battle.side], battle.staged, battle.records.at(-1), battle.side);
    Object.assign(battle, { staged: [], stage: "hits" });
    battle.owed[battle.side] = 0;
    E.battleContinue(state, battle);
  },
  passReaction: (state, battle) => {
    if (battle.stage === "pre") {
      battle.start = { pick: null, asked: battle.round };
      return;
    }
    Object.assign(battle, { reaction: null, stage: "hits" });
    E.battleContinue(state, battle);
  },
  playReaction: (state, battle) => {
    if (battle.stage === "pre") {
      const mine = SIDES.find((name) => E.controls(state, name))!;
      battle.boost[mine] = true;
      state.hands[SEAT[mine]] = state.hands[SEAT[mine]].filter(
        (card: string) => card !== battle.start.pick,
      );
      battle.start = { pick: null, asked: battle.round };
      return;
    }
    const { side, target, type } = battle.reaction;
    const rec = battle.records.at(-1);
    const unit = battle.forces[target][type];
    unit.n--;
    unit.dmg--;
    rec.dmg[target][type]--;
    rec.lost[target][type] = (rec.lost[target][type] || 0) + 1;
    rec.notes.push(
      `${P[SEAT[side as "att"]].name} played Direct Hit. The ${U[type as "carrier"].name} that sustained damage was destroyed.`,
    );
    state.hands[SEAT[side as "att"]] = state.hands[SEAT[side as "att"]].filter(
      (card: string) => card !== "Direct Hit",
    );
    Object.assign(battle, { reaction: null, stage: "hits" });
    E.battleContinue(state, battle);
  },
  pickRetreat: (state, battle, data) => {
    if (
      battle.stage === "retreat" &&
      E.controls(state, battle.side) &&
      RETREATS[battle.side as "att"].includes(data.system)
    ) {
      battle.pick = battle.pick === data.system ? null : data.system;
      state.reveal = `sys:${data.system}`;
    }
  },
  retreat: (state, battle) => {
    if (battle.stage !== "retreat" || !battle.pick) {
      return;
    }
    battle.destination = battle.pick;
    E.endRound(state, battle);
  },
};

/** Choices in an action of Jamie that is not a battle. Another viewer cannot send them. */
const PRIVATE = new Set<Intent["type"]>([
  "togglePlanet",
  "toggleSystem",
  "pickSeat",
  "chooseOption",
  "clearPart",
  "clearChoice",
  "stageReaction",
  "reaction",
  "payment",
  "setPayment",
  "endTurn",
  "flowCount",
  "flow",
]);

export function reduce(world: World, intent: Intent) {
  E.env.viewer = world.viewer;
  E.env.neverOffer = world.neverOffer;
  const state = currentState(world);
  const toast = (text: string) => (world.toast = { id: ++toastId, text });
  const announce = (text: string) => (world.announcement = text);
  const progress = () =>
    state.frontier === null ? "Action complete." : `Current step: ${STEPS[state.frontier]}.`;
  state.reveal = null;

  function selectStep(step: number | null) {
    if (state.kind !== "tactical") {
      return void (state.selected = step);
    }
    if (step === null || !E.isReached(state, step)) {
      return;
    }
    state.selected = step;
    announce(
      `Viewing ${STEPS[step]}. ${state.frontier === null ? "Action complete." : `Current step: ${STEPS[state.frontier]}.`}`,
    );
  }
  /** One ship moves or stays, or one unit goes into the hold of a ship or out of it. */
  function stageMove(key: string, value: number) {
    const data = state.edit.value;
    const [unit, cargo] = key.split(">");
    if (!E.shipLine(unit)) {
      return;
    }
    if (cargo) {
      const room = E.holdRoom(data, unit) - E.holdLoad(data, unit) + (data[key] || 0);
      data[key] = Math.max(0, Math.min(value, room, E.available(state, data, key)));
      return;
    }
    data[unit] = value > 0 && E.available(state, data, unit) > 0 ? 1 : 0;
    // A ship that stays takes its hold with it. A ship that needs Gravity Drive takes it.
    E.trimHold(data, unit);
    if (data[unit]) {
      E.autoBoost(state, data);
    }
  }
  /** A draft records every change of the movement at once: one step of undo for each. */
  function recordMove() {
    if (E.recordMovement(state)) {
      E.remember(state);
    }
  }
  function battle(action: string, data: object = {}) {
    const open = E.activeBattle(state);
    if (!open) {
      return;
    }
    const frontier = state.frontier;
    state.rev = (state.rev || 0) + 1;
    battleActions[action](state, open, data);
    if (open.stage === "done") {
      toast(
        open.kind === "space"
          ? "Space combat resolved. Its record stays in this step."
          : "Ground combat resolved.",
      );
    } else if (action === "announceRetreat") {
      toast("Retreat announced. Your ships leave after this round.");
    }
    if (state.frontier !== frontier) {
      announce(progress());
    }
  }

  if (!E.own() && state.kind !== "tactical" && PRIVATE.has(intent.type)) {
    return;
  }
  switch (intent.type) {
    case "selectStep":
      selectStep(intent.step);
      break;
    case "returnToCurrent":
      selectStep(state.frontier);
      break;
    case "returnToEdit":
      selectStep(state.edit.step);
      break;
    case "commitEdit": {
      if (!state.edit || E.validation(state)) {
        break;
      }
      const step = state.edit.step;
      if (step === 0 && state.edit.value.system !== state.data.activation.system) {
        state.data.invasion = {};
      }
      state.data[KEYS[step]!] = clone(state.edit.value);
      state.edit = null;
      E.commitStep(state, step);
      // Keep a fresh dice result on screen; otherwise follow the action.
      const results =
        state.mode === "live" &&
        ((step === 3 &&
          (Object.keys(state.inv.records).length || Object.keys(state.inv.battles).length)) ||
          (step === 1 && state.cannon));
      state.selected = results ? step : (state.frontier ?? step);
      // The movement of a draft is staged on the map: nothing is open over it at the start.
      state.inspect =
        state.mode === "draft" && state.selected === 1 ? null : state.data.activation.system;
      E.remember(state);
      toast(
        state.mode === "draft"
          ? `${STEPS[step]} drafted. Later steps rechecked.`
          : `${STEPS[step]} committed.`,
      );
      announce(
        state.frontier === null ? "All steps complete." : `Current step: ${STEPS[state.frontier]}.`,
      );
      break;
    }
    case "confirmApply": {
      const draft = world.workspaces.draft;
      if (world.mode !== "draft" || !E.canApply(draft) || E.pendingEdit(draft)) {
        break;
      }
      const live = E.blankState("applied", "live", draft.route, draft.data);
      Object.assign(live, {
        stale: draft.stale,
        scene: draft.scene,
        present: draft.present,
        tip: "Your draft is now the live action. Recorded steps are read-only; the rest needs live decisions.",
      });
      // Replay the recorded choices until the live game needs something the draft could not hold.
      for (const step of [0, 1, 3, 4]) {
        if (draft.done[step] && live.frontier === step && live.blocker === "decision") {
          E.commitStep(live, step);
        }
      }
      live.selected = live.frontier ?? 4;
      E.remember(live);
      world.workspaces.live = live;
      world.workspaces.draft = E.makeState("draft-start");
      world.mode = "live";
      toast("Draft applied. Continue in Live.");
      break;
    }
    case "battle":
      battle(intent.action);
      break;
    case "stageHit":
      battle("stageHit", intent);
      break;
    case "workspace":
      if (intent.mode !== "history" || world.workspaces.history) {
        world.mode = intent.mode;
      }
      break;
    case "simulate":
      if (state.kind === "tactical") {
        state.rev = (state.rev || 0) + 1;
        E.simulate(state);
      } else {
        E.flowAdvance(state);
      }
      break;
    case "undo":
    case "redo":
      E.travel(state, state.cursor + (intent.type === "undo" ? -1 : 1));
      break;
    case "startOver":
      world.workspaces.draft = E.makeState("draft-start");
      toast("Draft cleared. Choose a system to begin.");
      break;
    case "discardDraft":
      world.workspaces.draft = E.makeState("draft-start");
      world.mode = "live";
      toast("Draft discarded.");
      break;
    case "editStep": {
      const step = intent.step;
      if (state.mode !== "draft" || state.edit?.dirty || !E.isReached(state, step) || !KEYS[step]) {
        break;
      }
      E.beginEdit(state, step);
      announce(`Editing ${STEPS[step]}. Changes are uncommitted.`);
      break;
    }
    case "cancelEdit":
      state.edit = null;
      toast("Changes dropped. The recorded choices are unchanged.");
      break;
    case "setCount":
    case "removeLine":
    case "setPlacement": {
      if (!state.edit) {
        break;
      }
      const path = intent.type === "setPlacement" ? `place.${intent.unit}` : intent.key;
      const value =
        intent.type === "setPlacement"
          ? intent.place
          : intent.type === "removeLine"
            ? Math.max(0, E.available(state, state.edit.value, path))
            : Math.max(0, intent.value);
      if (state.edit.step === 1) {
        stageMove(path, Number(value));
      } else {
        E.put(state.edit.value, path, value);
      }
      state.edit.dirty = true;
      announce(E.validation(state) || "Selection updated.");
      recordMove();
      break;
    }
    case "inspectSystem":
      state.inspect = intent.system;
      if (state.edit?.step === 0 && state.selected === 0) {
        Object.assign(state.edit, { value: { system: state.inspect }, dirty: true });
      }
      if (state.edit?.step === 1) {
        state.reveal = `sys:${state.inspect}`;
      }
      break;
    case "setBoost": {
      if (state.edit?.step !== 1) {
        break;
      }
      E.setBoost(state, state.edit.value, intent.ship);
      state.edit.dirty = true;
      announce(E.validation(state) || "Gravity Drive moved.");
      recordMove();
      break;
    }
    case "fillHold": {
      if (state.edit?.step === 1) {
        const data = state.edit.value;
        if (intent.key.startsWith("origin:")) {
          E.fillOrigin(state, data, intent.key.slice(7), intent.source);
        } else {
          E.fillHold(state, data, intent.key, intent.source);
        }
        state.edit.dirty = true;
        announce(E.validation(state) || "Hold filled.");
        recordMove();
      }
      break;
    }
    case "resetOrigin": {
      if (state.edit?.step !== 1) {
        break;
      }
      // Everything that leaves with the ships of this system stays: the ships and their holds.
      for (const entry of E.stagedEntries(state.edit.value)) {
        if (entry.carrier === intent.system) {
          state.edit.value[entry.key] = 0;
        }
      }
      for (const key of Object.keys(state.edit.value)) {
        if (key.startsWith("@gd:") && E.shipLine(key.slice(4))?.origin === intent.system) {
          delete state.edit.value[key];
        }
      }
      state.handled = { ...state.handled, [intent.system]: undefined };
      state.edit.dirty = true;
      announce(E.validation(state) || `Movement from ${MAP[intent.system].name} reset.`);
      recordMove();
      break;
    }
    case "markOrigin":
      if (state.edit?.step === 1) {
        // A mark of this page only: it is not a step of the draft.
        state.handled = { ...state.handled, [intent.system]: E.activeId(state) };
        state.inspect = null;
      }
      break;
    case "closeInspector":
      state.inspect = null;
      break;
    case "togglePlanet":
      if (state.kind === "tactical") {
        if (state.edit?.step !== 4) {
          break;
        }
        state.edit.value.pay[intent.planet] = !state.edit.value.pay[intent.planet];
        state.edit.dirty = true;
        state.reveal = `pl:${intent.planet}`;
        announce(E.validation(state) || "Payment staged.");
      }
      break;
    case "toggleSystem":
      if (state.kind === "tactical" && state.edit?.step === 1) {
        // The fleet of a system opens at the system: its controls are on the board.
        state.inspect = state.inspect === intent.system ? null : intent.system;
        break;
      }
      if (state.kind === "tactical") {
        battle("pickRetreat", intent);
        break;
      }
      break;
    case "pickSeat":
    case "chooseOption":
    case "clearPart":
      break;
    case "clearChoice":
      {
        const open =
          openReaction(state) ??
          (state.kind === "tactical" && E.activeBattle(state)?.stage === "retreat"
            ? E.activeBattle(state)
            : null);
        if (open) {
          open.pick = null;
        }
      }
      break;
    case "stageReaction": {
      const open = openReaction(state);
      if (open) {
        open.pick = open.pick === intent.card ? null : intent.card;
      }
      break;
    }
    case "reaction": {
      const open = openReaction(state);
      if (!open || (intent.action === "play" && !open.pick)) {
        break;
      }
      battle(intent.action === "play" ? "playReaction" : "passReaction");
      break;
    }
    case "setOffer":
      // The demo has the two modes of a card: offered, or never.
      world.neverOffer = world.neverOffer.filter((card) => card !== intent.key);
      if (intent.mode !== "always") {
        world.neverOffer.push(intent.key);
      }
      toast(
        intent.mode !== "always"
          ? `${intent.name}: never offered. Its windows are skipped and you are told.`
          : `${intent.name}: offered again.`,
      );
      break;
    case "setCardFake":
      world.fakedCards = world.fakedCards.filter((card) => card !== intent.card);
      if (intent.fake) {
        world.fakedCards.push(intent.card);
      }
      break;
    case "payment": {
      const auto = intent.action === "auto";
      if (state.edit?.step === 4) {
        const cost = E.productionTotals(state, state.edit.value).cost;
        state.edit.value.pay = auto ? E.autoPay(cost, "res") : {};
        state.edit.dirty = true;
        announce(E.validation(state) || "Payment staged.");
      }
      break;
    }
    case "openReference":
      break;
    case "setPayment":
      if (state.edit?.step === 4) {
        state.edit.value.pay[intent.source] = intent.value;
        state.edit.dirty = true;
        announce(E.validation(state) || "Payment staged.");
      }
      break;
    case "findSystem": {
      const text = intent.query.trim().toLowerCase();
      const id = text.match(/#?(\d+)$/)?.[1];
      const ids = Object.keys(MAP);
      const found =
        id && MAP[id]
          ? id
          : ids.find((other) => text && MAP[other].name.toLowerCase().startsWith(text)) ||
            ids.find((other) => text && MAP[other].name.toLowerCase().includes(text));
      if (!found || state.edit?.step !== 0) {
        announce("No system has this name or number.");
        break;
      }
      Object.assign(state.edit, { value: { system: found }, dirty: true });
      state.inspect = found;
      announce(`${sysLabel(found)} selected. ${E.validation(state)}`);
      break;
    }
    case "inspectLogEntry":
      world.workspaces.history = pastState(HISTORY.find((entry) => entry.id === intent.id)!);
      world.mode = "history";
      break;
    case "closeHistory":
      delete world.workspaces.history;
      world.mode = "live";
      break;
    case "endTurn":
      if (!E.turnClosing(state)) {
        break;
      }
      if (state.kind === "strategic" && !world.usedCards.includes(state.flow.number ?? 1)) {
        world.usedCards.push(state.flow.number ?? 1);
      }
      // The demo has no next turn: the example starts again.
      world.workspaces.live = E.makeState(state.example);
      world.mode = "live";
      toast("Turn ended.");
      break;
    case "flowCount":
      E.put(state.flow.mine, intent.key, Math.max(0, intent.value));
      if (!strategyEditor(state).cost?.amount) {
        state.flow.mine.pay = {};
      }
      announce(strategyEditor(state).problem || "Selection updated.");
      break;
    case "flow": {
      const flow = state.flow;
      if (intent.action === "play") {
        break;
      } else if (intent.action === "ready") {
        const editor = strategyEditor(state);
        if (editor.problem) {
          break;
        }
        Object.assign(flow.mine, { ready: true, outcome: editor.result });
        toast("Draft ready. It resolves when your seat is reached.");
        E.settle(state);
      } else if (intent.action === "change") {
        flow.mine.ready = false;
      } else {
        const editor = strategyEditor(state);
        if (editor.problem) {
          break;
        }
        if (flow.stage === "primary") {
          Object.assign(flow, { stage: "secondary", primaryResult: editor.result });
        } else {
          flow.mine.outcome = editor.result;
          E.resolveMine(state);
        }
        E.settle(state);
        toast(
          `${flow.card} ${flow.stage === "secondary" ? "primary resolved. Secondaries follow in seat order." : "secondary resolved."}`,
        );
      }
      break;
    }
    case "openApply":
      break;
  }
  for (const text of E.env.toasts.splice(0)) {
    toast(text);
  }
  E.normalize(currentState(world));
}
