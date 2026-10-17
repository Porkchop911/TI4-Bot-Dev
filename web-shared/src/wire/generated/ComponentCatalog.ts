// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { AbilityCard } from "./AbilityCard.ts";
import type { ActionCard } from "./ActionCard.ts";
import type { AgendaCard } from "./AgendaCard.ts";
import type { FactionSheet } from "./FactionSheet.ts";
import type { LeaderCard } from "./LeaderCard.ts";
import type { PrintedCard } from "./PrintedCard.ts";
import type { PublicObjectiveCard } from "./PublicObjectiveCard.ts";
import type { SecretObjective } from "./SecretObjective.ts";
import type { StrategyCard } from "./StrategyCard.ts";

/**
 * The printed cards of the play areas. It does not change during a game.
 */
export type ComponentCatalog = { abilities: Array<AbilityCard>, 
/**
 * The abilities of each faction, in the order of the faction sheet.
 */
factions: { [key in string]: Array<string> }, 
/**
 * The name and the commodity value of each faction.
 */
faction_sheets: { [key in string]: FactionSheet }, 
/**
 * The strategy cards of every set, by initiative: a game names its cards by id, and the id
 * of a card of an older printing has no name in it (`base4`). They are not only those of
 * the sources: a game can play a set of another source than its other cards (`te`).
 */
strategy_cards: Array<StrategyCard>, leaders: Array<LeaderCard>, technologies: Array<PrintedCard>, relics: Array<PrintedCard>, 
/**
 * The ability cards of the legendary planets, by the id of the planet.
 */
legendary: Array<PrintedCard>, 
/**
 * The action card deck as a game begins with it, sorted by id.
 */
action_cards: Array<ActionCard>, 
/**
 * The groups of the timings of the action cards, in the order in which a reader is shown them.
 */
action_card_groups: Array<string>, 
/**
 * The secret objective deck as a game begins with it, sorted by id.
 */
secret_objectives: Array<SecretObjective>, 
/**
 * The groups of what the secret objectives ask for, in the order in which a reader is shown
 * them.
 */
secret_objective_groups: Array<string>, 
/**
 * Every public objective that a game can reveal: stage I, then stage II, each sorted by id.
 * A game draws five of each stage, and no fact says which.
 */
public_objectives: Array<PublicObjectiveCard>, 
/**
 * The groups of what the public objectives ask for, in the order in which a reader is shown
 * them.
 */
public_objective_groups: Array<string>, 
/**
 * The agenda deck as a game begins with it, sorted by id.
 */
agendas: Array<AgendaCard>, 
/**
 * The groups of what the agendas affect, in the order in which a reader is shown them.
 */
agenda_groups: Array<string>, };
