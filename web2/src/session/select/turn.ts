// The turn menu of the action phase: the action picker, its lists of cards, a staged action, and
// what the seat can still do before it ends its turn. The engine offers the options and says the
// facts of the menu (`details` of its question, `kind: "turn_menu"`); this file sorts them into
// the fixed groups of the picker. No rule is decided here, and no rule text is written here.
import type {
  ActionButtonView,
  BlockView,
  FlowActionView,
  FooterView,
  MenuRowView,
} from "../../model";
import { plural } from "../../model";
import type { Catalog, Choice, ChoiceOption, SessionUpdate } from "../wire";
import type { LocalState } from "./action";
import { systemLabel, tilesOf } from "./movement";
import { factionName, seatLabel, strategyCard } from "./names";
import { ACTION_CARD, type Printed, printedOption } from "./printed";

/** A list of the picker that the player opened: one row of the picker stands for all of them. */
export type TurnMenu = "actionCards" | "components" | "trade";

const KEYS = "123456789";
const TACTICAL = "tactical";
const PASS = "pass";
const END_TURN = "end_turn";

const isStrategic = (option: ChoiceOption) =>
  option.id === "strategic" || option.id.startsWith("strategic|");
const isTrade = (option: ChoiceOption) => option.kind === "open_transaction";
const isActionCard = (option: ChoiceOption) => option.id.startsWith(ACTION_CARD);
const isEnd = (option: ChoiceOption) => option.kind === END_TURN || option.id === END_TURN;
/** Everything that is not one of the fixed rows is a component of the second list. */
const isComponent = (option: ChoiceOption) =>
  option.id !== TACTICAL &&
  option.id !== PASS &&
  !isEnd(option) &&
  !isStrategic(option) &&
  !isTrade(option) &&
  !isActionCard(option);

/** The question of the turn menu, also while the seat closes its turn. */
export const isTurnMenu = (choice: Choice | null): choice is Choice =>
  choice?.details?.kind === "turn_menu";

interface HeldCard {
  card: string;
  used: boolean;
  /** The option that plays the card. Null when the game does not offer it now. */
  option: string | null;
}

interface Partner {
  seat: string;
  faction: string;
  reason: string | null;
  trade_goods: number;
  commodities: number;
  promissory_notes: number;
}

/** The facts of the menu, as the engine sends them with its question. */
function factsOf(choice: Choice) {
  const details = choice.details ?? {};
  const tokens = details.tokens as { tactic: number; fleet: number; strategy: number } | undefined;
  return {
    closing: details.closing === true,
    tokens: tokens ?? null,
    cards: (Array.isArray(details.strategy_cards) ? details.strategy_cards : []) as HeldCard[],
    partners: (Array.isArray(details.partners) ? details.partners : []) as Partner[],
    effects: (details.action_card_effects ?? {}) as Record<
      string,
      { kind?: string; planets?: { exhausted?: boolean }[] }
    >,
  };
}

/** What the play of a card would touch, as the engine says it. Undefined when it says nothing. */
function effectOf(effect: { kind?: string; planets?: { exhausted?: boolean }[] } | undefined) {
  if (effect?.kind !== "ready_cultural" || !effect.planets) {
    return undefined;
  }
  const exhausted = effect.planets.filter((planet) => planet.exhausted).length;
  return `Readies ${exhausted} of ${plural(effect.planets.length, "cultural planet")}`;
}

/** The main button of the footer. */
const main = (label: string, intent: ActionButtonView["intent"], disabled = false) => ({
  label,
  tone: "primary" as const,
  key: "Enter",
  arrow: true,
  disabled,
  intent,
});

/** One level back: from a staged action or a list of cards to the picker. */
const BACK: ActionButtonView = {
  label: "Back to actions",
  key: "Escape",
  intent: { type: "backToPicker" },
};

/** What an option of the menu is, as printed: the row of its list, and the screen when it is staged. */
interface Entry extends Printed {
  option: ChoiceOption;
  /** What its play would change, in game values. */
  state?: string;
  /** The word of the main button: "Play", "Use". */
  verb: string;
}

export function selectTurn(
  update: SessionUpdate,
  local: LocalState,
  choice: Choice,
  printed: Catalog | null,
  /** Undo, when the last answer can be taken back. */
  undo: ActionButtonView[],
): FlowActionView {
  const facts = factsOf(choice);
  const offered = new Map(choice.options.map((option) => [option.id, option]));
  const tiles = tilesOf(update);
  const entry = (option: ChoiceOption): Entry => {
    const card = printedOption(update, option, printed);
    return {
      ...card,
      option,
      state: isActionCard(option) ? effectOf(facts.effects[option.id]) : undefined,
      verb: isActionCard(option) ? "Play" : "Use",
    };
  };
  const cards = choice.options.filter(isActionCard).map(entry);
  const components = choice.options.filter(isComponent).map(entry);
  const strategy = facts.cards.map((held) => {
    const card = strategyCard(held.card, printed);
    const record = printed?.records.strategy_cards.find((item) => item.id === held.card);
    return { ...held, ...card, primary: record?.primary_text ?? "" };
  });
  const partners = facts.partners.map((partner) => ({
    ...partner,
    name: seatLabel(update, partner.seat, printed),
    option:
      choice.options.find(
        (option) => isTrade(option) && option.id === `component|trade|${partner.faction}`,
      ) ?? null,
  }));
  // A partner that the facts do not name is still offered, by the name of the option.
  const unnamed = choice.options
    .filter(isTrade)
    .filter((option) => !partners.some((partner) => partner.option?.id === option.id));
  const trades = [
    ...partners,
    ...unnamed.map((option) => ({
      seat: "",
      faction: option.id.replace("component|trade|", ""),
      name: factionName(option.id.replace("component|trade|", ""), printed),
      reason: null,
      trade_goods: 0,
      commodities: 0,
      promissory_notes: 0,
      option,
    })),
  ];
  const tradeCount = trades.filter((partner) => partner.option).length;
  const unused = strategy.filter((card) => !card.used);
  const draftSystem = local.drafted?.system ?? null;

  const frame = (parts: {
    subtitle: string;
    heading: string;
    blocks: BlockView[];
    closing?: BlockView[];
    footer: FooterView;
    staged?: boolean;
    help?: FlowActionView["help"];
  }): FlowActionView => ({
    kind: "flow",
    title: "Your turn",
    subtitle: parts.subtitle,
    badge: parts.staged ? { tone: "draft", label: "Not sent" } : null,
    past: null,
    tabs: [],
    selected: 0,
    current: null,
    contentKey: `${update.pending_choice?.nonce ?? "idle"}:${local.menu ?? ""}:${local.staged ?? ""}`,
    interrupt: null,
    closing: parts.closing ?? [],
    heading: parts.heading,
    pill: { tone: "live", label: "Your decision" },
    trail: [],
    help: parts.help ?? [],
    blocks: parts.blocks,
    footer: parts.footer,
  });

  // A staged action: what it is, as printed. The main button sends it.
  const staged = local.staged ? offered.get(local.staged) : undefined;
  if (staged) {
    const held = strategy.find((card) => card.option === staged.id);
    const partner = trades.find((item) => item.option?.id === staged.id);
    const item = held || partner || staged.id === PASS ? null : entry(staged);
    const what = held
      ? {
          heading: `${held.number} ${held.name}`,
          name: held.name,
          type: "Strategic action",
          text: held.primary,
          button: `Use ${held.name}`,
        }
      : partner
        ? {
            heading: `Transaction with ${partner.name}`,
            name: partner.name,
            type: "Transaction",
            text: "",
            button: "Open transaction",
          }
        : item
          ? { heading: item.name, ...item, button: `${item.verb} ${item.name}` }
          : { heading: "Pass", name: "Pass", type: "Pass", text: "", button: "Pass" };
    return frame({
      subtitle: what.type,
      heading: what.heading,
      staged: true,
      blocks: what.text
        ? [{ kind: "source", name: what.name, type: what.type, text: what.text }]
        : [
            {
              kind: "note",
              tone: "accent",
              strong: what.heading,
              text: "Staged. Nothing is sent yet.",
            },
          ],
      footer: {
        note: "This commits to the live game.",
        error: false,
        actions: [...undo, BACK, main(what.button, { type: "flow", action: "resolve" })],
      },
    });
  }

  // A list that one row of the picker stands for.
  const list = (title: string, rows: MenuRowView[], note: string): FlowActionView =>
    frame({
      subtitle: facts.closing ? "Before you end your turn" : "Choose one action",
      heading: title,
      blocks: [{ kind: "menu", rows }],
      footer: { note, error: false, actions: [...undo, BACK] },
    });
  const listRows = (entries: Entry[], text: boolean): MenuRowView[] =>
    entries.map((item, index) => ({
      key: KEYS[index] ?? "",
      group: undefined,
      title: item.name,
      // A list where the player chooses one card shows its printed text.
      text: text ? item.text || undefined : undefined,
      hint: text ? undefined : item.text || undefined,
      state: item.state ?? (text ? undefined : item.type),
      disabled: false,
      intent: { type: "chooseOption", option: item.option.id },
    }));
  if (local.menu === "actionCards" && cards.length) {
    return list("Play an action card", listRows(cards, true), `Choose 1 of ${cards.length}.`);
  }
  if (local.menu === "components" && components.length) {
    return list("Use a component", listRows(components, true), `Choose 1 of ${components.length}.`);
  }
  if (local.menu === "trade" && trades.length) {
    return list(
      "Transaction",
      trades.map((partner, index) => ({
        key: partner.option ? (KEYS[index] ?? "") : "",
        title: partner.name,
        state: partner.option
          ? `${plural(partner.trade_goods, "trade good")} · ${plural(partner.commodities, "commodity", "commodities")} · ${plural(partner.promissory_notes, "promissory note")}`
          : (partner.reason ?? "Not now"),
        disabled: !partner.option,
        intent: { type: "chooseOption", option: partner.option?.id ?? "" },
      })),
      `${tradeCount} of ${plural(trades.length, "player")} can trade now.`,
    );
  }

  // The fixed groups in their fixed order. A row that cannot be used stays and says why.
  const tactical = offered.get(TACTICAL);
  const rows: MenuRowView[] = [
    {
      group: "Tactical",
      key: "t",
      title: draftSystem ? "Open your draft" : "Activate a system",
      state: draftSystem
        ? systemLabel(tiles, draftSystem)
        : facts.tokens
          ? plural(facts.tokens.tactic, "tactic token")
          : undefined,
      hint: "Opens the tactical action as your private draft. You choose the system on the map; nothing is sent until you apply it.",
      disabled: !tactical,
      intent: { type: "pickAction", action: "tactical" },
    },
    // One row for each strategy card. The key is its number.
    ...strategy.map((card, index): MenuRowView => ({
      group: index ? undefined : "Strategic",
      key: `${card.number}`,
      title: `${card.number} ${card.name}`,
      state: card.used ? "Used in this round" : card.option ? undefined : "Not now",
      hint: card.primary || undefined,
      disabled: !card.option || !offered.has(card.option),
      intent: { type: "pickAction", action: "strategic", card: card.option ?? "" },
    })),
    // One row for the action cards and one for the other components, whatever the player holds:
    // the second list asks which one. So the picker is the same in every turn of a game.
    {
      group: "Component",
      key: "a",
      title: "Play an action card",
      state: cards.length ? `${cards.length} playable` : "None is playable",
      disabled: !cards.length,
      intent: { type: "pickGroup", group: "actionCards" },
    },
    {
      key: "c",
      title: "Use another component",
      state: components.length ? `${components.length} ready` : "None is ready",
      disabled: !components.length,
      intent: { type: "pickGroup", group: "components" },
    },
    {
      group: "Trade",
      key: "r",
      title: "Transaction",
      state: tradeCount
        ? `${tradeCount} of ${plural(trades.length, "player")}`
        : (trades.find((partner) => partner.reason)?.reason ?? "Nobody can trade now"),
      disabled: !tradeCount,
      intent: { type: "pickGroup", group: "trade" },
    },
  ];
  if (facts.closing) {
    // After the action: what does not use one, until the seat ends its turn.
    const free = rows.filter((row) => !row.disabled);
    return frame({
      subtitle: "Action taken",
      heading: "End your turn",
      blocks: [],
      closing: free.length ? [{ kind: "menu", title: "Before you end your turn", rows: free }] : [],
      footer: {
        note: "Your action is over.",
        error: false,
        actions: [...undo, main("End turn", { type: "endTurn" }, !choice.options.some(isEnd))],
      },
    });
  }
  const pass = offered.get(PASS);
  rows.push({
    group: "Pass",
    key: "p",
    title: "Take no more actions in this round",
    state: pass
      ? undefined
      : unused.length
        ? `Use ${unused.map((card) => `${card.number} ${card.name}`).join(" and ")} first`
        : "Not now",
    disabled: !pass,
    intent: { type: "pass" },
  });
  return frame({
    subtitle: "Choose one action",
    heading: "Choose an action",
    blocks: [{ kind: "menu", rows }],
    help: [
      {
        title: "Nothing is sent by a choice",
        text: "A tactical action opens as your private draft. Any other action is staged; the main button of the footer sends it. Esc goes back.",
      },
    ],
    footer: { note: "Your turn. Choose one action.", error: false, actions: [...undo] },
  });
}
