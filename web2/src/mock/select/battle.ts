// Dice tables, odds, outcomes and the decisions of an open battle.
import type {
  BattleOfferView,
  BattleRowView,
  BattleSideView,
  BattleTableView,
  OddsView,
  OutcomeView,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import { P, ROLE, SEAT, SIDES, U, clone, other, type Side } from "../data";
import { E } from "../loose";
import { sysLabel, type State } from "../world";
import { toForce } from "./shared";

export function oddsView(kind: "space" | "ground", att: any, def: any, caveat: string): OddsView {
  const result = E.odds(kind, att, def);
  return {
    kind,
    attacker: SEAT.att,
    defender: SEAT.def,
    attackerWins: result.att,
    defenderWins: result.def,
    rollouts: result.runs,
    rounds: result.rounds,
    attackerLeft: result.attLeft,
    defenderLeft: result.defLeft,
    caveat,
  };
}

/** options: meta ('space' | 'ground'), battle (keeps row slots, live forces), live (latest roll of an open battle) */
export function battleTable(
  state: State,
  rec: any,
  options: { meta?: "space" | "ground"; battle?: any; live?: boolean } = {},
): BattleTableView {
  const battle = options.battle;
  const viewer: string = E.env.viewer;
  const assigning =
    options.live && battle?.stage === "assign" && E.controls(state, battle.side) ? battle : null;
  const left = assigning ? E.applyActions(clone(battle.forces[battle.side]), battle.staged) : null;
  const owed =
    options.live && battle && ["assign", "waiting"].includes(battle.stage)
      ? battle.owed[battle.side]
      : 0;
  const side = (name: Side): BattleSideView => {
    const seat = SEAT[name];
    const now = options.live && battle.stage !== "done" ? battle.forces[name] : rec.start[name];
    const mod = rec.mod[name];
    const rows = ((battle?.types[name] ?? E.types(rec.start[name])) as UnitType[]).map(
      (type): BattleRowView => {
        const unit = rec.start[name][type] || { n: 0, dmg: 0 };
        const target: number | undefined = rec.targets[name][type];
        const dice: { face: number; hit: boolean }[] = rec.dice[name][type] || [];
        const hits = dice.filter((die) => die.hit).length;
        const lost = rec.lost[name][type] || 0;
        const hurt = rec.dmg[name][type] || 0;
        const expected =
          rec.pre && target
            ? (
                (rec.count[name][type] * Math.min(10, 11 - target + (mod?.value || 0))) /
                10
              ).toFixed(1)
            : null;
        const staged: { kind: string }[] =
          assigning && name === battle.side
            ? battle.staged.filter((action: any) => action.type === type)
            : [];
        const after = left?.[type];
        const full = !!assigning && battle.staged.length >= battle.owed[battle.side];
        const pick = (kind: "sustain" | "destroy" | "destroy-damaged", label: string) => ({
          kind,
          label,
          disabled: full,
        });
        const assign =
          assigning && name === battle.side && after?.n
            ? [
                ...(U[type].sustain && after.n > after.dmg
                  ? [pick("sustain", "Sustain damage")]
                  : []),
                ...(after.dmg && after.n > after.dmg
                  ? [
                      pick("destroy-damaged", "Destroy damaged"),
                      pick("destroy", "Destroy undamaged"),
                    ]
                  : [pick(after.dmg ? "destroy-damaged" : "destroy", "Destroy")]),
              ]
            : [];
        return {
          unit: type,
          count: unit.n,
          target: target ? target - (mod?.value || 0) : null,
          targetTitle: target
            ? `Hits on ${target}+${mod ? `, ${mod.source} +${mod.value}` : ""}`
            : "",
          modified: !!(mod && target),
          dice: rec.pre
            ? rec.count[name][type] || 0
            : dice.map((die) => ({ roll: die.face, hit: die.hit })),
          hits: expected ? "≈" + expected : target ? plural(hits, "hit") : "",
          scored: hits > 0,
          pills: [
            ...(unit.dmg ? [{ tone: "damage" as const, label: `${unit.dmg} damaged` }] : []),
            ...(hurt ? [{ tone: "damage" as const, label: `+${hurt} damaged` }] : []),
            ...(lost
              ? [
                  {
                    tone: "loss" as const,
                    label: `−${lost}`,
                    title: `${lost} destroyed in this step`,
                  },
                ]
              : []),
            ...staged.map((action) => ({
              tone: "staged" as const,
              label: action.kind === "sustain" ? "sustain" : "−1",
            })),
          ],
          gone: !unit.n,
          assign,
        };
      },
    );
    const used =
      (now.fighter?.n || 0) + (name === "att" ? state.ground.infantry + state.ground.mech : 0);
    const hand: string[] = state.hands[seat];
    const meta: BattleSideView["meta"] = [];
    if (options.meta === "space") {
      meta.push({
        text: `Fleet supply ${E.ships(now)} / ${P[seat].fleet}`,
        tone: E.ships(now) > P[seat].fleet ? "bad" : undefined,
      });
      meta.push({
        text: `Capacity ${used} / ${E.capacityOf(now)}`,
        tone: used > E.capacityOf(now) ? "bad" : undefined,
      });
    }
    const cards = options.meta
      ? `${plural(hand.length, "action card")}${viewer === seat && hand.length ? ": " + hand.join(", ") : ""}`
      : "";
    if (mod) {
      meta.push({ text: `+${mod.value} to combat rolls · ${mod.source}`, tone: "accent" });
    }
    const rolled = Object.keys(rec.targets[name]).length > 0;
    return {
      seat,
      role: ROLE[name],
      isViewer: viewer === seat,
      hits: rec.pre || !rolled ? null : rec.hits[name],
      meta,
      cards,
      rows,
    };
  };
  return {
    label: rec.label,
    score: rec.pre
      ? "Before the roll · expected hits shown"
      : SIDES.filter((name) => Object.keys(rec.targets[name]).length)
          .map((name) => `${P[SEAT[name]].faction} ${plural(rec.hits[name], "hit")}`)
          .join(" · "),
    owed: owed ? { hits: owed, seat: SEAT[battle.side as Side] } : null,
    sides: SIDES.map(side),
    notes: rec.notes,
    hint: rec.hint,
  };
}

export function outcomeView(battle: any, title: string, subtitle: string): OutcomeView {
  return {
    title,
    subtitle,
    sides: SIDES.map((name) => {
      const final = battle.retreat?.side === name ? battle.retreat.units : battle.forces[name];
      const lost = E.force(
        Object.fromEntries(
          E.types(battle.initial[name]).map((type: string) => [
            type,
            battle.initial[name][type].n - (final[type]?.n || 0),
          ]),
        ),
      );
      return {
        seat: SEAT[name],
        lost: toForce(lost),
        kept: toForce(final),
        keptLabel:
          battle.retreat?.side === name ? "Retreated to " + battle.retreat.to : "Survivors",
      };
    }),
    note: battle.cargoLost?.length
      ? `Transport capacity was lost with the ships. Removed: ${battle.cargoLost.map((type: UnitType) => E.unitName(type)).join(", ")}.`
      : undefined,
  };
}

/** Cards, warnings and choices that belong to the open stage of a battle. */
export function battleOffers(state: State, battle: any): BattleOfferView[] {
  const out: BattleOfferView[] = [];
  const viewer: string = E.env.viewer;
  const side = SIDES.find((name) => E.controls(state, name));
  for (const name of SIDES) {
    if (battle.announced[name] && battle.stage !== "done") {
      out.push({
        kind: "note",
        tone: "accent",
        strong: `${viewer === SEAT[name] ? "You" : P[SEAT[name]].name} announced a retreat.`,
        hint: "Surviving ships leave after this round.",
      });
    }
  }
  if (battle.stage === "pre" && side && battle.kind === "space") {
    if (battle.boost[side]) {
      out.push({
        kind: "note",
        tone: "success",
        strong: "Morale Boost played.",
        text: "+1 to each of your combat rolls this round.",
      });
    }
  }
  if (battle.stage === "assign" && E.controls(state, battle.side)) {
    const foe = SEAT[other(battle.side)];
    const group = battle.forces[battle.side];
    if (
      battle.kind === "space" &&
      E.types(group).some((type: UnitType) => U[type].sustain && group[type].n > group[type].dmg) &&
      state.hands[foe].length
    ) {
      out.push({
        kind: "note",
        tone: "accent",
        alert: true,
        strong: `${P[foe].name} holds ${plural(state.hands[foe].length, "action card")}.`,
        hint: "A ship that sustains damage can be destroyed by Direct Hit.",
      });
    }
  }
  if (battle.stage === "retreat" && E.controls(state, battle.side)) {
    out.push({
      kind: "offer",
      eyebrow: "Retreat",
      title: battle.pick ? `Retreat to ${sysLabel(battle.pick)}` : "Choose a system on the board",
      text: "Your surviving ships move to an adjacent system that holds one of your command tokens or none of the enemy’s ships.",
      actions: [],
    });
  }
  return out;
}
