// Small derivations that several selectors share: forces, captions, who must act.
import type {
  ActionButtonView,
  BlockView,
  FooterView,
  Force,
  Intent,
  InterruptView,
  PaymentView,
  PlanetTraitsView,
  MenuRowView,
  SeatId,
  SeatView,
  UnitType,
} from "../../model";
import {
  ATTACHMENTS,
  MAP,
  P,
  PAY,
  PLANET_TYPES,
  REACTION_CARDS,
  SEAT,
  SEATS,
  STEPS,
  plural,
} from "../data";
import { E } from "../loose";
import type { State, World } from "../world";

export type Group = Partial<Record<UnitType, { n: number; dmg: number }>>;

export const toForce = (group: Group): Force =>
  (E.types(group) as UnitType[]).map((unit) => ({
    unit,
    count: group[unit]!.n,
    damaged: group[unit]!.dmg,
  }));
export const countsForce = (counts: Partial<Record<UnitType, number>>): Force =>
  toForce(E.force(counts));

export const seatViews: Record<SeatId, SeatView> = Object.fromEntries(
  Object.entries(SEATS).map(([id, seat]) => [
    id,
    { id, name: seat.name, faction: seat.faction, symbol: seat.symbol, color: seat.color },
  ]),
);

/** What the exploration says about a planet of the demo: a planet of nobody is still explored. */
export const traitsOf = (planet: { id: string; owner?: string | null }): PlanetTraitsView => ({
  types: PLANET_TYPES[planet.id] ?? [],
  unexplored: !planet.owner && !!PLANET_TYPES[planet.id],
  attachments: ATTACHMENTS[planet.id] ?? [],
});

export const isTactical = (state: State) => state.kind === "tactical";
export const systemName = (state: State) => MAP[state.data.activation.system].name;
export const waitingFor = (state: State): string => E.waitingFor(state);
export const mine = (state: State): boolean => E.mine(state);

export const button = (
  intent: Intent,
  label: string,
  tone: ActionButtonView["tone"] = "quiet",
  disabled = false,
  arrow = false,
): ActionButtonView => ({ intent, label, tone, disabled, arrow });

/**
 * A reaction window, the same inside and outside a battle: the cards that the viewer can play
 * now as a list on top of the open action, and the buttons of the footer. Pass is the main
 * button; a staged card changes it to Play.
 */
export function reactionParts(
  trigger: string,
  link: string[],
  cards: string[],
  pick: string | null,
): { interrupt: InterruptView; note: string; actions: ActionButtonView[] } {
  return {
    interrupt: {
      eyebrow: `Reaction · ${trigger}`,
      link,
      blocks: [
        {
          kind: "menu",
          rows: cards.map((card, index) => ({
            key: `${index + 1}`,
            title: card,
            text: REACTION_CARDS[card],
            selected: pick === card,
            disabled: false,
            intent: { type: "stageReaction", card },
          })),
        },
      ],
      actions: [button({ type: "openReference", sheet: "cards" }, "Offer settings")],
    },
    note: pick ? "This commits to the live game." : "Reaction window. Passing keeps your cards.",
    actions: pick
      ? [
          button({ type: "clearChoice" }, "Clear choice"),
          button({ type: "reaction", action: "play" }, `Play ${pick}`, "primary", false, true),
        ]
      : [button({ type: "reaction", action: "pass" }, "Pass", "primary")],
  };
}

/** The reaction window of a battle that the viewer must answer, or null. */
export function battleReaction(state: State) {
  const battle = isTactical(state) ? E.activeBattle(state) : null;
  const system = [`sys:${state.data?.activation.system}`];
  if (E.startWindow(state, battle)) {
    return reactionParts(
      `Start of combat round ${battle.round}`,
      system,
      ["Morale Boost"],
      battle.start.pick,
    );
  }
  if (battle?.stage !== "reaction" || !E.controls(state, battle.reaction.side)) {
    return null;
  }
  const { target, type, pick } = battle.reaction;
  return reactionParts(
    `After ${P[SEAT[target as "att"]].name}’s ${E.unitName(type)} used Sustain Damage`,
    system,
    ["Direct Hit"],
    pick || null,
  );
}

/** Auto-pay stages the payment with the least waste on the map. "Clear payment" removes the staged payment. */
/** The counter of the trade goods in a payment that costs `cost` and has `paid` staged. */
export function goodsView(cost: number, paid: number, pay: Record<string, boolean | number>) {
  const source = E.GOODS;
  if (!source) {
    return null;
  }
  const value: number = E.payValue(pay, source);
  const rest = value + cost - paid;
  return {
    id: source.id,
    value,
    max: source.res,
    rest: cost !== paid && rest >= 0 && rest <= source.res ? rest : null,
  };
}

/** What pays now, with the trade goods: "Jord 4 + Vefut 2 + 1 trade good". */
export const paySummary = (pay: Record<string, boolean | number>, key: "res" | "inf" = "res") =>
  PAY.filter((source) => E.payValue(pay, source, key))
    .map((source) =>
      source.system
        ? `${source.label.replace("Exhaust ", "")} ${E.payValue(pay, source, key)}`
        : plural(E.payValue(pay, source, key), "trade good"),
    )
    .join(" + ");

/** A payment that is open: its state, and the controls that the board shows. */
export const openPayment = (
  cost: number,
  key: "res" | "inf",
  pay: Record<string, boolean | number>,
): PaymentView => ({
  title: "Payment",
  cost,
  paid: E.payTotal(pay, key),
  unit: key === "inf" ? "influence" : "resources",
  editable: true,
  summary: paySummary(pay, key),
  goods: goodsView(cost, E.payTotal(pay, key), pay),
  actions: paymentActions(cost, pay),
});

/** The payment of the production step while the player edits it. Null in any other state. */
export function productionPayment(state: State): PaymentView | null {
  if (state.edit?.step !== 4) {
    return null;
  }
  const data = E.editedData(state, 4);
  return openPayment(E.productionTotals(state, data).cost, "res", data.pay);
}

export const paymentActions = (
  cost: number,
  pay: Record<string, boolean | number>,
): ActionButtonView[] => [
  button({ type: "payment", action: "auto" }, "Auto-pay", "quiet", cost <= 0),
  button(
    { type: "payment", action: "reset" },
    "Clear payment",
    "quiet",
    !Object.values(pay).some(Boolean),
  ),
];

export const END_TURN = button({ type: "endTurn" }, "End turn", "primary", false, true);

/**
 * Enter is the main button of the footer. Esc goes one level back: it drops the edits or the
 * staged choice, then leaves the action.
 */
export function withKeys(footer: FooterView): FooterView {
  const main = footer.actions.find((action) => action.tone === "primary");
  if (main) {
    main.key = "Enter";
  }
  const back = ["cancelEdit", "clearChoice", "backToPicker"]
    .map((type) => footer.actions.find((action) => action.intent.type === type))
    .find(Boolean);
  if (back) {
    back.key = "Escape";
  }
  return footer;
}

/** Transactions do not use the action of the turn. The demo has no trade desk yet. */
export const tradeRow: MenuRowView = {
  group: "Trade",
  key: "r",
  title: "Transaction",
  state: "Not in the demo",
  hint: "One transaction with each neighbour in your turn. It does not use your action.",
  disabled: true,
  intent: { type: "backToPicker" },
};

/** After the viewer's action: what does not use an action, until they end the turn. */
export function closingBlocks(state: State): BlockView[] {
  if (!E.turnClosing(state)) {
    return [];
  }
  return [{ kind: "menu", title: "Before you end your turn", rows: [tradeRow] }];
}

export function stepCaption(state: State, step: number, status: string): string {
  if (status === "future") {
    return "";
  }
  if (status === "skipped") {
    return `Skipped · ${state.skipped[step]}`;
  }
  if (status === "boundary") {
    return "Preview ends here";
  }
  if (status === "needs-review") {
    return "Needs review";
  }
  if (status === "decision") {
    return state.mode === "draft"
      ? "Preparing"
      : mine(state)
        ? "Your decision"
        : `Waiting for ${waitingFor(state)}`;
  }
  const draft = state.mode === "draft";
  if (step === 0) {
    return `#${state.data.activation.system} ${systemName(state)}`;
  }
  if (step === 1) {
    const moved = E.originTotals(state.data.movement);
    return `${plural(moved.ships, "ship")} · ${moved.cargo} cargo`;
  }
  if (step === 2) {
    const battle = state.battle;
    if (battle.stage !== "done") {
      return `Round ${battle.round} · ${mine(state) ? "your decision" : "waiting"}`;
    }
    return `${battle.winner ? P[SEAT[battle.winner as "att"]].faction + " won" : "Both fleets destroyed"} · ${plural(battle.round, "round")}`;
  }
  if (step === 3) {
    if (status === "battle") {
      return "Ground combat";
    }
    const count = state.planets.reduce(
      (sum: number, planet: any) => sum + E.size(E.landed(state, planet.id)),
      0,
    );
    if (draft) {
      return `${count} planned to land`;
    }
    const result = (kind: string) =>
      state.planets.find((planet: any) => state.inv.results[planet.id] === kind)?.name;
    return result("captured")
      ? `${result("captured")} captured`
      : result("held")
        ? `${result("held")} held by Hacan`
        : result("claimed")
          ? `${result("claimed")} claimed`
          : `${count} landed`;
  }
  const total = E.productionTotals(state, state.data.production);
  return `${plural(total.count, "unit")} · ${plural(total.cost, "resource")}`;
}

export function statusText(world: World, state: State): string {
  if (world.mode === "history") {
    return "Viewing a past action";
  }
  if (isTactical(state)) {
    const draft = state.mode === "draft";
    const done = state.frontier === null;
    return draft
      ? "Your turn · Preparing a tactical action"
      : done
        ? "Tactical action complete"
        : mine(state)
          ? `Your decision · ${E.need(state).what}`
          : `Waiting for ${waitingFor(state)} · ${E.need(state).what}`;
  }
  const wait = E.flowNeed(state);
  if (!wait) {
    return state.kind === "decision" && !state.flow.card
      ? "Decision resolved"
      : `${state.kind === "strategic" ? "Strategic" : "Component"} action complete`;
  }
  if (state.kind === "picker") {
    return "Your turn · Choose an action";
  }
  if (state.kind === "waiting") {
    return `${state.flow.passed ? "You passed" : "Turn ended"} · ${SEATS[wait.seat].name} is next`;
  }
  return wait.mine
    ? `Your decision · ${wait.text}`
    : `Waiting for ${SEATS[wait.seat].name} · ${wait.text}`;
}

/** One line per stage: for a tactical action these are the stepper captions. */
export function stagesOf(state: State): [string, string][] {
  if (isTactical(state)) {
    return STEPS.map(
      (name, step) =>
        [name, stepCaption(state, step, E.stepState(state, step))] as [string, string],
    ).filter(([, text]) => text);
  }
  const flow = state.flow;
  if (state.kind === "component") {
    return [[flow.card, flow.result || "In progress"]];
  }
  if (state.kind === "decision") {
    return [[flow.decision, flow.result || "In progress"]];
  }
  return [
    ["Primary", flow.primaryResult || "In progress"],
    ...flow.order
      .filter((seat: string) => flow.results[seat])
      .map(
        (seat: string) =>
          [`${SEATS[seat].name} · secondary`, flow.results[seat]] as [string, string],
      ),
  ];
}

export function summaryOf(state: State): string {
  if (state.kind !== "strategic") {
    return stagesOf(state)
      .map(([, text]) => text)
      .join(" · ");
  }
  const flow = state.flow;
  return [
    flow.primaryResult || "Primary in progress",
    ...flow.order
      .filter((seat: string) => flow.results[seat])
      .map(
        (seat: string) => `${SEATS[seat].name} ${flow.results[seat].split(" · ")[0].toLowerCase()}`,
      ),
  ].join(" · ");
}

/** The step being edited in view decides what the map shows. */
export const mapStep = (state: State): number | null =>
  state.edit && state.edit.step === state.selected && [0, 1, 4].includes(state.edit.step)
    ? state.edit.step
    : null;
