import type { EffectPreviewDto } from "../../../src/protocol/types";
import { actor } from "../_shared/players";
import { opponent } from "../_shared/players";

export const ME = actor;
export const RIVAL = opponent.id;

export type EffectGroup =
  | "Cancel a card or hits"
  | "Hit or destroy ships"
  | "Combat and movement bonuses"
  | "Retreats and rolls"
  | "Invasion"
  | "Command tokens, commodities and production"
  | "Agenda and votes"
  | "Strategy, turns and negotiation";

export interface CardScene {
  /** File-name key. */
  key: string;
  group: EffectGroup;
  /** Content alias the offer carries. */
  alias: string;
  name: string;
  event: string;
  relation: "when" | "after";
  /** Fields of the DecisionTrigger as the engine serializes it (empty fields left out). */
  trigger: Record<string, unknown>;
  /** The engine's EffectPreview for the card (without option/card, which the scene fills in). */
  preview: Omit<EffectPreviewDto, "option" | "card">;
  /** A one-line note for the artifact on what the preview adds. */
  note: string;
}

const unit = (owner: string, unit_type: string, count = 1) => ({ owner, unit_type, count });

const t = (kind: string, event: string, relation: "when" | "after", rest: Record<string, unknown> = {}) => ({
  kind,
  event_type: event,
  event_id: 21,
  relation,
  ...rest,
});

export const SCENES: CardScene[] = [
  // Cancel a card or hits
  {
    key: "sabotage", group: "Cancel a card or hits", alias: "sabo1", name: "Sabotage", event: "ACTION_CARD_PLAYED", relation: "when",
    trigger: t("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: RIVAL, card: "uprising" }),
    preview: { effect: "cancel_card", seats: [RIVAL], refs: ["uprising"] },
    note: "Names whose card it cancels and that the card stays spent.",
  },
  {
    key: "coup", group: "Cancel a card or hits", alias: "coup", name: "Coup d'Etat", event: "STRATEGIC_ACTION_BEGAN", relation: "when",
    trigger: t("strategic_action_began", "STRATEGIC_ACTION_BEGAN", "when", { actor: RIVAL, card: "warfare" }),
    preview: { effect: "cancel_strategic_action", seats: [RIVAL], refs: ["warfare"] },
    note: "Names the strategy card whose action it would cancel.",
  },
  {
    key: "shields-holding", group: "Cancel a card or hits", alias: "sh1", name: "Shields Holding", event: "HITS_TO_ASSIGN", relation: "when",
    trigger: t("hits_to_assign", "HITS_TO_ASSIGN", "when", { subject: ME, system: "27", hits: 5 }),
    preview: { effect: "cancel_hits", amount: 2, of: 5, system: "27" },
    note: "Says how many of the incoming hits it cancels and how many remain.",
  },
  {
    key: "maneuvering-jets", group: "Cancel a card or hits", alias: "mjets1", name: "Maneuvering Jets", event: "SPACE_CANNON_HITS", relation: "when",
    trigger: t("space_cannon_hits", "SPACE_CANNON_HITS", "when", { actor: RIVAL, subject: ME, system: "27", hits: 3 }),
    preview: { effect: "cancel_hits", amount: 1, of: 3, system: "27", seats: [RIVAL] },
    note: "Says which gunner's space cannon hits it cancels.",
  },
  // Hit or destroy ships
  {
    key: "direct-hit", group: "Hit or destroy ships", alias: "dh1", name: "Direct Hit", event: "SUSTAIN_DAMAGE_USED", relation: "after",
    trigger: t("sustain_damage", "SUSTAIN_DAMAGE_USED", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "destroy_unit", seats: [RIVAL], system: "27", units: [unit(RIVAL, "dreadnought")] },
    note: "Names the exact ship that would be destroyed.",
  },
  {
    key: "direct-hit-immune", group: "Hit or destroy ships", alias: "dh1", name: "Direct Hit", event: "SUSTAIN_DAMAGE_USED", relation: "after",
    trigger: t("sustain_damage", "SUSTAIN_DAMAGE_USED", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "destroy_unit", seats: [RIVAL], system: "27", units: [unit(RIVAL, "dreadnought_2")], fizzles: "immune" },
    note: "Warns when the sustained ship cannot be destroyed by Direct Hit.",
  },
  {
    key: "reflective-shielding", group: "Hit or destroy ships", alias: "reflective", name: "Reflective Shielding", event: "SUSTAIN_DAMAGE_USED", relation: "when",
    trigger: t("sustain_damage", "SUSTAIN_DAMAGE_USED", "when", { actor: ME, system: "27" }),
    preview: { effect: "produce_hits", seats: [RIVAL], system: "27", amount: 2 },
    note: "Names the opponent who takes the 2 hits and where.",
  },
  {
    key: "emergency-repairs", group: "Hit or destroy ships", alias: "emergency", name: "Emergency Repairs", event: "COMBAT_ROUND_STARTED", relation: "after",
    trigger: t("combat_started", "COMBAT_ROUND_STARTED", "after", { actor: RIVAL, subject: ME, system: "27" }),
    preview: { effect: "repair_units", system: "27", units: [unit(ME, "dreadnought"), unit(ME, "cruiser")] },
    note: "Lists the damaged units that would be repaired.",
  },
  {
    key: "emergency-repairs-none", group: "Hit or destroy ships", alias: "emergency", name: "Emergency Repairs", event: "COMBAT_ROUND_STARTED", relation: "after",
    trigger: t("combat_started", "COMBAT_ROUND_STARTED", "after", { actor: RIVAL, subject: ME, system: "27" }),
    preview: { effect: "repair_units", system: "27", fizzles: "nothing_there" },
    note: "Says so when nothing is damaged.",
  },
  // Combat and movement bonuses
  {
    key: "morale-boost", group: "Combat and movement bonuses", alias: "mb1", name: "Morale Boost", event: "COMBAT_ROUND_STARTED", relation: "after",
    trigger: t("combat_started", "COMBAT_ROUND_STARTED", "after", { actor: ME, subject: RIVAL, system: "27" }),
    preview: { effect: "combat_roll_bonus", amount: 1, system: "27" },
    note: "States the bonus and that it ends with this combat round; the active-effects strip then shows it.",
  },
  {
    key: "fighter-prototype", group: "Combat and movement bonuses", alias: "f_prototype", name: "Fighter Prototype", event: "SPACE_COMBAT_STARTED", relation: "after",
    trigger: t("combat_started", "SPACE_COMBAT_STARTED", "after", { actor: ME, subject: RIVAL, system: "27" }),
    preview: { effect: "fighter_bonus", amount: 2, system: "27", units: [unit(ME, "fighter", 6)] },
    note: "Counts the fighters that get the +2.",
  },
  {
    key: "flank-speed", group: "Combat and movement bonuses", alias: "fs1", name: "Flank Speed", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "move_bonus", amount: 1, system: "27" },
    note: "States the bonus and its duration; the strip shows it during the tactical action.",
  },
  {
    key: "lost-star-chart", group: "Combat and movement bonuses", alias: "lost_star", name: "Lost Star Chart", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "wormholes_adjacent" },
    note: "Explains the adjacency change and its duration.",
  },
  {
    key: "nav-suite", group: "Combat and movement bonuses", alias: "nav_suite", name: "Nav Suite", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "ignore_anomalies" },
    note: "Explains what is ignored and for how long.",
  },
  {
    key: "solar-flare", group: "Combat and movement bonuses", alias: "solar_flare", name: "Solar Flare", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "no_space_cannon", system: "27" },
    note: "Says space cannon is off for this movement step.",
  },
  // Retreats and rolls
  {
    key: "rout", group: "Retreats and rolls", alias: "rout", name: "Rout", event: "RETREAT_STEP_STARTED", relation: "after",
    trigger: t("retreat", "RETREAT_STEP_STARTED", "after", { actor: ME, system: "27" }),
    preview: { effect: "force_retreat", seats: [RIVAL], system: "27" },
    note: "Names who must announce a retreat.",
  },
  {
    key: "intercept", group: "Retreats and rolls", alias: "intercept", name: "Intercept", event: "RETREAT_DECLARED", relation: "after",
    trigger: t("retreat", "RETREAT_DECLARED", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "bar_retreat", seats: [RIVAL], system: "27" },
    note: "Names who cannot retreat this round.",
  },
  {
    key: "waylay", group: "Retreats and rolls", alias: "waylay", name: "Waylay", event: "ANTI_FIGHTER_BARRAGE_STARTED", relation: "when",
    trigger: t("anti_fighter_barrage", "ANTI_FIGHTER_BARRAGE_STARTED", "when", { actor: ME, system: "27" }),
    preview: { effect: "barrage_hits_all_ships", system: "27" },
    note: "Says the barrage hits are not limited to fighters.",
  },
  {
    key: "scramble-frequency", group: "Retreats and rolls", alias: "scramble", name: "Scramble Frequency", event: "UNIT_ABILITY_ROLLED", relation: "after",
    trigger: t("unit_ability_rolled", "UNIT_ABILITY_ROLLED", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "reroll_all_dice", seats: [RIVAL], system: "27" },
    note: "Names who must reroll everything.",
  },
  {
    key: "salvage", group: "Retreats and rolls", alias: "salvage", name: "Salvage", event: "SPACE_COMBAT_WON", relation: "after",
    trigger: t("space_combat_won", "SPACE_COMBAT_WON", "after", { actor: ME, system: "27" }),
    preview: { effect: "take_commodities", seats: [RIVAL], amount: 3, system: "27" },
    note: "Already had a client-side preview; the engine now states the opponents and the total.",
  },
  // Invasion
  {
    key: "bunker", group: "Invasion", alias: "bunker", name: "Bunker", event: "INVASION_BEGAN", relation: "after",
    trigger: t("invasion_began", "INVASION_BEGAN", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "bombardment_penalty", amount: -4, system: "27", planets: ["lodor"] },
    note: "Lists the planets the -4 protects.",
  },
  {
    key: "blitz", group: "Invasion", alias: "blitz", name: "Blitz", event: "INVASION_BEGAN", relation: "after",
    trigger: t("invasion_began", "INVASION_BEGAN", "after", { actor: ME, system: "27" }),
    preview: { effect: "grant_bombardment", amount: 6, system: "27", units: [unit(ME, "dreadnought", 2), unit(ME, "cruiser")] },
    note: "Lists the ships that gain Bombardment 6.",
  },
  {
    key: "disable", group: "Invasion", alias: "disable", name: "Disable", event: "INVASION_BEGAN", relation: "after",
    trigger: t("invasion_began", "INVASION_BEGAN", "after", { actor: ME, system: "27" }),
    preview: { effect: "disable_pds", seats: [RIVAL], system: "27", units: [unit(RIVAL, "pds", 2)] },
    note: "Lists the opposing PDS that lose their guns.",
  },
  {
    key: "parley", group: "Invasion", alias: "parley", name: "Parley", event: "UNITS_COMMITTED", relation: "after",
    trigger: t("units_committed", "UNITS_COMMITTED", "after", { actor: RIVAL, system: "27", planet: "lodor" }),
    preview: { effect: "return_committed_unit", seats: [RIVAL], system: "27", planets: ["lodor"], units: [unit(RIVAL, "infantry")] },
    note: "Names the committed unit that would go back to space.",
  },
  {
    key: "infiltrate", group: "Invasion", alias: "infiltrate", name: "Infiltrate", event: "PLANET_CONTROL_GAINED", relation: "when",
    trigger: t("planet_control_gained", "PLANET_CONTROL_GAINED", "when", { actor: ME, system: "27", planet: "lodor" }),
    preview: { effect: "replace_structures", system: "27", planets: ["lodor"], units: [unit(ME, "pds"), unit(ME, "spacedock")] },
    note: "Lists the structures it would replace.",
  },
  // Command tokens, commodities and production
  {
    key: "counterstroke", group: "Command tokens, commodities and production", alias: "counterstroke", name: "Counterstroke", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: RIVAL, system: "27" }),
    preview: { effect: "return_command_token", amount: 1, system: "27" },
    note: "Names the system whose token comes back to the tactic pool.",
  },
  {
    key: "rally", group: "Command tokens, commodities and production", alias: "rally", name: "Rally", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "gain_fleet_tokens", amount: 2, seats: [RIVAL], system: "27" },
    note: "States the tokens and whose ships justify them.",
  },
  {
    key: "harness-energy", group: "Command tokens, commodities and production", alias: "harness", name: "Harness Energy", event: "SYSTEM_ACTIVATED", relation: "after",
    trigger: t("system_activated", "SYSTEM_ACTIVATED", "after", { actor: ME, system: "27" }),
    preview: { effect: "replenish_commodities", amount: 3, of: 4 },
    note: "Gives the exact commodities gained.",
  },
  {
    key: "war-machine", group: "Command tokens, commodities and production", alias: "war_machine1", name: "War Machine", event: "PRODUCTION_USED", relation: "after",
    trigger: t("production_used", "PRODUCTION_USED", "after", { actor: ME, system: "27" }),
    preview: { effect: "production_boost", amount: 4 },
    note: "States +4 production and -1 cost.",
  },
  // Agenda and votes
  {
    key: "deadly-plot", group: "Agenda and votes", alias: "deadly_plot", name: "Deadly Plot", event: "AGENDA_RESOLVED", relation: "when",
    trigger: t("agenda_resolved", "AGENDA_RESOLVED", "when", { agenda: "minister_of_peace", subject: RIVAL }),
    preview: { effect: "discard_agenda", amount: 4, refs: ["minister_of_peace"] },
    note: "Names the agenda and how many planets get exhausted.",
  },
  {
    key: "confounding-legal-text", group: "Agenda and votes", alias: "confounding", name: "Confounding Legal Text", event: "AGENDA_RESOLVED", relation: "when",
    trigger: t("agenda_resolved", "AGENDA_RESOLVED", "when", { agenda: "minister_of_peace", subject: RIVAL }),
    preview: { effect: "become_elected", seats: [RIVAL], refs: ["minister_of_peace"] },
    note: "Names who you replace as the elected player.",
  },
  {
    key: "hack-election", group: "Agenda and votes", alias: "hack", name: "Hack Election", event: "AGENDA_REVEALED", relation: "after",
    trigger: t("agenda_revealed", "AGENDA_REVEALED", "after", { agenda: "minister_of_peace" }),
    preview: { effect: "vote_last", refs: ["minister_of_peace"] },
    note: "States the new vote order for this agenda; the strip keeps it visible.",
  },
  {
    key: "veto", group: "Agenda and votes", alias: "veto", name: "Veto", event: "AGENDA_REVEALED", relation: "when",
    trigger: t("agenda_revealed", "AGENDA_REVEALED", "when", { agenda: "minister_of_peace" }),
    preview: { effect: "replace_agenda", refs: ["minister_of_peace"] },
    note: "Says the revealed agenda is replaced by the next one.",
  },
  {
    key: "distinguished-councilor", group: "Agenda and votes", alias: "distinguished", name: "Distinguished Councilor", event: "VOTES_CAST", relation: "after",
    trigger: t("votes_cast", "VOTES_CAST", "after", { actor: ME }),
    preview: { effect: "extra_votes", amount: 5 },
    note: "States the +5 votes; the strip shows them while the agenda is open.",
  },
  // Strategy, turns and negotiation
  {
    key: "political-stability", group: "Strategy, turns and negotiation", alias: "stability", name: "Political Stability", event: "STRATEGY_CARDS_WOULD_RETURN", relation: "when",
    trigger: t("strategy_cards_would_return", "STRATEGY_CARDS_WOULD_RETURN", "when"),
    preview: { effect: "keep_strategy_cards", refs: ["politics", "trade"] },
    note: "Names the strategy cards you would keep.",
  },
  {
    key: "reverse-engineer", group: "Strategy, turns and negotiation", alias: "reverse_engineer", name: "Reverse Engineer", event: "ACTION_CARD_DISCARDED", relation: "after",
    trigger: t("action_card_discarded", "ACTION_CARD_DISCARDED", "after", { actor: RIVAL, card: "unexpected" }),
    preview: { effect: "take_discarded_card", seats: [RIVAL], refs: ["unexpected"] },
    note: "Names the discarded card you would take.",
  },
  {
    key: "master-plan", group: "Strategy, turns and negotiation", alias: "master_plan", name: "Master Plan", event: "ACTION_COMPLETED", relation: "after",
    trigger: t("action_completed", "ACTION_COMPLETED", "after", { actor: ME }),
    preview: { effect: "extra_action" },
    note: "Says an extra action follows.",
  },
  {
    key: "puppets-on-a-string", group: "Strategy, turns and negotiation", alias: "puppetsonastring", name: "Puppets on a String", event: "PLAYER_PASSED", relation: "after",
    trigger: t("player_passed", "PLAYER_PASSED", "after", { actor: ME }),
    preview: { effect: "puppet_action" },
    note: "Says you act once more although you have passed.",
  },
  {
    key: "extreme-duress", group: "Strategy, turns and negotiation", alias: "extremeduress", name: "Extreme Duress", event: "TURN_BEGAN", relation: "after",
    trigger: t("turn_began", "TURN_BEGAN", "after", { actor: RIVAL }),
    preview: { effect: "duress", seats: [RIVAL] },
    note: "Names the marked player and the penalty; the strip then shows the mark to everyone.",
  },
  {
    key: "crisis", group: "Strategy, turns and negotiation", alias: "crisis", name: "Crisis", event: "TURN_PASSED", relation: "after",
    trigger: t("turn_passed", "TURN_PASSED", "after", { actor: RIVAL }),
    preview: { effect: "skip_next_turn" },
    note: "Says the next turn is skipped.",
  },
  {
    key: "black-market-dealings", group: "Strategy, turns and negotiation", alias: "blackmarketdealing", name: "Black Market Dealings", event: "TRANSACTION_OPENED", relation: "when",
    trigger: t("transaction", "TRANSACTION_OPENED", "when", { actor: ME, subject: RIVAL }),
    preview: { effect: "black_market", seats: [RIVAL] },
    note: "Says what the transaction may now include; the strip shows it while the negotiation is open.",
  },
];
