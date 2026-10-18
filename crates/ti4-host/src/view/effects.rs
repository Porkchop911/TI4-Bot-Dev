//! The lingering effects of played action cards that are in force now: who they help or bind,
//! what they do, and when they end.
//!
//! Nothing here decides a rule. Every entry is read from a marker the engine already keeps on a
//! seat or on the table, and the marker is compared to the counter the engine compares it with
//! (the combat round, the tactical action, the agenda), so an entry is shown exactly while the
//! engine would still honour it. A card is played face up, so every marker is public and the list
//! is the same for every viewer.
//!
//! The engine words each entry (`title`, `summary` and `expiry`), so a client keeps no rule for
//! each kind; the facts stay beside the words as data.
//!
//! The list is display only: it is built from the state on each projection, adds nothing to the
//! state, the decision records or any hash, and is left out of a view that has none (older
//! clients and saves never see the field).

use serde::{Deserialize, Serialize};
use ti4_engine::phrases::{Phrase, Subject};
use ti4_model::id::{ActionCardId, PlayerId, SystemId};
use ti4_model::state::{GameState, Phase, TransientFlags};

/// When a lingering effect ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EffectExpiry {
    /// At the end of the current round of space combat.
    CombatRound,
    /// At the end of the current tactical action.
    TacticalAction,
    /// At the end of the current invasion.
    Invasion,
    /// When the agenda being voted on is resolved.
    Agenda,
    /// When the player next takes an action.
    NextAction,
    /// When the negotiation that is open closes.
    Negotiation,
    /// When the next strategy phase has passed.
    NextStrategyPhase,
}

impl EffectExpiry {
    /// When it ends, in words: "until the end of this combat round".
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::CombatRound => "until the end of this combat round",
            Self::TacticalAction => "until the end of this tactical action",
            Self::Invasion => "until the end of this invasion",
            Self::Agenda => "until this agenda is resolved",
            Self::NextAction => "until their next action",
            Self::Negotiation => "until this negotiation closes",
            Self::NextStrategyPhase => "until the next strategy phase has passed",
        }
    }
}

/// One lingering effect of a played action card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActiveEffectView {
    /// The seat the effect belongs to: the player who played the card, or the player it binds
    /// when the card marks another seat (`bars_retreat`, `duress`).
    pub seat: PlayerId,
    /// A content alias of the card, when one card alone causes the effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    /// A stable snake_case kind; the same words as the previews of the reaction windows.
    pub effect: String,
    pub until: EffectExpiry,
    /// How many copies of the card stack on it.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub copies: u32,
    /// The size of the effect: a bonus, hits left to cancel, votes added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    /// The other seat involved: who holds the duress, the opponent of a rout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemId>,
    /// What causes it: the card (a subject), or the words of a pool of cards ("Shields Holding /
    /// Maneuvering Jets").
    #[serde(default)]
    pub title: Phrase,
    /// What it does, in one short line: "+1 to all combat rolls", "Marked by {0}: ...".
    #[serde(default)]
    pub summary: Phrase,
    /// `until` in words: "until the end of this combat round".
    #[serde(default)]
    pub expiry: String,
}

fn one() -> u32 {
    1
}

fn is_one(value: &u32) -> bool {
    *value == 1
}

fn entry(
    seat: &PlayerId,
    card: Option<&str>,
    effect: &str,
    until: EffectExpiry,
) -> ActiveEffectView {
    ActiveEffectView {
        seat: seat.clone(),
        card: card.map(str::to_owned),
        effect: effect.to_owned(),
        until,
        copies: 1,
        amount: None,
        other: None,
        system: None,
        title: Phrase::default(),
        summary: Phrase::default(),
        expiry: String::new(),
    }
}

/// The words of `effect`: what causes it, what it does and when it ends.
fn worded(effect: ActiveEffectView) -> ActiveEffectView {
    let title = match &effect.card {
        Some(card) => Phrase::default().name(Subject::action_card(&ActionCardId::new(card))),
        // Shields Holding and Maneuvering Jets feed one pool.
        None if effect.effect == "cancel_hits" => {
            Phrase::default().say("Shields Holding / Maneuvering Jets")
        }
        None => Phrase::default().say(&effect.effect),
    };
    let amount = effect.amount.unwrap_or(0);
    // The printed size when the facts have none.
    let or = |default: i64| effect.amount.filter(|value| *value != 0).unwrap_or(default);
    let copies = if effect.copies > 1 {
        format!(" ({} cards)", effect.copies)
    } else {
        String::new()
    };
    let say = |words: &str| Phrase::default().say(words);
    let other = |phrase: Phrase| match &effect.other {
        Some(other) => phrase.name(Subject::seat(other)),
        None => phrase.say("the opponent"),
    };
    let summary = match effect.effect.as_str() {
        "combat_roll_bonus" => say(&format!("+{} to all combat rolls", or(1))),
        "fighter_bonus" => say(&format!("+{} to fighter combat rolls{copies}", or(2))),
        "force_retreat" => other(Phrase::default()).say(" must announce a retreat, if able"),
        "barrage_hits_all_ships" => say("Anti-fighter barrage hits all ships"),
        "bar_retreat" => say("Cannot retreat"),
        "cancel_hits" => say(&format!(
            "Cancels the next {amount} {}",
            if amount == 1 { "hit" } else { "hits" }
        )),
        "move_bonus" => say(&format!("+{} move for all ships", or(1))),
        "ignore_anomalies" => say("Ignores anomalies while moving"),
        "no_space_cannon" => say(&format!(
            "Others cannot use Space Cannon against these ships{copies}"
        )),
        "wormholes_adjacent" => say("Alpha and beta wormholes are adjacent"),
        "production_boost" => say(&format!("+{} production, -1 cost{copies}", or(4))),
        "bombardment_penalty" => say(&format!(
            "{} to bombardment against their planets{copies}",
            or(-4)
        )),
        "grant_bombardment" => say(&format!("Non-fighter ships gain Bombardment {}", or(6))),
        "disable_pds" => say("Opposing PDS lose Planetary Shield and Space Cannon"),
        "vote_last" => say("Votes last on this agenda"),
        "extra_votes" => say(&format!("+{amount} votes on this outcome")),
        "keep_strategy_cards" => say("Keeps strategy cards and skips the next pick"),
        "duress" => other(say("Marked by "))
            .say(": a non-strategic action costs cards, trade goods and secrets"),
        "black_market" => say("Relic fragments, action cards and secrets can be traded"),
        _ => Phrase::default(),
    };
    ActiveEffectView {
        title,
        summary,
        expiry: effect.until.text().to_owned(),
        ..effect
    }
}

/// The lingering card effects in force in `state`, seat by seat in table order.
#[must_use]
pub fn project_active_effects(state: &GameState) -> Vec<ActiveEffectView> {
    use EffectExpiry::{
        Agenda, CombatRound, Invasion, Negotiation, NextAction, NextStrategyPhase, TacticalAction,
    };
    let mut out = Vec::new();
    let combat = state.active_space_combat.clone();
    let round = state.combat_round_seq;
    let activation = state.activation_seq;
    let in_action = state.active_system.is_some();
    let in_invasion = state.active_invasion.is_some();
    let agenda_now = state.phase == Phase::Agenda;
    for seat in &state.players {
        let id = &seat.id;
        let copies = |count: usize| u32::try_from(count).unwrap_or(u32::MAX);
        let mut push = |effect: ActiveEffectView| out.push(effect);
        if let Some((system, attacker, defender)) = &combat {
            let opponent = if attacker == id {
                Some(defender.clone())
            } else if defender == id {
                Some(attacker.clone())
            } else {
                None
            };
            if seat.combat_bonus_round == Some(round) {
                let mut e = entry(id, Some("mb1"), "combat_roll_bonus", CombatRound);
                e.amount = Some(1);
                e.system = Some(system.clone());
                push(e);
            }
            let prototypes = seat
                .fighter_bonus_round
                .iter()
                .filter(|played| **played == round)
                .count();
            if prototypes > 0 {
                let mut e = entry(id, Some("f_prototype"), "fighter_bonus", CombatRound);
                e.copies = copies(prototypes);
                e.amount = Some(2 * i64::from(e.copies));
                e.system = Some(system.clone());
                push(e);
            }
            if seat.rout_round == Some(round) {
                let mut e = entry(id, Some("rout"), "force_retreat", CombatRound);
                e.other = opponent;
                e.system = Some(system.clone());
                push(e);
            }
            if seat.waylay_barrage_round == Some(round) {
                let mut e = entry(id, Some("waylay"), "barrage_hits_all_ships", CombatRound);
                e.system = Some(system.clone());
                push(e);
            }
            if seat.retreat_barred_round == Some(round) {
                let mut e = entry(id, Some("intercept"), "bar_retreat", CombatRound);
                e.system = Some(system.clone());
                push(e);
            }
        }
        if in_action
            && let Some((held, left)) = seat.cancel_hits_round
            && held == round
            && left > 0
        {
            // Shields Holding and Maneuvering Jets feed one pool, so no single card is named.
            let mut e = entry(id, None, "cancel_hits", CombatRound);
            e.amount = i64::try_from(left).ok();
            push(e);
        }
        if in_action {
            if seat.move_bonus_activation == Some(activation) {
                let mut e = entry(id, Some("fs1"), "move_bonus", TacticalAction);
                e.amount = Some(1);
                e.system = state.active_system.clone();
                push(e);
            }
            if seat.anomalies_ignored_activation == Some(activation) {
                push(entry(
                    id,
                    Some("nav_suite"),
                    "ignore_anomalies",
                    TacticalAction,
                ));
            }
            if seat.solar_flare.contains(&activation) {
                let mut e = entry(id, Some("solar_flare"), "no_space_cannon", TacticalAction);
                e.copies = copies(
                    seat.solar_flare
                        .iter()
                        .filter(|a| **a == activation)
                        .count(),
                );
                e.system = state.active_system.clone();
                push(e);
            }
            if seat.lost_star.contains(&activation) {
                push(entry(
                    id,
                    Some("lost_star"),
                    "wormholes_adjacent",
                    TacticalAction,
                ));
            }
            let machines = seat
                .war_machine_use
                .iter()
                .filter(|a| **a == activation)
                .count();
            if machines > 0 {
                let mut e = entry(id, Some("war_machine1"), "production_boost", TacticalAction);
                e.copies = copies(machines);
                e.amount = Some(4 * i64::from(e.copies));
                push(e);
            }
        }
        if in_invasion {
            let count = |marks: &[u32]| marks.iter().filter(|a| **a == activation).count();
            let bunkers = count(&seat.bunker_invasion);
            if bunkers > 0 {
                let mut e = entry(id, Some("bunker"), "bombardment_penalty", Invasion);
                e.copies = copies(bunkers);
                e.amount = Some(-4 * i64::from(e.copies));
                e.system = state.active_system.clone();
                push(e);
            }
            if count(&seat.blitz_invasion) > 0 {
                let mut e = entry(id, Some("blitz"), "grant_bombardment", Invasion);
                e.amount = Some(6);
                e.system = state.active_system.clone();
                push(e);
            }
            if count(&seat.disable_invasion) > 0 {
                let mut e = entry(id, Some("disable"), "disable_pds", Invasion);
                e.system = state.active_system.clone();
                push(e);
            }
        }
        if agenda_now {
            if seat.hack_votes_last_agenda == Some(state.agenda_seq) {
                push(entry(id, Some("hack"), "vote_last", Agenda));
            }
            if let Some((held, votes)) = seat.extra_votes_agenda
                && held == state.agenda_seq
                && votes > 0
            {
                let mut e = entry(id, Some("distinguished"), "extra_votes", Agenda);
                e.amount = Some(votes);
                push(e);
            }
        }
        if seat.stability {
            push(entry(
                id,
                Some("stability"),
                "keep_strategy_cards",
                NextStrategyPhase,
            ));
        }
        if let Some(holder) = &seat.duress_by {
            let mut e = entry(id, Some("extremeduress"), "duress", NextAction);
            e.other = Some(holder.clone());
            push(e);
        }
    }
    if state.transient_flags.has(TransientFlags::BLACK_MARKET)
        && let Some(active) = &state.active
    {
        out.push(entry(
            active,
            Some("blackmarketdealing"),
            "black_market",
            Negotiation,
        ));
    }
    out.into_iter().map(worded).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::project_game_view;
    use crate::protocol::status::ViewerRole;
    use ti4_model::id::{ActionCardId, SystemId};

    fn game() -> GameState {
        ti4_engine::fixtures::seated_game(
            &[("a", "mahact"), ("b", "xxcha")],
            ti4_model::content_types::DEFAULT,
        )
    }

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn a_fresh_game_has_no_lingering_effects_and_the_view_omits_the_field() {
        let state = game();
        assert!(project_active_effects(&state).is_empty());
        let json = serde_json::to_value(project_game_view(&state, &ViewerRole::Spectator)).unwrap();
        assert!(json.get("active_effects").is_none());
    }

    #[test]
    fn a_round_effect_shows_during_its_combat_round_and_not_after_it() {
        let mut state = game();
        state.combat_round_seq = 4;
        state.player_mut(&me()).unwrap().combat_bonus_round = Some(4);
        // The marker outlives the fight in the state; it must not outlive it on screen.
        assert!(project_active_effects(&state).is_empty());
        state.active_space_combat = Some((SystemId::new("27"), me(), PlayerId::new("b")));
        let shown = project_active_effects(&state);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].effect, "combat_roll_bonus");
        assert_eq!(shown[0].until, EffectExpiry::CombatRound);
        assert_eq!(shown[0].amount, Some(1));
        state.combat_round_seq = 5;
        assert!(project_active_effects(&state).is_empty());
    }

    #[test]
    fn tactical_action_effects_stack_by_copy_and_end_with_the_action() {
        let mut state = game();
        state.activation_seq = 3;
        state.active_system = Some(SystemId::new("27"));
        let seat = state.player_mut(&me()).unwrap();
        seat.move_bonus_activation = Some(3);
        seat.war_machine_use = vec![3, 3, 2];
        seat.lost_star = vec![3];
        let shown = project_active_effects(&state);
        let kinds: Vec<_> = shown.iter().map(|e| e.effect.as_str()).collect();
        assert_eq!(
            kinds,
            ["move_bonus", "wormholes_adjacent", "production_boost"]
        );
        let machines = shown
            .iter()
            .find(|e| e.effect == "production_boost")
            .unwrap();
        assert_eq!(machines.copies, 2);
        assert_eq!(machines.amount, Some(8));
        state.active_system = None;
        assert!(project_active_effects(&state).is_empty());
    }

    #[test]
    fn duress_is_listed_against_the_marked_seat_with_the_holder_named() {
        let mut state = game();
        state.player_mut(&PlayerId::new("b")).unwrap().duress_by = Some(me());
        let shown = project_active_effects(&state);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].seat, PlayerId::new("b"));
        assert_eq!(shown[0].other, Some(me()));
        assert_eq!(
            shown[0].summary.text,
            "Marked by {0}: a non-strategic action costs cards, trade goods and secrets"
        );
        assert_eq!(shown[0].summary.refs["0"], Subject::seat(&me()));
        assert_eq!(shown[0].expiry, "until their next action");
    }

    #[test]
    fn an_effect_names_its_card_says_what_it_does_and_when_it_ends() {
        let mut state = game();
        state.combat_round_seq = 4;
        state.active_space_combat = Some((SystemId::new("27"), me(), PlayerId::new("b")));
        state.player_mut(&me()).unwrap().combat_bonus_round = Some(4);
        let shown = project_active_effects(&state);
        assert_eq!(shown[0].title.text, "{0}");
        assert_eq!(
            shown[0].title.refs["0"],
            Subject::action_card(&ActionCardId::new("mb1"))
        );
        assert_eq!(shown[0].summary.text, "+1 to all combat rolls");
        assert!(shown[0].summary.refs.is_empty());
        assert_eq!(shown[0].expiry, "until the end of this combat round");
        let json = serde_json::to_value(&shown[0]).unwrap();
        assert_eq!(json["title"]["refs"]["0"]["kind"], "action_card");
        assert_eq!(json["expiry"], "until the end of this combat round");
    }

    #[test]
    fn stacked_copies_are_counted_in_the_summary() {
        let mut state = game();
        state.activation_seq = 3;
        state.active_system = Some(SystemId::new("27"));
        state.player_mut(&me()).unwrap().war_machine_use = vec![3, 3];
        let shown = project_active_effects(&state);
        assert_eq!(shown[0].summary.text, "+8 production, -1 cost (2 cards)");
        assert_eq!(shown[0].expiry, "until the end of this tactical action");
    }

    #[test]
    fn the_pool_of_hit_cancellations_has_a_title_of_words() {
        let mut state = game();
        state.combat_round_seq = 2;
        state.active_system = Some(SystemId::new("27"));
        state.player_mut(&me()).unwrap().cancel_hits_round = Some((2, 1));
        let shown = project_active_effects(&state);
        assert_eq!(shown[0].title.text, "Shields Holding / Maneuvering Jets");
        assert!(shown[0].title.refs.is_empty());
        assert_eq!(shown[0].summary.text, "Cancels the next 1 hit");
    }

    #[test]
    fn every_viewer_sees_the_same_effects_and_no_hand_is_named() {
        let mut state = game();
        state.player_mut(&me()).unwrap().stability = true;
        state.player_mut(&me()).unwrap().action_cards = vec![ActionCardId::new("sabo1")];
        let json = |viewer: &ViewerRole| {
            serde_json::to_value(project_game_view(&state, viewer)).unwrap()["active_effects"]
                .clone()
        };
        let spectator = json(&ViewerRole::Spectator);
        assert_eq!(spectator, json(&ViewerRole::Player(me())));
        assert_eq!(spectator, json(&ViewerRole::Player(PlayerId::new("b"))));
        assert!(!spectator.to_string().contains("sabo"));
    }

    #[test]
    fn the_black_market_belongs_to_the_active_seat_for_the_negotiation() {
        let mut state = game();
        state.active = Some(me());
        state.transient_flags.set(TransientFlags::BLACK_MARKET);
        let shown = project_active_effects(&state);
        assert_eq!(shown[0].effect, "black_market");
        assert_eq!(shown[0].until, EffectExpiry::Negotiation);
    }
}
