import type { ActiveEffectDto } from "../../../src/protocol/types";
import { ME, RIVAL } from "./cards";
import { type EffectFacts, wordedEffect } from "./words";

export interface StripScene {
  key: string;
  phase: string;
  title: string;
  caption: string;
  effects: ActiveEffectDto[];
  /** Open the details list before the shot. */
  expand?: boolean;
}

/** Shapes as `ti4_host::view::effects::project_active_effects` serializes them. */
/** A scene as written here: the facts of its effects, which `wordedEffect` gives their words. */
type StripFacts = Omit<StripScene, "effects"> & { effects: EffectFacts[] };

const FACTS: StripFacts[] = [
  {
    key: "combat-round",
    phase: "action",
    title: "A combat round with three effects",
    caption: "Your Morale Boost (+1 to all combat rolls), the rival's Fighter Prototype (+2 for fighters) and the rival's pool of hit cancellations from Shields Holding or Maneuvering Jets, each with its seat badge. They end with this combat round.",
    effects: [
      { seat: ME, effect: "combat_roll_bonus", until: "combat_round", card: "mb1", amount: 1, system: "27" },
      { seat: RIVAL, effect: "fighter_bonus", until: "combat_round", card: "f_prototype", amount: 2, system: "27" },
      { seat: RIVAL, effect: "cancel_hits", until: "combat_round", amount: 2 },
    ],
  },
  {
    key: "combat-round-open",
    phase: "action",
    title: "The same, opened",
    caption: "Tapping the line lists each effect with its summary and when it ends (the chips collapse into the list on a phone).",
    expand: true,
    effects: [
      { seat: ME, effect: "combat_roll_bonus", until: "combat_round", card: "mb1", amount: 1, system: "27" },
      { seat: RIVAL, effect: "fighter_bonus", until: "combat_round", card: "f_prototype", amount: 2, system: "27" },
      { seat: RIVAL, effect: "cancel_hits", until: "combat_round", amount: 2 },
    ],
  },
  {
    key: "tactical-action",
    phase: "action",
    title: "Your tactical action",
    caption: "Flank Speed, Nav Suite, Solar Flare and Lost Star Chart, all until the end of this tactical action.",
    expand: true,
    effects: [
      { seat: ME, effect: "move_bonus", until: "tactical_action", card: "fs1", amount: 1, system: "27" },
      { seat: ME, effect: "ignore_anomalies", until: "tactical_action", card: "nav_suite" },
      { seat: ME, effect: "no_space_cannon", until: "tactical_action", card: "solar_flare", system: "27" },
      { seat: ME, effect: "wormholes_adjacent", until: "tactical_action", card: "lost_star" },
    ],
  },
  {
    key: "rival-turn",
    phase: "action",
    title: "A rival's turn: what they are running with",
    caption: "While the rival moves, everybody sees their Flank Speed and two stacked War Machines (+8 production, -1 cost each).",
    effects: [
      { seat: RIVAL, effect: "move_bonus", until: "tactical_action", card: "fs1", amount: 1, system: "27" },
      { seat: RIVAL, effect: "production_boost", until: "tactical_action", card: "war_machine1", amount: 8, copies: 2 },
    ],
  },
  {
    key: "invasion",
    phase: "action",
    title: "An invasion",
    caption: "Bunker (-4 to bombardment), Blitz (Bombardment 6) and Disable (opposing PDS silenced) until the end of this invasion.",
    expand: true,
    effects: [
      { seat: RIVAL, effect: "bombardment_penalty", until: "invasion", card: "bunker", amount: -4, system: "27" },
      { seat: ME, effect: "grant_bombardment", until: "invasion", card: "blitz", amount: 6, system: "27" },
      { seat: ME, effect: "disable_pds", until: "invasion", card: "disable", system: "27" },
    ],
  },
  {
    key: "agenda-phase",
    phase: "agenda",
    title: "The agenda phase",
    caption: "Hack Election (votes last) and Distinguished Councilor (+5 votes) until the agenda is resolved.",
    effects: [
      { seat: ME, effect: "vote_last", until: "agenda", card: "hack" },
      { seat: RIVAL, effect: "extra_votes", until: "agenda", card: "distinguished", amount: 5 },
    ],
  },
  {
    key: "duress-and-stability",
    phase: "action",
    title: "Marks that outlast a turn",
    caption: "Extreme Duress marks you until your next action (the holder is named); Political Stability keeps strategy cards until the next strategy phase has passed.",
    expand: true,
    effects: [
      { seat: ME, effect: "duress", until: "next_action", card: "extremeduress", other: RIVAL },
      { seat: RIVAL, effect: "keep_strategy_cards", until: "next_strategy_phase", card: "stability" },
    ],
  },
  {
    key: "black-market",
    phase: "action",
    title: "A negotiation under Black Market Dealings",
    caption: "While the transaction is open the strip says relic fragments, action cards and secret objectives may be traded.",
    expand: true,
    effects: [{ seat: ME, effect: "black_market", until: "negotiation", card: "blackmarketdealing" }],
  },
];

export const STRIP_SCENES: StripScene[] = FACTS.map((scene) => ({ ...scene, effects: scene.effects.map(wordedEffect) }));
