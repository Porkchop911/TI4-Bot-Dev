// A tactical action: the stepper, the open step with its content, and its footer.
import type {
  ActivationView,
  BattleRecordView,
  CombatView,
  FooterView,
  HelpView,
  InvasionPlanetView,
  InvasionView,
  PillView,
  ProductionView,
  StepContentView,
  TacticalActionView,
  TaskView,
  TrailStatus,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import {
  ANOMALY,
  KEYS,
  MAP,
  P,
  SEAT,
  SEATS,
  SIDES,
  SPECS,
  STEPS,
  STOCK,
  SUPPLY,
  U,
  type Side,
} from "../data";
import { E } from "../loose";
import { sysLabel, type State, type World } from "../world";
import { battleOffers, battleTable, oddsView, outcomeView } from "./battle";
import { movement } from "./movement";
import {
  END_TURN,
  battleReaction,
  button,
  paySummary,
  productionPayment,
  closingBlocks,
  countsForce,
  mine,
  stepCaption,
  systemName,
  waitingFor,
  withKeys,
} from "./shared";

type Trail = TaskView["trail"];

const skippedText = (reason: string) => `${reason[0].toUpperCase() + reason.slice(1)}.`;

function activation(state: State): ActivationView {
  const editing = state.edit?.step === 0;
  const draft = state.mode === "draft";
  const base = {
    kind: "activation" as const,
    editing,
    draft,
    tacticPool: [SEATS.sol.tokens[0], SEATS.sol.tokens[0] - 1] as [number, number],
    systems: Object.keys(MAP).map((id) => ({ id, label: sysLabel(id) })),
  };
  if (!editing) {
    return {
      ...base,
      chosen: null,
      active: { system: state.data.activation.system, name: systemName(state) },
    };
  }
  const id: string | null = state.edit.value.system;
  const system = id ? MAP[id] : null;
  if (!id || !system) {
    return { ...base, chosen: null, active: null };
  }
  const fleets = E.others(state, id);
  const range = system.token ? { ships: 0, systems: 0 } : E.reach(state, id);
  const facts =
    [
      system.home && `${SEATS[system.home].faction} home system`,
      system.anomaly && ANOMALY[system.anomaly].label,
      system.wormhole && `${system.wormhole} wormhole`,
      ...E.planetsOf(state, id).map(
        (planet: any) =>
          `${planet.name} ${planet.res}/${planet.inf} · ${planet.owner ? SEATS[planet.owner].faction : "uncontrolled"}`,
      ),
      ...Object.keys(fleets).map(
        (seat) => `${SEATS[seat].faction} fleet: ${E.list(E.force(fleets[seat]))}`,
      ),
    ]
      .filter(Boolean)
      .join(" · ") || "No planets, no ships";
  return {
    ...base,
    chosen: {
      system: id,
      label: sysLabel(id),
      facts,
      commandToken: !!system.token,
      shipsInRange: range.ships,
      originsInRange: range.systems,
      production: 0,
      cannon: 0,
    },
    active: null,
  };
}

function combat(state: State): CombatView {
  const base = {
    kind: "combat" as const,
    skipped: null,
    outcome: null,
    offers: [],
    revision: `${state.rev || 0}`,
  };
  if (state.skipped[2]) {
    return { ...base, skipped: skippedText(state.skipped[2]), records: [] };
  }
  const battle = state.battle;
  const both = { att: SPECS.combat, def: SPECS.combat };
  if (!battle) {
    return {
      ...base,
      records: [
        {
          key: "next",
          label: "Round 1",
          odds: oddsView("space", state.fleet, state.enemyFleet, "no action cards"),
          table: battleTable(
            state,
            E.preview("Round 1", { att: state.fleet, def: state.enemyFleet }, both),
            { meta: "space" },
          ),
        },
      ],
    };
  }
  const tabs: [string, string][] = battle.records.map((rec: any) => [rec.key, rec.label]);
  if (battle.stage === "pre") {
    tabs.push(["next", `Round ${battle.round} · next`]);
  }
  const latest = tabs.at(-1)![0];
  const records = tabs.map(([key, label]): BattleRecordView => {
    const rec =
      key === "next"
        ? E.preview(`Round ${battle.round}`, battle.forces, both, battle.boost)
        : battle.records.find((item: any) => item.key === key);
    return {
      key,
      label,
      odds:
        key === "next"
          ? oddsView("space", battle.forces.att, battle.forces.def, "no action cards")
          : undefined,
      table: battleTable(state, rec, { meta: "space", battle, live: key === latest }),
    };
  });
  const outcome =
    battle.stage === "done"
      ? outcomeView(
          battle,
          battle.winner
            ? `${P[SEAT[battle.winner as Side]].faction} won the space combat`
            : "Both fleets were destroyed",
          `System ${state.data.activation.system} · ${plural(battle.round, "round")}${battle.retreat ? ` · ${P[SEAT[battle.retreat.side as Side]].name} retreated` : ""}`,
        )
      : null;
  return { ...base, outcome, offers: battleOffers(state, battle), records };
}

function invasion(state: State): InvasionView {
  const base = {
    kind: "invasion" as const,
    landing: null,
    skipped: null,
    planets: [],
    gauges: [],
    plannedNote: false,
    revision: `${state.rev || 0}`,
  };
  if (state.skipped[3]) {
    return { ...base, skipped: skippedText(state.skipped[3]) };
  }
  const editing = state.edit?.step === 3;
  const data = E.editedData(state, 3);
  const inv = state.inv;
  const draft = state.mode === "draft";
  const carried = (["infantry", "mech"] as UnitType[]).filter(
    (type) =>
      state.ground[type] || state.planets.some((planet: any) => E.landing(data, planet.id, type)),
  );
  const used = (type: UnitType): number =>
    state.planets.reduce((sum: number, planet: any) => sum + E.landing(data, planet.id, type), 0);
  const committed =
    state.done[3] || (!!inv.battles && state.mode === "live" && state.frontier !== 3);
  const planets = state.planets.map((planet: any): InvasionPlanetView => {
    const before = inv.before[planet.id];
    const enemy = E.hostile(before);
    const troops = E.landed(state, planet.id, data);
    const battle = inv.battles[planet.id];
    const structures = before.structures
      .map(
        (type: UnitType) =>
          U[type].name + (type === "pds" ? " · Planetary Shield · Space Cannon 6" : ""),
      )
      .join(" · ");
    const counters = editing
      ? carried.map((type) => {
          const left = Math.max(0, state.ground[type] - used(type));
          const value: number = E.landing(data, planet.id, type);
          return {
            unit: type,
            left,
            counter: {
              id: `${planet.id}:${type}`,
              value,
              max: value + left,
              label: `${E.unitName(type)} on ${planet.name}`,
            },
          };
        })
      : [];
    let projection: InvasionPlanetView["projection"] = null;
    if (editing || state.boundary === "ground") {
      projection =
        !enemy || !E.size(planet.units)
          ? { note: "No ground combat expected here." }
          : !E.size(troops)
            ? { note: "Stage forces to see projected odds." }
            : {
                table: battleTable(
                  state,
                  E.preview(
                    "Ground combat · Round 1",
                    { att: troops, def: planet.units },
                    { att: SPECS.ground, def: SPECS.ground },
                  ),
                  { meta: "ground" },
                ),
                odds: oddsView(
                  "ground",
                  troops,
                  planet.units,
                  "no action cards or space cannon defense",
                ),
              };
    }
    const recs: any[] = [...(inv.records[planet.id] || []), ...(battle?.records || [])];
    const records = recs.map((rec): BattleRecordView => {
      const ground = battle?.records.includes(rec);
      return {
        key: rec.key,
        label: rec.label,
        table: battleTable(
          state,
          rec,
          ground ? { meta: "ground", battle, live: rec === battle.records.at(-1) } : {},
        ),
      };
    });
    let preview: InvasionPlanetView["preview"] = null;
    if (enemy && E.size(planet.units)) {
      if (state.boundary === "bombard") {
        preview = battleTable(
          state,
          E.preview(
            "Bombardment",
            {
              att: E.force(
                Object.fromEntries(
                  E.types(state.fleet)
                    .filter((type: UnitType) => U[type].bombard)
                    .map((type: UnitType) => [type, state.fleet[type].n]),
                ),
              ),
              def: planet.units,
            },
            { att: SPECS.bombard },
          ),
        );
      }
      if (state.boundary === "cannon-defense" && E.size(troops)) {
        preview = battleTable(
          state,
          E.preview(
            "Space cannon defense",
            { att: troops, def: E.force({ pds: 1 }) },
            { def: SPECS.cannon },
          ),
        );
      }
    }
    const kind: string | undefined = inv.results[planet.id];
    const outcome = kind
      ? {
          reinforced: "Reinforced · Sol keeps control",
          claimed: "Claimed · Sol now controls",
          captured: "Captured · Sol now controls",
          held: "Invasion failed · Hacan keeps control",
          repelled: "Landing force destroyed by space cannon defense",
        }[kind]
      : undefined;
    const done = battle?.stage === "done";
    const facts: InvasionPlanetView["facts"] = [
      {
        label: `${enemy ? "Defenders" : "Garrison"}: `,
        strong: E.list(enemy && !battle ? planet.units : before.units),
        text:
          enemy && E.size(planet.units) < E.size(before.units) && !battle
            ? " after bombardment"
            : "",
      },
    ];
    if (structures) {
      facts.push({ text: structures });
    }
    if (inv.bombard[planet.id]) {
      facts.push({ text: `Bombardment skipped · ${inv.bombard[planet.id]}` });
    }
    if (!editing) {
      facts.push({
        label: `${committed && !draft ? "Landed" : draft ? "Planned to land" : "Staged"}: `,
        strong: E.list(troops),
      });
    }
    return {
      id: planet.id,
      name: planet.name,
      heading: `${plural(planet.res, "resource")} · ${planet.inf} influence${editing ? ` · On planet ${E.size(before.units)} · Staged ${E.size(troops)}` : ""}`,
      counters,
      owner: before.owner,
      facts,
      result: !done && outcome ? { text: outcome, failed: kind === "repelled" } : null,
      outcome: done
        ? outcomeView(
            battle,
            outcome!,
            `${planet.name} · ${plural(battle.round, "round")} of ground combat`,
          )
        : null,
      projection,
      offers: battle && battle.stage !== "done" ? battleOffers(state, battle) : [],
      records: preview ? [] : records,
      preview,
    };
  });
  return {
    ...base,
    planets,
    gauges: editing
      ? carried.map((type) => ({
          label: `${E.unitName(type, 2)} committed`,
          used: used(type),
          total: state.ground[type],
        }))
      : [],
    plannedNote: draft && !editing && !!state.done[3],
  };
}

function production(state: State): ProductionView {
  const empty = {
    kind: "production" as const,
    title: "",
    subtitle: "",
    rows: [],
    gauges: [],
    payment: {
      title: "",
      cost: 0,
      paid: 0,
      unit: "resource",
      editable: false,
      summary: "",
      goods: null,
      actions: [],
    },
    done: null,
  };
  if (state.skipped[4]) {
    return { ...empty, skipped: skippedText(state.skipped[4]) };
  }
  const editing = state.edit?.step === 4;
  const data = E.editedData(state, 4);
  const total = E.productionTotals(state, data);
  const draft = state.mode === "draft";
  const done = state.frontier === null;
  const rows = (Object.keys(STOCK) as UnitType[])
    .filter((type) => editing || data.units[type])
    .map((type) => {
      const count: number = data.units[type] || 0;
      const unit = U[type];
      const where = unit.ground
        ? data.place[type] === "space"
          ? "in the space area"
          : "on Starpoint"
        : "in the space area";
      return {
        unit: type,
        name: E.unitName(type, editing ? 1 : count) as string,
        subtitle: editing
          ? `${unit.per ? `${plural(unit.cost!, "resource")} for ${unit.per}` : plural(unit.cost!, "resource")} · ${STOCK[type]! - count} left in supply`
          : `Placed ${where}`,
        counter: editing
          ? {
              id: `units.${type}`,
              value: count,
              max: STOCK[type]!,
              label: unit.name,
            }
          : null,
        quantity: editing ? null : count,
        placement:
          editing && unit.ground && count
            ? {
                options: [
                  { id: "starpoint", label: "Starpoint" },
                  { id: "space", label: "In space" },
                ],
                selected: data.place[type] as string,
              }
            : null,
      };
    });
  return {
    kind: "production",
    skipped: null,
    title: "Space dock · Starpoint",
    subtitle: `Production ${total.capacity} · Starpoint 3 resources + 2`,
    rows,
    gauges: [
      { label: "Production capacity", used: total.count, total: total.capacity },
      { label: "Resources staged", used: total.paid, total: total.cost },
      { label: "Fleet supply after", used: total.ships, total: SUPPLY },
      { label: "Fighters and cargo in space", used: total.cargo, total: total.room },
    ],
    payment: {
      title: editing ? "Payment" : draft ? "Planned payment" : "Payment",
      cost: total.cost,
      paid: total.paid,
      unit: "resource",
      editable: editing,
      summary: paySummary(data.pay),
      goods: null,
      actions: [],
    },
    done:
      done && !editing
        ? {
            strong: draft ? "Production drafted." : "Units produced.",
            text: draft ? "Nothing is spent until you apply." : "The units are on the board.",
          }
        : null,
  };
}

function stepTrail(state: State, step: number): Trail {
  const status = E.stepState(state, step);
  const editing = state.edit?.step === step;
  const over = status === "done";
  const mark = (active: boolean): TrailStatus => (over ? "complete" : active ? "active" : "todo");
  if (status === "skipped" || status === "future") {
    return [];
  }
  if (step === 1) {
    const pds = state.planets.some((planet: any) => E.hostile(planet) && E.shielded(planet));
    const cannon = state.boundary === "cannon";
    const rift = state.boundary === "rift";
    const staged = cannon || rift;
    const rolls = E.riftExits(state, E.editedData(state, 1)).length;
    return [
      { label: "Move ships", status: staged ? "complete" : mark(editing) },
      { label: "Load cargo", status: staged ? "complete" : mark(editing) },
      ...(rolls
        ? [
            {
              label: "Gravity rift",
              status: rift ? ("active" as const) : cannon ? ("complete" as const) : mark(false),
            },
          ]
        : []),
      {
        label: "Space cannon offense",
        status: pds ? (cannon ? "active" : mark(false)) : "skipped",
        reason: "no PDS",
      },
    ];
  }
  if (step === 2) {
    const battle = state.battle;
    const afb = battle
      ? battle.records.some((rec: any) => rec.key === "afb")
      : SIDES.some((side) =>
          E.types(side === "att" ? state.fleet : state.enemyFleet).some(
            (type: UnitType) => U[type].afb,
          ),
        );
    return [
      {
        label: "Barrage",
        status: afb ? (battle ? "complete" : "active") : "skipped",
        reason: "no eligible units",
      },
      { label: "Combat rounds", status: battle ? mark(battle.stage !== "retreat") : "todo" },
      {
        label: "Retreat",
        status: battle?.retreat ? "complete" : over ? "skipped" : mark(battle?.stage === "retreat"),
        reason: "none",
      },
    ];
  }
  if (step === 3) {
    const inv = state.inv;
    const enemies = state.planets.filter(
      (planet: any) => E.hostile(inv.before[planet.id]) && E.size(inv.before[planet.id].units),
    );
    const skip: string | undefined = enemies
      .map((planet: any) => inv.bombard[planet.id])
      .find(Boolean);
    const kind = state.boundary;
    const fighting = status === "battle";
    const past = (...kinds: string[]) => over || fighting || kinds.includes(kind);
    return [
      {
        label: "Bombardment",
        status: !enemies.length
          ? "skipped"
          : skip
            ? "skipped"
            : kind === "bombard"
              ? "active"
              : "complete",
        reason: skip || "no defenders",
      },
      {
        label: "Commit forces",
        status: past("cannon-defense", "ground")
          ? "complete"
          : kind === "bombard"
            ? "todo"
            : "active",
      },
      {
        label: "Space cannon defense",
        status: !enemies.some((planet: any) => E.shielded(inv.before[planet.id]))
          ? "skipped"
          : kind === "cannon-defense"
            ? "active"
            : past("ground")
              ? "complete"
              : "todo",
        reason: "no PDS",
      },
      {
        label: "Ground combat",
        status: !enemies.length
          ? "skipped"
          : fighting || kind === "ground"
            ? "active"
            : over
              ? "complete"
              : "todo",
        reason: "no defenders",
      },
      { label: "Control", status: over ? "complete" : "todo" },
    ];
  }
  if (step === 4) {
    return ["Build units", "Payment", "Placement"].map((label) => ({
      label,
      status: mark(editing),
    }));
  }
  return [];
}

/** Explanations of the open step. The panel shows them on hover or click only. */
function stepHelp(state: State, step: number): HelpView[] {
  const text = stepLede(state, step);
  return text ? [{ title: STEPS[step], text }] : [];
}

function stepLede(state: State, step: number): string {
  const status = E.stepState(state, step);
  const editing = state.edit?.step === step;
  if (status === "boundary") {
    return (
      (
        {
          rift: "Ships that leave a gravity rift roll for survival.",
          cannon: "Space cannon offense needs a live roll.",
          combat: "Space combat needs live rolls.",
          bombard:
            "Bombardment rolls before you commit ground forces, so landings are chosen in Live.",
          "cannon-defense": "Space cannon defense rolls against your landing forces.",
          ground: "Ground combat needs live rolls.",
        } as Record<string, string>
      )[state.boundary] +
      " The draft holds no dice and no predicted winner. Apply to continue in Live."
    );
  }
  if (editing && status === "needs-review") {
    return "A recorded choice no longer fits. The affected rows are marked.";
  }
  if (editing && step !== state.frontier) {
    return "Your recorded choices stay intact until you commit these edits. Later steps are rechecked.";
  }
  return "";
}

function taskFooter(state: State): FooterView {
  const step: number = state.selected;
  const draft = state.mode === "draft";
  const battle = E.activeBattle(state);
  const viewer: string = E.env.viewer;
  const foot = (note: string, actions: FooterView["actions"] = [], error = false): FooterView => ({
    note,
    actions,
    error,
  });
  const apply = button(
    { type: "openApply" },
    "Apply to Live",
    "primary",
    viewer !== "sol" || !!state.edit,
    true,
  );
  if (state.edit?.step === step) {
    const error: string = E.validation(state);
    const current = step === state.frontier;
    if (draft && step === 1) {
      // A draft records every change of the movement: there is nothing to send and nothing to
      // reset here. The footer only says why a movement is not recorded.
      return foot(error, [], !!error);
    }
    const verb = (
      draft
        ? ["Preview activation", "Preview movement", "", "Preview landings", "Preview production"]
        : ["Activate system", "Move fleet", "", "Commit ground forces", "Produce units"]
    )[step];
    // Moving nothing is a move: the main button says so.
    const nothing = step === 1 && !E.stagedEntries(state.edit.value).length;
    const back =
      state.edit.dirty || !current
        ? [button({ type: "cancelEdit" }, current ? "Reset selection" : "Cancel edits")]
        : [];
    return {
      ...foot(
        error || (draft ? "Private to you until you apply." : "This commits to the live game."),
        [
          ...back,
          button({ type: "commitEdit" }, nothing ? "Move nothing" : verb, "primary", !!error, true),
        ],
        !!error,
      ),
      // The open payment is one line here; its controls are on the board.
      payment: productionPayment(state) ?? undefined,
    };
  }
  if (state.frontier !== null && step !== state.frontier) {
    return foot(
      `Viewing ${STEPS[step].toLowerCase()}. The action is at ${STEPS[state.frontier].toLowerCase()}.`,
      [
        button(
          { type: "returnToCurrent" },
          `${draft ? "Go to" : "Continue to"} ${STEPS[state.frontier].toLowerCase()}`,
          draft ? "quiet" : "primary",
          false,
          true,
        ),
      ],
    );
  }
  if (state.frontier === null) {
    return foot(
      draft
        ? "Every step is drafted. Review, edit, or apply."
        : "Action complete. Every step keeps its result.",
      draft ? [apply] : [],
    );
  }
  if (state.blocker === "boundary") {
    return foot("Later steps unlock after this live result.", [apply]);
  }
  if (!mine(state)) {
    return foot(`Waiting for ${waitingFor(state)} · ${E.need(state).what}.`, [
      button({ type: "simulate" }, `Simulate ${waitingFor(state)} (demo)`, "demo"),
    ]);
  }
  const reaction = battleReaction(state);
  if (reaction) {
    return foot(reaction.note, reaction.actions);
  }
  if (battle?.stage === "pre") {
    const side = SIDES.find((name) => E.controls(state, name));
    const retreat =
      battle.kind === "space" && !battle.announced.att && !battle.announced.def
        ? [button({ type: "battle", action: "announceRetreat" }, "Announce retreat")]
        : [];
    return foot(
      `Round ${battle.round} · Both sides roll at once.${side && battle.boost[side] ? " Morale Boost applies." : ""}`,
      [
        ...retreat,
        button({ type: "battle", action: "roll" }, "Roll combat dice", "primary", false, true),
      ],
    );
  }
  if (battle?.stage === "assign") {
    const owed: number = battle.owed[battle.side];
    return foot(
      `${battle.staged.length} of ${plural(owed, "hit")} assigned. Choose on your rows.`,
      [
        ...(battle.staged.length
          ? [button({ type: "battle", action: "resetHits" }, "Reset selection")]
          : []),
        button(
          { type: "battle", action: "assignHits" },
          `Assign ${plural(owed, "hit")}`,
          "primary",
          battle.staged.length < owed,
        ),
      ],
    );
  }
  if (battle?.stage === "retreat") {
    return foot("Your surviving ships leave the active system.", [
      ...(battle.pick ? [button({ type: "clearChoice" }, "Clear choice")] : []),
      button(
        { type: "battle", action: "retreat" },
        battle.pick ? `Retreat to ${sysLabel(battle.pick)}` : "Choose a destination",
        "primary",
        !battle.pick,
      ),
    ]);
  }
  return foot("");
}

function task(state: State): TaskView {
  const step: number = state.selected;
  const status: string = E.stepState(state, step);
  const editing = state.edit?.step === step;
  const draft = state.mode === "draft";
  const pill: PillView =
    status === "skipped"
      ? { tone: "quiet", label: "Skipped" }
      : status === "boundary"
        ? { tone: "draft", label: "Preview ends here" }
        : status === "needs-review"
          ? { tone: "alert", label: "Needs review" }
          : status === "done"
            ? editing
              ? { tone: "draft", label: "Editing" }
              : { tone: "done", label: draft ? "Drafted" : "Resolved" }
            : draft
              ? { tone: "draft", label: "Preparing" }
              : mine(state)
                ? { tone: "live", label: `Your decision · ${E.need(state).what}` }
                : {
                    tone: "quiet",
                    label: `Waiting for ${waitingFor(state)} · ${E.need(state).what}`,
                  };
  const canEdit = draft && KEYS[step] && status === "done" && !editing;
  const content: StepContentView = [activation, movement, combat, invasion, production][step](
    state,
  );
  return {
    step,
    title: STEPS[step],
    pill,
    trail: stepTrail(state, step),
    edit: canEdit
      ? {
          label: `Edit ${step === 3 ? "landings" : STEPS[step].toLowerCase()}`,
          step,
          disabled: !!state.edit?.dirty,
          hint: state.edit?.dirty
            ? `Commit or reset the ${STEPS[state.edit.step].toLowerCase()} changes first`
            : undefined,
        }
      : null,
    help: stepHelp(state, step),
    content,
    footer: taskFooter(state),
  };
}

export function selectTactical(world: World, state: State): TacticalActionView {
  const draft = state.mode === "draft";
  const done = state.frontier === null;
  const target: string | null =
    state.edit?.step === 0 ? state.edit.value.system : E.activeId(state);
  const dirtyElsewhere = state.edit?.dirty && state.selected !== state.edit.step;
  const view = task(state);
  // The draft was opened from the action picker: the player can go back; the draft stays.
  if (world.mode === "draft" && world.viewer === "sol" && world.workspaces.live.kind === "picker") {
    view.footer.actions.unshift(button({ type: "backToPicker" }, "Back to actions"));
  }
  const closing = world.mode === "live" ? closingBlocks(state) : [];
  if (closing.length) {
    view.footer.actions.push(END_TURN);
  }
  withKeys(view.footer);
  return {
    kind: "tactical",
    title: "Tactical action",
    subtitle: `${target ? sysLabel(target) : "No system chosen"} · ${P.sol.name} (${P.sol.faction})`,
    badge: {
      tone: draft ? "draft" : done ? "done" : "live",
      label: draft ? "Private draft" : done ? "Complete" : "In progress",
    },
    past: world.mode === "history" ? { label: state.past.label } : null,
    tabs: STEPS.map((name, step) => {
      const status = E.stepState(state, step);
      return {
        name,
        status,
        caption: stepCaption(state, step, status),
      };
    }),
    selected: state.selected,
    current: state.frontier,
    contentKey: `${state.kind}:${state.selected}`,
    uncommitted: dirtyElsewhere ? { step: state.edit.step, name: STEPS[state.edit.step] } : null,
    interrupt: world.mode === "history" ? null : (battleReaction(state)?.interrupt ?? null),
    closing,
    task: view,
  };
}

/** What "Apply to Live" will commit. */
export function applyItems(state: State): string[] {
  const items = [
    `Activate ${systemName(state)} · #${state.data.activation.system}`,
    `Move ${E.list(state.fleet)}`,
  ];
  if (state.ground.infantry + state.ground.mech) {
    items.push(`Transport ${E.list(E.force(state.ground))}`);
  }
  if (state.done[3]) {
    items.push(
      ...state.planets
        .filter((planet: any) => E.size(E.landed(state, planet.id)))
        .map((planet: any) => `Land ${E.list(E.landed(state, planet.id))} on ${planet.name}`),
    );
  }
  if (state.done[4]) {
    items.push(
      `Produce ${plural(E.productionTotals(state, state.data.production).count, "unit")} for ${plural(E.productionTotals(state, state.data.production).cost, "resource")}`,
    );
  }
  return items;
}

export { countsForce };
