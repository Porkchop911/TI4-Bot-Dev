import type { ChoiceOptionDto, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { describeComponent } from "./componentInfo.ts";
import {
  findActionCardByName,
  findStrategyCardMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";

/**
 * The action-phase turn menu (`action phase`) and the end-turn question (`end_turn`) as a
 * persistent bar: Tactical, one Strategic button per held strategy card, Components, Action cards,
 * Trade (one partner picker), Pass and End turn. This module only maps the engine's options and
 * display facts into buttons; every enabled button carries the id of the engine option it submits.
 */

export type BarTone = "ok" | "warn" | "bad" | "neutral";

/** Text for the side panel shown while a button or menu row is hovered or focused. */
export interface BarInfo {
  kind: string;
  title: string;
  text: string;
  secondary: string | null;
  chips: { label: string; tone: BarTone }[];
  /** Planets a card's play would touch (Economic Initiative: the cultural ones), when known. */
  planets?: { id: string; name: string; trait: string; exhausted: boolean }[];
}

export interface BarAction {
  key: "tactical" | "pass" | "end";
  label: string;
  /** One line under the label while the button is enabled. */
  hint: string;
  optionId: string | null;
  enabled: boolean;
  /** Why the button is disabled; `null` when it is enabled. */
  reason: string | null;
  info: BarInfo | null;
}

export interface StrategicButton {
  cardId: string;
  /** "8. Imperial" with several cards, "Strategic action" when the seat holds a single card. */
  label: string;
  cardName: string;
  /** One line of the card's primary effect. */
  effect: string;
  optionId: string | null;
  enabled: boolean;
  reason: string | null;
  used: boolean;
  info: BarInfo;
}

export interface BarItem {
  optionId: string;
  label: string;
  detail: string | null;
  info: BarInfo;
  /** Components: kind, owner, timing and cost, shown as a line under the name. */
  facts?: string[];
  /** Components: a short extra line (borrowed through Ssruu, the technology researched). */
  note?: string | null;
}

export interface BarGroup {
  title: string;
  items: BarItem[];
}

export interface TradePartnerView {
  seat: string | null;
  faction: string;
  optionId: string | null;
  available: boolean;
  reason: string | null;
  inContact: boolean | null;
  tradeGoods: number | null;
  commodities: number | null;
  promissoryNotes: number | null;
}

export interface TokenPools {
  tactic: number;
  fleet: number;
  strategy: number;
}

export interface TurnBarModel {
  /** `readonly` is the bar shown when it is not the seat's turn. */
  mode: "active" | "closing" | "readonly";
  heading: string;
  /** In `readonly` mode, the seat whose turn it is (a raw seat id), when known. */
  waitingFor: string | null;
  tokens: TokenPools | null;
  tactical: BarAction;
  strategic: StrategicButton[];
  components: { groups: BarGroup[]; count: number; enabled: boolean; reason: string | null };
  actionCards: { items: BarItem[]; count: number; enabled: boolean; reason: string | null };
  trade: {
    partners: TradePartnerView[];
    enabled: boolean;
    reason: string | null;
    available: number;
  };
  pass: BarAction;
  end: BarAction;
}

const NOT_YOUR_TURN = "Not your turn";

export function isTurnMenuChoice(
  choice: Pick<PendingChoiceDto, "prompt" | "context" | "details"> | null,
): boolean {
  if (!choice) return false;
  if (choice.details?.kind === "turn_menu") return true;
  const subtype = choice.context?.subtype;
  return choice.prompt === "action phase" || subtype === "end_turn";
}

const isEnd = (o: ChoiceOptionDto) => o.kind === "end_turn" || o.id === "end_turn";
const isPass = (o: ChoiceOptionDto) => o.id === "pass";
const isTactical = (o: ChoiceOptionDto) => o.id === "tactical";
const isStrategic = (o: ChoiceOptionDto) => o.id === "strategic" || o.id.startsWith("strategic|");
const isTrade = (o: ChoiceOptionDto) => o.kind === "open_transaction";
const isActionCard = (o: ChoiceOptionDto) => o.id.startsWith("action_card|");

function firstSentence(text: string, max = 96): string {
  const trimmed = text.replace(/\s+/g, " ").trim();
  const match = /^.*?[.!?](?=\s|$)/.exec(trimmed);
  const sentence = match ? match[0] : trimmed;
  return sentence.length > max ? `${sentence.slice(0, max - 1).trimEnd()}…` : sentence;
}

function strategyInfo(cardId: string, used: boolean): BarInfo & { name: string; effect: string } {
  const meta = findStrategyCardMeta(cardId);
  const name = meta ? `${meta.initiative}. ${meta.name}` : humanizeId(cardId);
  return {
    kind: meta ? `Strategy card · initiative ${meta.initiative}` : "Strategy card",
    title: name,
    name,
    text: meta?.primaryText ?? "",
    secondary: meta?.secondaryText ? `Secondary: ${meta.secondaryText}` : null,
    effect: meta?.primaryText ? firstSentence(meta.primaryText) : "",
    chips: [{ label: used ? "Used this round" : "Ready", tone: used ? "warn" : "ok" }],
  };
}

/** The printed name an option label carries: "play Ghost Ship" -> "Ghost Ship". */
function playedName(label: string): string {
  return label
    .replace(/^play\s+/i, "")
    .replace(/\s+off the salvage yard$/i, "")
    .trim();
}

function componentGroupTitle(id: string): string {
  if (id.startsWith("faction|")) return "Faction abilities";
  if (id.startsWith("component|tech|")) return "Technologies";
  if (id.startsWith("component|leader|")) return "Leaders";
  if (id.startsWith("relic|") || id.startsWith("purge|")) return "Relics";
  if (id.startsWith("play_area:")) return "Exploration cards";
  if (id.startsWith("component|expedition|")) return "Thunder's Edge";
  return "Other";
}

const GROUP_ORDER = [
  "Faction abilities",
  "Technologies",
  "Leaders",
  "Relics",
  "Exploration cards",
  "Thunder's Edge",
  "Other",
];

function costChip(label: string): { label: string; tone: BarTone } | null {
  const lower = label.toLowerCase();
  if (lower.includes("strategy token")) return { label: "Cost: 1 strategy token", tone: "neutral" };
  const resources = /(\d+) resources/.exec(lower);
  if (resources) return { label: `Cost: ${resources[1]} resources`, tone: "neutral" };
  const goods = /(\d+) trade goods/.exec(lower);
  if (goods) return { label: `Cost: ${goods[1]} trade goods`, tone: "neutral" };
  if (/\bexhaust\b/.test(lower)) return { label: "Exhausts the card", tone: "neutral" };
  return null;
}

/** `details.action_card_effects[optionId]`: the planets a card readies, with their state. */
function readEffectPlanets(
  details: Record<string, unknown> | undefined,
  optionId: string,
): NonNullable<BarInfo["planets"]> | undefined {
  const effects = details?.action_card_effects;
  if (!effects || typeof effects !== "object") return undefined;
  const effect = (effects as Record<string, unknown>)[optionId];
  if (!effect || typeof effect !== "object") return undefined;
  const planets = (effect as Record<string, unknown>).planets;
  if (!Array.isArray(planets)) return undefined;
  const out: NonNullable<BarInfo["planets"]> = [];
  for (const entry of planets) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    if (typeof e.id !== "string") continue;
    out.push({
      id: e.id,
      name: typeof e.name === "string" ? e.name : e.id,
      trait: typeof e.trait === "string" ? e.trait : "cultural",
      exhausted: e.exhausted === true,
    });
  }
  return out;
}

/** One line under the menu row: which cultural planets the card readies and how many change. */
export function planetsSummary(planets: NonNullable<BarInfo["planets"]>): string {
  if (planets.length === 0) return "No cultural planets: nothing to ready";
  const names = planets.map((p) => (p.exhausted ? p.name : `${p.name} (ready)`)).join(", ");
  const changing = planets.filter((p) => p.exhausted).length;
  return `Cultural: ${names}; readies ${changing} of ${planets.length}`;
}

function itemFor(
  option: ChoiceOptionDto,
  details?: Record<string, unknown>,
  faction?: string | null,
): BarItem {
  const label = option.label;
  if (isActionCard(option)) {
    const name = playedName(label);
    const planets = readEffectPlanets(details, option.id);
    const meta = findActionCardByName(name);
    return {
      optionId: option.id,
      label: name,
      detail: planets
        ? planetsSummary(planets)
        : meta?.description
          ? firstSentence(meta.description, 120)
          : null,
      info: {
        kind: "Action card",
        title: name,
        text: meta?.description ?? "",
        secondary: null,
        chips: [{ label: `Window: ${meta?.phase ?? "Action"}`, tone: "neutral" }],
        ...(planets ? { planets } : {}),
      },
    };
  }
  const component = describeComponent(option, faction);
  const chips: BarInfo["chips"] = [];
  const cost = component.cost ?? costChip(label)?.label ?? null;
  if (cost) chips.push({ label: cost, tone: "neutral" });
  if (component.timing) chips.push({ label: `When: ${component.timing}`, tone: "neutral" });
  const kind = component.owner ? `${component.kind} · ${component.owner}` : component.kind;
  const facts = [kind, component.timing, cost].filter((f): f is string => !!f);
  return {
    optionId: option.id,
    label: component.name,
    detail: component.text,
    facts,
    note: component.note,
    info: {
      kind,
      title: component.name,
      text: component.text ?? label,
      secondary: component.note,
      chips,
    },
  };
}

interface DetailCard {
  card: string;
  used: boolean;
  option: string | null;
}

function readTokens(details: Record<string, unknown> | undefined): TokenPools | null {
  const tokens = details?.tokens;
  if (!tokens || typeof tokens !== "object") return null;
  const t = tokens as Record<string, unknown>;
  if (
    typeof t.tactic !== "number" ||
    typeof t.fleet !== "number" ||
    typeof t.strategy !== "number"
  )
    return null;
  return { tactic: t.tactic, fleet: t.fleet, strategy: t.strategy };
}

function readCards(details: Record<string, unknown> | undefined): DetailCard[] | null {
  const cards = details?.strategy_cards;
  if (!Array.isArray(cards)) return null;
  const out: DetailCard[] = [];
  for (const entry of cards) {
    if (!entry || typeof entry !== "object") return null;
    const e = entry as Record<string, unknown>;
    if (typeof e.card !== "string") return null;
    out.push({
      card: e.card,
      used: e.used === true,
      option: typeof e.option === "string" ? e.option : null,
    });
  }
  return out;
}

const num = (v: unknown): number | null => (typeof v === "number" ? v : null);

function readPartners(
  details: Record<string, unknown> | undefined,
  options: readonly ChoiceOptionDto[],
): TradePartnerView[] {
  const trades = options.filter(isTrade);
  const byFaction = (faction: string) =>
    trades.find((o) => o.id === `component|trade|${faction}`) ??
    trades.find((o) => o.label.endsWith(` ${faction}`)) ??
    null;
  const listed = details?.partners;
  if (Array.isArray(listed)) {
    const out: TradePartnerView[] = [];
    for (const entry of listed) {
      if (!entry || typeof entry !== "object") continue;
      const e = entry as Record<string, unknown>;
      if (typeof e.faction !== "string") continue;
      const option = byFaction(e.faction);
      out.push({
        seat: typeof e.seat === "string" ? e.seat : null,
        faction: e.faction,
        optionId: option?.id ?? null,
        // The offered option is the authority: a partner is only openable when it is there.
        available: option !== null,
        reason:
          option !== null
            ? null
            : typeof e.reason === "string"
              ? e.reason
              : "Not available now",
        inContact: typeof e.in_contact === "boolean" ? e.in_contact : null,
        tradeGoods: num(e.trade_goods),
        commodities: num(e.commodities),
        promissoryNotes: num(e.promissory_notes),
      });
    }
    return out;
  }
  // No details: only the partners the engine offers, named from the option itself.
  return trades.map((option) => {
    const faction = option.id.startsWith("component|trade|")
      ? option.id.slice("component|trade|".length)
      : option.label.replace(/^open a transaction with\s+/i, "");
    return {
      seat: null,
      faction,
      optionId: option.id,
      available: true,
      reason: null,
      inContact: null,
      tradeGoods: null,
      commodities: null,
      promissoryNotes: null,
    };
  });
}

function passReason(unused: number, closing: boolean): string {
  if (closing) return "Already acted: end your turn";
  if (unused <= 1) return "Use your strategy card first";
  if (unused === 2) return "Use both cards first";
  return `Use all ${unused} cards first`;
}

function actionInfo(
  kind: string,
  title: string,
  text: string,
  chips: BarInfo["chips"] = [],
): BarInfo {
  return { kind, title, text, secondary: null, chips };
}

function disabledAction(
  key: BarAction["key"],
  label: string,
  reason: string,
  hint = "",
): BarAction {
  return { key, label, hint, optionId: null, enabled: false, reason, info: null };
}

/**
 * Map a turn-menu or end-turn decision to the bar. Returns `null` when the options contain
 * something the bar cannot place (no structural option at all, a duplicated one, or an option
 * without an id), so the caller keeps the old list.
 */
export function deriveTurnBar(
  choice: Pick<PendingChoiceDto, "prompt" | "options" | "context" | "details">,
  player?: PlayerView | null,
): TurnBarModel | null {
  if (!isTurnMenuChoice(choice)) return null;
  const options = choice.options;
  if (options.length === 0 || options.some((o) => !o.id)) return null;
  const structural = options.filter(
    (o) => isEnd(o) || isPass(o) || isTactical(o) || isStrategic(o) || isTrade(o),
  );
  if (structural.length === 0) return null;
  const ends = options.filter(isEnd);
  const passes = options.filter(isPass);
  const tacticals = options.filter(isTactical);
  if (ends.length > 1 || passes.length > 1 || tacticals.length > 1) return null;

  const details = choice.details;
  const closing = details?.closing === true || ends.length > 0;
  const tokens = readTokens(details);
  const tacticLeft = tokens?.tactic ?? player?.tactic_tokens ?? null;

  // Tactical
  const tacticalOption = tacticals[0] ?? null;
  const tactical: BarAction = tacticalOption
    ? {
        key: "tactical",
        label: "Tactical action",
        hint: tacticLeft !== null ? `${tacticLeft} tactic tokens left` : "Activate a system",
        optionId: tacticalOption.id,
        enabled: true,
        reason: null,
        info: actionInfo(
          "Action",
          "Tactical action",
          "Spend 1 tactic token to activate a system, then move ships, fight and invade, and produce units.",
          tacticLeft !== null
            ? [{ label: `${tacticLeft} tactic tokens`, tone: tacticLeft > 0 ? "ok" : "bad" }]
            : [],
        ),
      }
    : disabledAction(
        "tactical",
        "Tactical action",
        tacticLeft !== null && tacticLeft < 1
          ? "No tactic tokens"
          : closing
            ? "Already acted"
            : "No system to activate",
      );

  // Strategic: one button per held card
  const strategicOptions = options.filter(isStrategic);
  let cards = readCards(details);
  if (!cards) {
    // No server details: rebuild what the options and the seat's snapshot show.
    const held = player?.strategy_cards ?? [];
    const spent = new Set(player?.exhausted_strategy_cards ?? []);
    if (held.length > 0) {
      const unusedHeld = held.filter((c) => !spent.has(c));
      cards = held.map((card) => ({
        card,
        used: spent.has(card),
        option:
          strategicOptions.find((o) => o.id === `strategic|${card}`)?.id ??
          (unusedHeld.length === 1 && unusedHeld[0] === card
            ? (strategicOptions.find((o) => o.id === "strategic")?.id ?? null)
            : null),
      }));
    } else {
      cards = strategicOptions
        .filter((o) => o.id.startsWith("strategic|"))
        .map((o) => ({ card: o.id.slice("strategic|".length), used: false, option: o.id }));
      if (cards.length === 0 && strategicOptions.length > 0) {
        cards = [{ card: "", used: false, option: strategicOptions[0].id }];
      }
    }
  }
  // Every strategic option must land on a button, or the bar would hide a legal action.
  const mapped = new Set(cards.map((c) => c.option).filter((id): id is string => id !== null));
  if (strategicOptions.some((o) => !mapped.has(o.id))) return null;
  const several = cards.length > 1;
  const strategic: StrategicButton[] = cards.map((entry) => {
    const info = strategyInfo(entry.card, entry.used);
    const offered = entry.option !== null && strategicOptions.some((o) => o.id === entry.option);
    const reason = offered
      ? null
      : entry.used
        ? "Already used this round"
        : closing
          ? "Already acted"
          : "Not available now";
    return {
      cardId: entry.card,
      label: several || !entry.card ? info.name || "Strategic action" : "Strategic action",
      cardName: info.name,
      effect: info.effect,
      optionId: offered ? entry.option : null,
      enabled: offered,
      reason,
      used: entry.used,
      info,
    };
  });
  const unusedCards = cards.filter((c) => !c.used).length;

  // Components and action cards
  const rest = options.filter(
    (o) => !isEnd(o) && !isPass(o) && !isTactical(o) && !isStrategic(o) && !isTrade(o),
  );
  const cardItems = rest.filter(isActionCard).map((o) => itemFor(o, details, player?.faction));
  const grouped = new Map<string, BarItem[]>();
  for (const option of rest.filter((o) => !isActionCard(o))) {
    const title = componentGroupTitle(option.id);
    grouped.set(title, [...(grouped.get(title) ?? []), itemFor(option, undefined, player?.faction)]);
  }
  const groups: BarGroup[] = GROUP_ORDER.filter((t) => grouped.has(t)).map((title) => ({
    title,
    items: grouped.get(title) ?? [],
  }));
  const componentCount = groups.reduce((n, g) => n + g.items.length, 0);

  // Trade
  const partners = readPartners(details, options);
  const availablePartners = partners.filter((p) => p.available).length;
  const reasons = [...new Set(partners.filter((p) => !p.available).map((p) => p.reason))];
  const tradeReason =
    availablePartners > 0
      ? null
      : partners.length === 0
        ? "No one to trade with"
        : reasons.length === 1 && reasons[0]
          ? reasons[0]
          : "No partner available";

  // Pass and End turn
  const passOption = passes[0] ?? null;
  const pass: BarAction = passOption
    ? {
        key: "pass",
        label: "Pass",
        hint: "End your round",
        optionId: passOption.id,
        enabled: true,
        reason: null,
        info: actionInfo(
          "Action",
          "Pass",
          "You take no more turns this round. Strategy cards you have not used stay unused.",
        ),
      }
    : disabledAction("pass", "Pass", passReason(unusedCards, closing));
  const endOption = ends[0] ?? null;
  const end: BarAction = endOption
    ? {
        key: "end",
        label: "End turn",
        hint: "Enter or click",
        optionId: endOption.id,
        enabled: true,
        reason: null,
        info: null,
      }
    : disabledAction("end", "End turn", "Act or pass first");

  return {
    mode: closing ? "closing" : "active",
    heading: closing ? "Choose: end your turn" : "Choose an action",
    waitingFor: null,
    tokens:
      tokens ??
      (player
        ? {
            tactic: player.tactic_tokens,
            fleet: player.fleet_tokens,
            strategy: player.strategic_tokens,
          }
        : null),
    tactical,
    strategic,
    components: {
      groups,
      count: componentCount,
      enabled: componentCount > 0,
      reason: componentCount > 0 ? null : "Nothing usable now",
    },
    actionCards: {
      items: cardItems,
      count: cardItems.length,
      enabled: cardItems.length > 0,
      reason:
        cardItems.length > 0
          ? null
          : player && player.action_cards_count > 0
            ? `${player.action_cards_count} in hand, none playable now`
            : "No playable action cards",
    },
    trade: {
      partners,
      enabled: availablePartners > 0,
      reason: tradeReason,
      available: availablePartners,
    },
    pass,
    end,
  };
}

/**
 * The bar when it is not the seat's turn: every button disabled with the reason, the seat's own
 * pools and strategy cards still visible (hover shows their text), and whose turn it is.
 */
export function deriveReadOnlyTurnBar(
  player: PlayerView,
  activeSeat: string | null,
): TurnBarModel {
  const passed = player.passed;
  const why = passed ? "You have passed" : NOT_YOUR_TURN;
  const spent = new Set(player.exhausted_strategy_cards);
  const several = player.strategy_cards.length > 1;
  const strategic: StrategicButton[] = player.strategy_cards.map((card) => {
    const used = spent.has(card);
    const info = strategyInfo(card, used);
    return {
      cardId: card,
      label: several ? info.name : "Strategic action",
      cardName: info.name,
      effect: info.effect,
      optionId: null,
      enabled: false,
      reason: used ? "Already used this round" : why,
      used,
      info,
    };
  });
  const off = (key: BarAction["key"], label: string) => disabledAction(key, label, why);
  return {
    mode: "readonly",
    heading: passed ? "You have passed" : "Waiting for the other players",
    waitingFor: passed ? null : activeSeat,
    tokens: {
      tactic: player.tactic_tokens,
      fleet: player.fleet_tokens,
      strategy: player.strategic_tokens,
    },
    tactical: off("tactical", "Tactical action"),
    strategic,
    components: { groups: [], count: 0, enabled: false, reason: why },
    actionCards: {
      items: [],
      count: 0,
      enabled: false,
      reason: why,
    },
    trade: { partners: [], enabled: false, reason: why, available: 0 },
    pass: off("pass", "Pass"),
    end: off("end", "End turn"),
  };
}
