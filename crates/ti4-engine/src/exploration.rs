//! Exploration and relic fragments (LRR 35, and 35.9 for fragments).
//!
//! Ported from the oracle's `engine/exploration.py`: `trait_of`, `draw`, `explore`, `_resolve`,
//! `_gain_fragment` and `_attach`, plus the fragment half of `engine/relics.py`.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, PlayerId, RelicId};
use ti4_model::state::GameState;

use crate::deck::EXPLORATION_TRAITS;

/// The frontier deck, which needs no planet (35.5).
pub const FRONTIER: &str = "FRONTIER";

/// How many fragments of one trait buy a relic (35.9).
pub const FRAGMENTS_PER_RELIC: i32 = 3;

/// What resolving one exploration card did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Explored {
    /// A relic fragment, kept faceup until purged (35.9).
    Fragment { trait_name: String },
    /// An attachment on the planet.
    Attached { card: String },
    /// Drawn, but this engine has no handler for it. Announced, never silently dropped.
    Unresolved { card: String },
    /// An attachment drawn from the frontier, which has no planet to attach to.
    Discarded { card: String },
}

/// The deck a planet explores into, or `None` if it cannot be explored (35.2b).
#[must_use]
pub fn trait_of(content: &ContentStore, sources: SourceSet, planet: &PlanetId) -> Option<String> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let record = catalogue.get(planet.as_str())?;
    let trait_name = record.planet_type()?.to_ascii_uppercase();
    EXPLORATION_TRAITS
        .iter()
        .find(|known| **known == trait_name && **known != FRONTIER)
        .map(|known| (*known).to_owned())
}

/// Draw the top card of one exploration deck.
pub fn draw(state: &mut GameState, deck: &str) -> Option<String> {
    let cards = state.exploration_decks.get_mut(deck)?;
    if cards.is_empty() {
        return None;
    }
    Some(cards.remove(0))
}

/// How a card resolves, from the corpus.
#[must_use]
pub fn resolution(content: &ContentStore, card: &str) -> Option<String> {
    content
        .get(ContentType::Explores, card)
        .and_then(|record| record.text("resolution"))
        .map(ToOwned::to_owned)
}

/// Explore a planet, resolving one card (35.2).
///
/// `planet` is `None` for a frontier draw, which is the point of the frontier deck: those cards
/// are resolved without a planet.
pub fn explore(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    deck: &str,
    planet: Option<&PlanetId>,
) -> Option<Explored> {
    let card = draw(state, deck)?;
    let kind = resolution(content, &card).unwrap_or_default();

    let outcome = match kind.as_str() {
        "Fragment" => {
            // 35.9: fragments stay faceup in the play area until purged for a relic. A
            // frontier fragment needs no planet, which is most of why the frontier deck is
            // worth drawing at all.
            let trait_name = content
                .get(ContentType::Explores, &card)
                .and_then(|record| record.text("type"))
                .unwrap_or(deck)
                .to_ascii_uppercase();
            gain_fragment(state, player, &trait_name);
            Explored::Fragment { trait_name }
        }
        "Attach" => {
            let Some(planet) = planet else {
                // Discarded rather than silently applied to nothing, and said out loud so a
                // count of unresolved cards stays honest.
                return Some(Explored::Discarded { card });
            };
            state
                .planet_attachments
                .entry(planet.clone())
                .or_default()
                .push(card.clone());
            Explored::Attached { card }
        }
        // Instant and token cards need per-card handlers, which this engine does not have.
        // Announced rather than dropped: an unresolved card must be visible as a gap.
        _ => Explored::Unresolved { card },
    };
    Some(outcome)
}

/// Add one relic fragment of a trait to a player's play area.
pub fn gain_fragment(state: &mut GameState, player: &PlayerId, trait_name: &str) {
    if let Some(seat) = state.player_mut(player) {
        *seat
            .relic_fragments
            .entry(trait_name.to_ascii_uppercase())
            .or_insert(0) += 1;
    }
}

/// Traits this player could purge three of for a relic (35.9).
///
/// Frontier fragments substitute for any trait, so they are counted towards every other one
/// rather than forming a pile of their own that can never be cashed.
#[must_use]
pub fn purgeable(state: &GameState, player: &PlayerId) -> Vec<String> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let frontier = seat.relic_fragments.get(FRONTIER).copied().unwrap_or(0);
    seat.relic_fragments
        .iter()
        .filter(|(trait_name, _)| trait_name.as_str() != FRONTIER)
        .filter(|(_, held)| **held + frontier >= FRAGMENTS_PER_RELIC)
        .map(|(trait_name, _)| trait_name.clone())
        .collect()
}

/// Purge three fragments of a trait and draw a relic (35.9).
///
/// Frontier fragments make up any shortfall, and are spent only after the matching ones — a
/// wildcard spent first would be a wildcard wasted.
pub fn purge_for_relic(
    state: &mut GameState,
    player: &PlayerId,
    trait_name: &str,
) -> Option<RelicId> {
    let trait_name = trait_name.to_ascii_uppercase();
    let seat = state.player(player)?;
    let matching = seat.relic_fragments.get(&trait_name).copied().unwrap_or(0);
    let frontier = seat.relic_fragments.get(FRONTIER).copied().unwrap_or(0);
    if matching + frontier < FRAGMENTS_PER_RELIC {
        return None;
    }
    let from_matching = matching.min(FRAGMENTS_PER_RELIC);
    let from_frontier = FRAGMENTS_PER_RELIC - from_matching;

    let relic = state.relic_deck.first().cloned()?;
    state.relic_deck.remove(0);

    let seat = state.player_mut(player)?;
    *seat.relic_fragments.entry(trait_name).or_insert(0) -= from_matching;
    if from_frontier > 0 {
        *seat.relic_fragments.entry(FRONTIER.to_owned()).or_insert(0) -= from_frontier;
    }
    seat.relics.push(relic.clone());
    Some(relic)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    #[test]
    fn a_planet_explores_into_its_own_trait_deck() {
        // 35.2b: a planet with no trait cannot be explored at all.
        let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        let mut traited = 0;
        let mut untraited = 0;
        for (id, record) in &catalogue {
            let found = trait_of(ContentStore::embedded(), POK, &PlanetId::new(*id));
            match record.planet_type() {
                Some(kind) if EXPLORATION_TRAITS.contains(&kind.to_ascii_uppercase().as_str()) => {
                    assert_eq!(found.as_deref(), Some(kind.to_ascii_uppercase().as_str()));
                    traited += 1;
                }
                _ => {
                    assert_eq!(found, None);
                    untraited += 1;
                }
            }
        }
        assert!(traited > 0 && untraited > 0, "the corpus has both");
    }

    #[test]
    fn drawing_takes_from_the_top_and_empties() {
        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["one".into(), "two".into()]);

        assert_eq!(draw(&mut state, "CULTURAL").as_deref(), Some("one"));
        assert_eq!(draw(&mut state, "CULTURAL").as_deref(), Some("two"));
        assert_eq!(draw(&mut state, "CULTURAL"), None);
    }

    #[test]
    fn an_unknown_card_is_announced_rather_than_dropped() {
        // An unresolved card must be visible as a gap, not silently discarded.
        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert("CULTURAL".to_owned(), vec!["not_a_card".into()]);

        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            "CULTURAL",
            None,
        );
        assert!(matches!(outcome, Some(Explored::Unresolved { .. })));
    }

    #[test]
    fn an_attachment_from_the_frontier_is_discarded_not_applied_to_nothing() {
        let attach = ContentStore::embedded()
            .records(ContentType::Explores)
            .iter()
            .find(|record| record.text("resolution") == Some("Attach"))
            .and_then(|record| record.text("id").or_else(|| record.text("alias")))
            .map(ToOwned::to_owned);
        let Some(attach) = attach else {
            return;
        };

        let mut state = game(&["a"]);
        state
            .exploration_decks
            .insert(FRONTIER.to_owned(), vec![attach]);

        let outcome = explore(
            &mut state,
            ContentStore::embedded(),
            &player(),
            FRONTIER,
            None,
        );
        assert!(matches!(outcome, Some(Explored::Discarded { .. })));
    }

    #[test]
    fn three_matching_fragments_buy_a_relic() {
        // 35.9.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }

        assert_eq!(purgeable(&state, &player()), vec!["CULTURAL".to_owned()]);
        let relic = purge_for_relic(&mut state, &player(), "CULTURAL");

        assert_eq!(relic, Some(RelicId::new("some_relic")));
        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.relic_fragments.get("CULTURAL"), Some(&0));
        assert_eq!(seat.relics.len(), 1);
        assert!(state.relic_deck.is_empty());
    }

    #[test]
    fn two_fragments_are_not_enough() {
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        for _ in 0..2 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }

        assert!(purgeable(&state, &player()).is_empty());
        assert_eq!(purge_for_relic(&mut state, &player(), "CULTURAL"), None);
        assert_eq!(state.relic_deck.len(), 1, "the deck was not touched");
    }

    #[test]
    fn a_frontier_fragment_substitutes_for_any_trait() {
        // 35.9. A wildcard that could not be cashed would be a pile that only grows.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("some_relic")];
        gain_fragment(&mut state, &player(), "HAZARDOUS");
        gain_fragment(&mut state, &player(), "HAZARDOUS");
        gain_fragment(&mut state, &player(), FRONTIER);

        assert_eq!(purgeable(&state, &player()), vec!["HAZARDOUS".to_owned()]);
        assert!(purge_for_relic(&mut state, &player(), "HAZARDOUS").is_some());

        let seat = state.player(&player()).unwrap();
        assert_eq!(seat.relic_fragments.get("HAZARDOUS"), Some(&0));
        assert_eq!(
            seat.relic_fragments.get(FRONTIER),
            Some(&0),
            "the wildcard made up the shortfall"
        );
    }

    #[test]
    fn matching_fragments_are_spent_before_wildcards() {
        // A frontier fragment spent while a matching one was available is a wildcard wasted.
        let mut state = game(&["a"]);
        state.relic_deck = vec![RelicId::new("r1")];
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "INDUSTRIAL");
        }
        gain_fragment(&mut state, &player(), FRONTIER);

        purge_for_relic(&mut state, &player(), "INDUSTRIAL").unwrap();

        let seat = state.player(&player()).unwrap();
        assert_eq!(
            seat.relic_fragments.get(FRONTIER),
            Some(&1),
            "the wildcard was kept"
        );
    }

    #[test]
    fn no_relic_deck_means_no_relic() {
        let mut state = game(&["a"]);
        state.relic_deck.clear();
        for _ in 0..3 {
            gain_fragment(&mut state, &player(), "CULTURAL");
        }
        assert_eq!(purge_for_relic(&mut state, &player(), "CULTURAL"), None);
        assert_eq!(
            state
                .player(&player())
                .unwrap()
                .relic_fragments
                .get("CULTURAL"),
            Some(&3),
            "nothing was spent"
        );
    }
}
