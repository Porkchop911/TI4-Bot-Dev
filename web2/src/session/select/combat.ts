// The space combat of a tactical action, from what every seat sees of it (`BoardView.combat`):
// both sides as they started the round, the round and its stage, and whom the game waits for.
// Every seat has this screen: a seat that does not fight watches it, read-only. The question of
// the viewer during the combat stays a list under the table, or the band of a reaction window
// on top of the panel.
import type {
  BattleRowView,
  BattleSideView,
  BattleTableView,
  CombatView,
  FlowActionView,
  FooterView,
  PillView,
} from "../../model";
import type {
  Catalog,
  CombatDieRoll,
  CombatView as CombatState,
  PlacedUnitView,
  SessionUpdate,
} from "../wire";
import { systemLabel, tilesOf } from "./movement";
import { expected } from "./movementRolls";
import { byUnitOrder, unitKey, unitLabel, unitRecord, unitType } from "./names";

/** The stage of the round in words, from the phase of the engine. */
const STAGES: Record<string, string> = {
  pre_roll: "Before the roll",
  barrage: "Anti-fighter barrage",
  resolving_hits: "Hits",
  retreating: "Retreat",
  complete: "Over",
};

/** The stage of the open round: "Before the roll", "Hits to assign". */
export function stageOf(combat: CombatState): string {
  if (combat.phase === "resolving_hits" && (combat.hits_to_assign ?? 0) > 0) {
    return "Hits to assign";
  }
  return STAGES[combat.phase] ?? combat.phase.replaceAll("_", " ");
}

const row = (change: Partial<BattleRowView>): BattleRowView => ({
  unit: null,
  count: null,
  target: null,
  targetTitle: "",
  modified: false,
  dice: [],
  hits: "",
  scored: false,
  pills: [],
  ...change,
});

/**
 * The rows of one side: its ships at the start of the round, by the printed unit, with what
 * each hits on. Before the roll the value is the printed one, with the dice that the ships roll
 * and the hits that they score on average. Once the round rolled, the value is what the dice of
 * the engine needed (after the modifiers of the round), with the printed value on hover when it
 * differs, and a row has its dice. The engine rolls the ships of one value together
 * (`CombatDieRoll.unit` names them all: "carrier+destroyer"), so the dice of such a roll are
 * dealt to its rows in their order, as many as each row rolls, the rest to its last row.
 */
export function sideRows(
  ships: PlacedUnitView[],
  combat: Pick<CombatState, "dice_rolls">,
  seat: string,
  printed: Catalog | null,
): BattleRowView[] {
  const groups = new Map<string, { id: string; count: number; damaged: number }>();
  for (const ship of ships) {
    const key = unitKey(ship.unit_type, printed);
    const group = groups.get(key) ?? { id: ship.unit_type, count: 0, damaged: 0 };
    group.count += 1;
    group.damaged += ship.damaged ? 1 : 0;
    groups.set(key, group);
  }
  const rolls = (combat.dice_rolls ?? []).filter((die) => die.player === seat);
  const sorted = [...groups.values()].sort((a, b) => byUnitOrder(a.id, b.id));
  // The roll of each row, and the dice that each row has of it.
  const rollOf = (id: string) =>
    rolls.find((die) =>
      die.unit.split("+").some((unit) => unitKey(unit, printed) === unitKey(id, printed)),
    )?.unit ?? null;
  const dealt = new Map<string, CombatDieRoll[]>();
  for (const roll of new Set(rolls.map((die) => die.unit))) {
    const dice = rolls.filter((die) => die.unit === roll);
    const members = sorted.filter((group) => rollOf(group.id) === roll);
    members.forEach((group, index) => {
      const count = group.count * (unitRecord(group.id, printed)?.combat_die_count ?? 1);
      dealt.set(group.id, index === members.length - 1 ? dice : dice.splice(0, count));
    });
  }
  return sorted.map(({ id, count, damaged }) => {
    const record = unitRecord(id, printed);
    const card = record?.combat_hits_on ?? null;
    const key = unitKey(id, printed);
    const rolled = rolls.find((die) =>
      die.unit.split("+").some((unit) => unitKey(unit, printed) === key),
    );
    const target = rolled?.target ?? card;
    const modified = card !== null && target !== null && target !== card;
    const dice = count * (record?.combat_die_count ?? 1);
    const before = !rolls.length && target !== null;
    const faces = dealt.get(id) ?? [];
    return row({
      unit: unitType(id),
      label: unitLabel(id, printed, count),
      count,
      target,
      targetTitle: target === null ? "" : modified ? `Printed ${card}+` : `Hits on ${target}+`,
      modified,
      dice: before ? dice : faces.map((die) => ({ roll: die.roll, hit: die.hit })),
      hits: before ? expected(dice, target) : "",
      scored: faces.some((die) => die.hit),
      pills: damaged ? [{ tone: "damage", label: `${damaged} damaged` }] : [],
    });
  });
}

/** The battle table of the open round: the attacker, then the defender. */
export function roundTable(
  update: SessionUpdate,
  combat: CombatState,
  printed: Catalog | null,
): BattleTableView {
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  // Before the first round the engine has no units of the round yet: the ships in the system.
  const ships = combat.round_start.length
    ? combat.round_start
    : (update.view.board.systems[combat.system_id]?.units ?? []).filter((unit) => {
        const type = unitType(unit.unit_type);
        return unit.planet === null && type !== null && type !== "infantry" && type !== "mech";
      });
  const rolled = (combat.dice_rolls ?? []).length > 0;
  const sides = [combat.attacker, combat.defender].map((seat): BattleSideView => ({
    seat,
    role: seat === combat.attacker ? "Attacker" : "Defender",
    isViewer: seat === viewer,
    hits: rolled
      ? ((seat === combat.attacker ? combat.attacker_hits : combat.defender_hits) ?? 0)
      : null,
    rows: sideRows(
      ships.filter((unit) => unit.owner === seat),
      combat,
      seat,
      printed,
    ),
  }));
  const owed = combat.hits_to_assign && combat.active_player;
  return {
    label: `Round ${combat.round}`,
    score: stageOf(combat),
    owed: owed ? { hits: combat.hits_to_assign!, seat: combat.active_player! } : null,
    sides,
  };
}

/**
 * The space combat step: the table of the round, and the open question of the viewer as a list
 * (`flow`, the decision of `selectAction`). A reaction window is the band on top of the panel
 * (`flow.interrupt`), so its trigger is not repeated here.
 */
export function selectCombat(
  update: SessionUpdate,
  combat: CombatState,
  flow: FlowActionView,
  asked: boolean,
  printed: Catalog | null,
): { content: CombatView; footer: FooterView; pill: PillView | null; target: string } {
  const decision = asked && !flow.interrupt ? { prompt: flow.heading, blocks: flow.blocks } : null;
  return {
    target: systemLabel(tilesOf(update), combat.system_id),
    content: {
      kind: "combat",
      decision,
      records: [
        {
          key: `${combat.battle_seq}:${combat.round}`,
          label: `Round ${combat.round}`,
          table: roundTable(update, combat, printed),
        },
      ],
      revision: `${combat.battle_seq}:${combat.round}:${combat.phase}`,
    },
    // The stage of the round is the score of the round table; a watching seat has no pill.
    pill: asked ? (flow.pill ?? { tone: "live", label: "Your decision" }) : null,
    footer: flow.footer,
  };
}
