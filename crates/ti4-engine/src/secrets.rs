//! Secret objectives (LRR 45, 61.18).
//!
//! Ported from the oracle's `engine/secrets.py`: `scored_count`, `held_count`, `draw`,
//! `enforce_hand_limit`, `scoreable` and `award`, plus a first tranche of requirements.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, SecretObjectiveId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};

/// 45.4: three in hand, counting scored ones.
pub const HAND_LIMIT: usize = 3;

/// The choice kind for returning a secret over the hand limit.
pub const RETURN_KIND: &str = "return";

/// When a secret may be scored.
///
/// The oracle is explicit that this matters: an action or agenda secret offered at status time
/// changes both its timing and whether the fact that satisfied it still exists by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    Status,
    Action,
    Agenda,
}

/// When this secret may be scored, from its printed phase.
#[must_use]
pub fn timing(content: &ContentStore, alias: &SecretObjectiveId) -> Timing {
    let printed = content
        .get(ContentType::SecretObjectives, alias.as_str())
        .and_then(|record| record.text("phase"))
        .unwrap_or("status")
        .to_ascii_lowercase();
    match printed.as_str() {
        "action" => Timing::Action,
        "agenda" => Timing::Agenda,
        _ => Timing::Status,
    }
}

/// How many of this player's scored objectives were secrets (45.4 counts them).
#[must_use]
pub fn scored_count(state: &GameState, content: &ContentStore, player: &PlayerId) -> usize {
    state
        .scored_by(player)
        .iter()
        .filter(|alias| {
            content
                .get(ContentType::SecretObjectives, alias.as_str())
                .is_some()
        })
        .count()
}

/// Secrets in hand plus secrets already scored (45.4).
#[must_use]
pub fn held_count(state: &GameState, content: &ContentStore, player: &PlayerId) -> usize {
    state
        .player(player)
        .map_or(0, |seat| seat.secret_objectives.len())
        + scored_count(state, content, player)
}

/// Draw the top secret, then enforce the hand limit.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn draw(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<SecretObjectiveId>, IllegalChoice> {
    if state.secret_deck.is_empty() {
        return Ok(None);
    }
    let top = state.secret_deck.remove(0);
    if let Some(seat) = state.player_mut(player) {
        seat.secret_objectives.push(top.clone());
    }
    enforce_hand_limit(state, content, table, player)?;
    Ok(Some(top))
}

/// 45.4: over three, return an *unscored* one to the deck.
///
/// Scored secrets count towards the limit but cannot be given back, so a player who has scored
/// two may only hold one more. A player whose every secret is scored has nothing to return and
/// simply stays over — which is the rules working, not a stuck state.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn enforce_hand_limit(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    while held_count(state, content, player) > HAND_LIMIT {
        let held: Vec<SecretObjectiveId> = state
            .player(player)
            .map(|seat| seat.secret_objectives.clone())
            .unwrap_or_default();
        let Some(first) = held.first().cloned() else {
            return Ok(()); // every one is scored, so there is nothing to hand back
        };

        let chosen = if held.len() == 1 {
            first
        } else {
            let options: Vec<ChoiceOption> = held
                .iter()
                .map(|alias| {
                    ChoiceOption::labelled(
                        alias.to_string(),
                        RETURN_KIND,
                        format!("return {alias}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "return a secret objective to the deck",
                options,
            );
            SecretObjectiveId::new(table.ask(&choice)?.id)
        };

        if let Some(seat) = state.player_mut(player) {
            seat.secret_objectives.retain(|alias| alias != &chosen);
        }
        state.secret_deck.push(chosen);
    }
    Ok(())
}

/// A registered requirement check.
type Requirement = fn(&Position<'_>) -> bool;

/// What a secret's requirement is evaluated against.
pub struct Position<'a> {
    pub state: &'a GameState,
    pub content: &'a ContentStore,
    pub sources: SourceSet,
    pub player: &'a PlayerId,
}

impl Position<'_> {
    /// Units of one base type this player has on the board, in space and on planets.
    fn units_on_board(&self, base_type: &str) -> usize {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        let matches = |unit: &ti4_model::units::Unit| {
            &unit.owner == self.player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == base_type)
        };
        self.state
            .board
            .values()
            .map(|board| {
                board.units.iter().filter(|u| matches(u)).count()
                    + board
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|u| matches(u))
                        .count()
            })
            .sum()
    }
    /// Systems where this player has a ship.
    fn systems_with_ships(&self) -> usize {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        self.state
            .board
            .values()
            .filter(|board| {
                board.units_of(self.player).into_iter().any(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
                })
            })
            .count()
    }

    /// Controlled planets of one trait.
    fn planets_of_trait(&self, trait_name: &str) -> usize {
        let catalogue = ti4_content::galaxy::all_planets(self.content, self.sources);
        self.state
            .controlled_planets(self.player)
            .into_iter()
            .filter(|(_, planet)| {
                catalogue.get(planet.as_str()).is_some_and(|record| {
                    record
                        .planet_type()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case(trait_name))
                })
            })
            .count()
    }

    /// Combined resources or influence of controlled planets.
    fn combined(&self, kind: crate::production::Spend) -> i64 {
        self.state
            .controlled_planets(self.player)
            .into_iter()
            .map(|(_, planet)| {
                crate::production::planet_value(self.content, self.sources, planet, kind)
            })
            .sum()
    }

    /// Technologies owned of one colour.
    fn technologies_of_colour(&self, colour: &str) -> usize {
        self.state.player(self.player).map_or(0, |seat| {
            seat.technologies
                .iter()
                .filter(|alias| crate::technology::colour_type(self.content, alias) == Some(colour))
                .count()
        })
    }
}

/// Have `count` units of a base type on the board.
fn units(count: usize, base_type: &'static str) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.units_on_board(base_type) >= count
}

/// Have ships in `count` systems.
fn ships_in_systems(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.systems_with_ships() >= count
}

/// Control `count` planets of one trait.
fn planets_of_trait(count: usize, trait_name: &'static str) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.planets_of_trait(trait_name) >= count
}

/// Control planets with a combined value of at least `total`.
fn combined_value(total: i64, kind: crate::production::Spend) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.combined(kind) >= total
}

/// Own `count` technologies of the same colour.
///
/// The same colour, not four technologies: a spread across four tracks is the opposite of what
/// this card asks for.
fn same_colour_technologies(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        crate::technology::COLOURS
            .iter()
            .any(|colour| position.technologies_of_colour(colour) >= count)
    }
}

/// The registered requirements.
///
/// Two tranches, and unregistered secrets are unscoreable — the same design the objective
/// registry documents, and for the same reason: a coverage gap must show as a card nobody can
/// take, never as a bot winning on a rule that was never written.
#[must_use]
pub fn requirement_for(alias: &SecretObjectiveId) -> Option<Requirement> {
    fn four_pds(p: &Position<'_>) -> bool {
        units(4, "pds")(p)
    }
    // Three, not four: the printed card says "Have 3 space docks on the game board", and
    // taking the count from memory rather than the corpus is how an objective ends up
    // unscoreable in practice while looking implemented.
    fn three_docks(p: &Position<'_>) -> bool {
        units(3, "spacedock")(p)
    }
    fn five_dreadnoughts(p: &Position<'_>) -> bool {
        units(5, "dreadnought")(p)
    }
    fn ships_in_six(p: &Position<'_>) -> bool {
        ships_in_systems(6)(p)
    }
    fn four_cultural(p: &Position<'_>) -> bool {
        planets_of_trait(4, "cultural")(p)
    }
    fn four_hazardous(p: &Position<'_>) -> bool {
        planets_of_trait(4, "hazardous")(p)
    }
    fn four_industrial(p: &Position<'_>) -> bool {
        planets_of_trait(4, "industrial")(p)
    }
    fn twelve_influence(p: &Position<'_>) -> bool {
        combined_value(12, crate::production::Spend::Influence)(p)
    }
    fn twelve_resources(p: &Position<'_>) -> bool {
        combined_value(12, crate::production::Spend::Resources)(p)
    }
    fn four_of_a_colour(p: &Position<'_>) -> bool {
        same_colour_technologies(4)(p)
    }

    match alias.as_str() {
        "eap" => Some(four_pds),
        "fwm" => Some(three_docks),
        "gamf" => Some(five_dreadnoughts),
        "ctr" => Some(ships_in_six),
        "faa" => Some(four_cultural),
        "mrm" => Some(four_hazardous),
        "mp" => Some(four_industrial),
        "eh" => Some(twelve_influence),
        "hrm" => Some(twelve_resources),
        "mlp" => Some(four_of_a_colour),
        _ => None,
    }
}

/// Aliases with a registered requirement.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "ctr", "eap", "eh", "faa", "fwm", "gamf", "hrm", "mlp", "mp", "mrm",
    ]
}

/// Status-phase secrets this player may score now.
///
/// Action and agenda secrets are deliberately excluded: they are offered at the event that
/// satisfies them, and treating them as status objectives changes both their timing and whether
/// the triggering fact still exists by the time status arrives.
#[must_use]
pub fn scoreable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SecretObjectiveId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let already = state.scored_by(player);
    let position = Position {
        state,
        content,
        sources,
        player,
    };
    seat.secret_objectives
        .iter()
        .filter(|alias| timing(content, alias) == Timing::Status)
        .filter(|alias| !already.contains(&ti4_model::id::ObjectiveId::new(alias.as_str())))
        .filter(|alias| requirement_for(alias).is_some_and(|check| check(&position)))
        .cloned()
        .collect()
}

/// 61.18: reveal it, take the points, and it leaves the hand.
pub fn award(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &SecretObjectiveId,
) -> Option<i32> {
    let held = state
        .player(player)
        .is_some_and(|seat| seat.secret_objectives.contains(alias));
    if !held {
        return None;
    }
    let points = content
        .get(ContentType::SecretObjectives, alias.as_str())
        .and_then(|record| record.int("points"))
        .and_then(|points| i32::try_from(points).ok())
        .unwrap_or(1);

    state.record_score(player, ti4_model::id::ObjectiveId::new(alias.as_str()));
    let seat = state.player_mut(player)?;
    seat.secret_objectives.retain(|held| held != alias);
    seat.victory_points = (seat.victory_points + points).min(crate::objectives::VICTORY_TARGET);
    Some(points)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::{a_placed_planet, game, put};

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn hand(state: &GameState) -> Vec<SecretObjectiveId> {
        state.player(&player()).unwrap().secret_objectives.clone()
    }

    #[test]
    fn a_hand_over_three_returns_one_to_the_deck() {
        // 45.4.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        state.secret_deck = (0..5)
            .map(|n| SecretObjectiveId::new(format!("s{n}")))
            .collect();
        let mut table = Table::new();

        for _ in 0..4 {
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();
        }

        assert_eq!(hand(&state).len(), HAND_LIMIT, "the fourth was handed back");
    }

    #[test]
    fn a_scored_secret_counts_towards_the_limit_but_cannot_be_returned() {
        // A player who has scored two may only hold one more.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("eap"));
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("fwm"));

        assert_eq!(
            scored_count(&state, ContentStore::embedded(), &player()),
            2,
            "both are real secrets in the corpus"
        );

        state.secret_deck = (0..3)
            .map(|n| SecretObjectiveId::new(format!("s{n}")))
            .collect();
        let mut table = Table::new();
        for _ in 0..3 {
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();
        }

        assert_eq!(
            hand(&state).len(),
            1,
            "two scored plus one held is the limit"
        );
    }

    #[test]
    fn a_player_whose_every_secret_is_scored_has_nothing_to_return() {
        // The rules working, not a stuck state.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        for alias in ["eap", "fwm", "ctr", "dtd"] {
            state.record_score(&player(), ti4_model::id::ObjectiveId::new(alias));
        }
        let mut table = Table::new();

        enforce_hand_limit(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();

        assert!(hand(&state).is_empty());
    }

    #[test]
    fn an_empty_deck_draws_nothing() {
        let mut state = game(&["a"]);
        state.secret_deck.clear();
        let mut table = Table::new();
        assert_eq!(
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap(),
            None
        );
    }

    #[test]
    fn only_status_secrets_are_offered_at_status_time() {
        // Action and agenda secrets are offered at the event that satisfies them; treating
        // them as status objectives changes their timing and whether the fact still holds.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("eap")];
        // Satisfy it: four PDS on the board.
        let (system, planet) = a_placed_planet();
        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    player(),
                ));
        }

        let open = scoreable(&state, ContentStore::embedded(), POK, &player());
        for alias in &open {
            assert_eq!(timing(ContentStore::embedded(), alias), Timing::Status);
        }
    }

    #[test]
    fn an_unregistered_secret_is_never_scoreable() {
        // The same design the objective registry documents: a coverage gap shows as a card
        // nobody can take, never as a bot winning on a rule nobody wrote.
        let mut state = game(&["a"]);
        let unregistered = SecretObjectiveId::new("dtd");
        assert!(requirement_for(&unregistered).is_none());
        state.player_mut(&player()).unwrap().secret_objectives = vec![unregistered];

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());
    }

    #[test]
    fn four_pds_satisfies_its_secret() {
        let mut state = game(&["a"]);
        let eap = SecretObjectiveId::new("eap");
        state.player_mut(&player()).unwrap().secret_objectives = vec![eap.clone()];
        let (system, planet) = a_placed_planet();

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());

        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    player(),
                ));
        }

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![eap]
        );
    }

    #[test]
    fn another_players_units_do_not_count() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("eap")];
        let (system, planet) = a_placed_planet();
        for _ in 0..6 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    PlayerId::new("b"),
                ));
        }

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());
    }

    #[test]
    fn four_technologies_of_one_colour_is_not_four_technologies() {
        // The card asks for four of the *same* colour; a spread across four tracks is the
        // opposite of what it wants.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("mlp")];

        let one_each: Vec<String> = crate::technology::COLOURS
            .iter()
            .filter_map(|colour| {
                ContentStore::embedded()
                    .records(ContentType::Technologies)
                    .iter()
                    .find(|record| record.strings("types").contains(colour))
                    .and_then(|record| record.text("alias"))
                    .map(ToOwned::to_owned)
            })
            .collect();
        for alias in &one_each {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty(),
            "one of each colour is not four of one"
        );

        let four_biotic: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"BIOTIC"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(4)
            .collect();
        if four_biotic.len() < 4 {
            return;
        }
        for alias in &four_biotic {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![SecretObjectiveId::new("mlp")]
        );
    }

    #[test]
    fn a_combined_value_secret_adds_up_controlled_planets() {
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("hrm")];

        let mut total = 0;
        for (id, record) in &ti4_content::galaxy::all_planets(ContentStore::embedded(), POK) {
            if record.is_placed_during_play() || record.resources() == 0 {
                continue;
            }
            let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
            state
                .system_mut(&system)
                .set_control(ti4_model::id::PlanetId::new(*id), player());
            total += record.resources();
            if total >= 12 {
                break;
            }
        }

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![SecretObjectiveId::new("hrm")],
            "twelve combined resources scores it"
        );
    }

    #[test]
    fn scoring_takes_it_out_of_the_hand_and_pays_points() {
        // 61.18.
        let mut state = game(&["a"]);
        let eap = SecretObjectiveId::new("eap");
        state.player_mut(&player()).unwrap().secret_objectives = vec![eap.clone()];

        let points = award(&mut state, ContentStore::embedded(), &player(), &eap).unwrap();

        assert!(points > 0);
        assert!(hand(&state).is_empty(), "it left the hand");
        assert_eq!(state.player(&player()).unwrap().victory_points, points);
        assert!(
            state
                .scored_by(&player())
                .contains(&ti4_model::id::ObjectiveId::new("eap"))
        );
    }

    #[test]
    fn a_secret_not_in_hand_cannot_be_scored() {
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        assert_eq!(
            award(
                &mut state,
                ContentStore::embedded(),
                &player(),
                &SecretObjectiveId::new("eap")
            ),
            None
        );
    }

    #[test]
    fn every_registered_alias_is_a_real_secret() {
        for alias in registered_aliases() {
            assert!(
                ContentStore::embedded()
                    .get(ContentType::SecretObjectives, alias)
                    .is_some(),
                "{alias} is not a secret the corpus knows"
            );
            assert!(requirement_for(&SecretObjectiveId::new(alias)).is_some());
        }
        let _ = put;
    }
}
