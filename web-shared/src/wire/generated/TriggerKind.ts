// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The family of event a reaction decision answers, in the words a client switches on.
 *
 * `Other` plus [`DecisionTrigger::event_type`] is the open-ended fallback, so a new window never
 * breaks a client.
 */
export type TriggerKind = "action_card_played" | "action_card_discarded" | "system_activated" | "ship_moved" | "strategic_action_began" | "strategy_card_chosen" | "strategy_phase_began" | "turn_began" | "turn_passed" | "player_passed" | "action_completed" | "strategy_cards_would_return" | "agenda_phase_began" | "agenda_revealed" | "votes_cast" | "agenda_resolved" | "transaction" | "planet_control_gained" | "invasion_began" | "units_committed" | "ground_rolls" | "combat_started" | "anti_fighter_barrage" | "space_cannon_hits" | "hits_to_assign" | "sustain_damage" | "ship_destroyed" | "retreat" | "space_combat_won" | "production_used" | "unit_ability_rolled" | "other";
