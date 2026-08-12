//! Strategy card abilities (LRR 52, 91, 92).
//!
//! Ported from the oracle's `engine/strategy.py`. A first tranche: Leadership and Technology,
//! which are the two whose effects the engine already has the machinery for — token gain,
//! payment, and research.
//!
//! A card with no registered ability resolves structurally (the token is spent, the card
//! exhausts) and announces its effect unresolved, as everywhere else here.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, TechnologyId};
use ti4_model::state::{GameState, TokenPool};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};
use crate::production::Spend;

/// 52.2: Leadership's primary gains three command tokens.
pub const LEADERSHIP_TOKENS: u32 = 3;
/// 52.3: three influence buys one more token.
pub const INFLUENCE_PER_TOKEN: i64 = 3;
/// 91.3: Technology's secondary costs four resources.
pub const TECHNOLOGY_SECONDARY_COST: i64 = 4;

/// The choice kind for researching a technology.
pub const RESEARCH_KIND: &str = "research";

/// What a card's ability did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ability {
    /// The ability ran.
    Resolved,
    /// No ability is registered for this card.
    Unresolved,
}

/// Cards whose abilities this engine resolves, by the name printed on the card.
#[must_use]
pub fn registered_cards() -> Vec<&'static str> {
    vec!["Leadership", "Technology"]
}

/// The name printed on a strategy card.
#[must_use]
pub fn card_name(content: &ContentStore, card: &str) -> Option<String> {
    content
        .get(ti4_model::content_types::ContentType::StrategyCards, card)
        .and_then(|record| record.text("name"))
        .map(ToOwned::to_owned)
}

/// 52.3: spend influence, three at a time, for one command token each.
///
/// Offered repeatedly rather than as a count, because each token also picks a pool — the same
/// reason [`crate::tokens::TokenGain`] asks once per token.
fn buy_tokens_with_influence(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
) -> Result<usize, IllegalChoice> {
    let mut bought = 0;
    loop {
        if !crate::payment::affordable(
            state,
            content,
            sources,
            player,
            INFLUENCE_PER_TOKEN,
            Spend::Influence,
        ) {
            return Ok(bought);
        }
        let choice = Choice::new(
            player.clone(),
            format!("spend {INFLUENCE_PER_TOKEN} influence for a command token"),
            vec![
                ChoiceOption::labelled("buy", "spend", "spend for a token"),
                ChoiceOption::decline(),
            ],
        );
        if table.ask(&choice)?.is_decline() {
            return Ok(bought);
        }
        let Some(plan) = crate::payment::plans(
            state,
            content,
            sources,
            player,
            INFLUENCE_PER_TOKEN,
            Spend::Influence,
        )
        .into_iter()
        .next() else {
            return Ok(bought);
        };
        if !crate::payment::apply(state, player, &plan) {
            return Ok(bought);
        }
        if let Some(seat) = state.player_mut(player) {
            seat.gain_token(TokenPool::Strategic, 1);
        }
        bought += 1;
    }
}

/// Offer one research, if anything is researchable.
fn offer_research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<TechnologyId>, IllegalChoice> {
    let open = crate::technology::researchable(state, content, sources, player);
    if open.is_empty() {
        return Ok(None);
    }
    let mut options: Vec<ChoiceOption> = open
        .iter()
        .map(|alias| ChoiceOption::labelled(alias.to_string(), RESEARCH_KIND, alias.to_string()))
        .collect();
    options.push(ChoiceOption::decline());

    let choice = Choice::new(player.clone(), "research a technology", options);
    let answer = table.ask(&choice)?;
    if answer.is_decline() {
        return Ok(None);
    }
    let alias = TechnologyId::new(answer.id);
    if crate::technology::research(state, content, sources, player, &alias) {
        Ok(Some(alias))
    } else {
        Ok(None)
    }
}

/// Resolve a card's primary ability.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn primary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    card: &str,
) -> Result<Ability, IllegalChoice> {
    let Some(name) = card_name(content, card) else {
        return Ok(Ability::Unresolved);
    };
    match name.as_str() {
        "Leadership" => {
            // 52.2 gains three, then 52.3 lets influence buy more. The gain goes to the
            // strategy pool here; TokenGain owns the per-token pool choice and is not reachable
            // from this call shape yet, which is recorded rather than silently decided.
            if let Some(seat) = state.player_mut(player) {
                seat.gain_token(
                    TokenPool::Strategic,
                    i32::try_from(LEADERSHIP_TOKENS).unwrap_or(0),
                );
            }
            buy_tokens_with_influence(state, content, sources, table, player)?;
        }
        "Technology" => {
            // 91.2: one technology free, then a second for six resources. The second is not
            // implemented; the first is, and the difference is announced by the caller.
            offer_research(state, content, sources, table, player)?;
        }
        _ => return Ok(Ability::Unresolved),
    }
    Ok(Ability::Resolved)
}

/// Resolve a card's secondary ability for one follower.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn secondary(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    card: &str,
) -> Result<Ability, IllegalChoice> {
    let Some(name) = card_name(content, card) else {
        return Ok(Ability::Unresolved);
    };
    match name.as_str() {
        // 52.3: the secondary is the influence purchase alone, with no free tokens.
        "Leadership" => {
            buy_tokens_with_influence(state, content, sources, table, player)?;
        }
        "Technology" => {
            // 91.3: four resources buys one technology. The strategy token was already spent
            // to follow, so only the resources are charged here.
            if !crate::payment::affordable(
                state,
                content,
                sources,
                player,
                TECHNOLOGY_SECONDARY_COST,
                Spend::Resources,
            ) {
                return Ok(Ability::Resolved);
            }
            if offer_research(state, content, sources, table, player)?.is_some()
                && let Some(plan) = crate::payment::plans(
                    state,
                    content,
                    sources,
                    player,
                    TECHNOLOGY_SECONDARY_COST,
                    Spend::Resources,
                )
                .into_iter()
                .next()
            {
                crate::payment::apply(state, player, &plan);
            }
        }
        _ => return Ok(Ability::Unresolved),
    }
    Ok(Ability::Resolved)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::{a_placed_planet, game};

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    /// The corpus id of a strategy card by printed name.
    fn card(name: &str) -> String {
        ContentStore::embedded()
            .records(ti4_model::content_types::ContentType::StrategyCards)
            .iter()
            .find(|record| record.text("name") == Some(name))
            .and_then(|record| record.text("alias").or_else(|| record.text("id")))
            .map_or_else(
                || panic!("{name} is not a strategy card"),
                ToOwned::to_owned,
            )
    }

    fn give_influence(state: &mut GameState) {
        let catalogue = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
        for (id, record) in &catalogue {
            if record.influence() == 0 || record.is_placed_during_play() {
                continue;
            }
            let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
            state
                .system_mut(&system)
                .set_control(ti4_model::id::PlanetId::new(*id), player());
            if crate::production::available(
                &*state,
                ContentStore::embedded(),
                POK,
                &player(),
                Spend::Influence,
            ) >= 9
            {
                break;
            }
        }
    }

    #[test]
    fn leadership_gains_three_tokens() {
        // 52.2.
        let mut state = game(&["a"]);
        let before = state.player(&player()).unwrap().total_tokens();
        let mut table = Table::with_default(Box::new(crate::choice::AlwaysDecline));

        let done = primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Leadership"),
        )
        .unwrap();

        assert_eq!(done, Ability::Resolved);
        assert_eq!(
            state.player(&player()).unwrap().total_tokens(),
            before + i32::try_from(LEADERSHIP_TOKENS).unwrap()
        );
    }

    #[test]
    fn leadership_buys_more_tokens_with_influence() {
        // 52.3, and the secondary is that purchase alone with no free tokens.
        let mut state = game(&["a"]);
        give_influence(&mut state);
        let before = state.player(&player()).unwrap().total_tokens();
        let mut table = Table::new(); // FirstOption always buys

        secondary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Leadership"),
        )
        .unwrap();

        let after = state.player(&player()).unwrap().total_tokens();
        assert!(after > before, "influence bought at least one token");
        assert!(
            !state.exhausted_planets.is_empty(),
            "and it was paid for by exhausting planets"
        );
    }

    #[test]
    fn a_player_with_no_influence_buys_nothing() {
        let mut state = game(&["a"]);
        let before = state.player(&player()).unwrap().total_tokens();
        let mut table = Table::new();

        secondary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Leadership"),
        )
        .unwrap();

        assert_eq!(state.player(&player()).unwrap().total_tokens(), before);
    }

    #[test]
    fn technology_researches_one() {
        // 91.2's first technology.
        let mut state = game(&["a"]);
        let before = state.player(&player()).unwrap().technologies.len();
        let mut table = Table::new();

        primary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Technology"),
        )
        .unwrap();

        assert_eq!(
            state.player(&player()).unwrap().technologies.len(),
            before + 1
        );
    }

    #[test]
    fn the_technology_secondary_charges_four_resources() {
        // 91.3. The strategy token was already spent to follow, so only resources are charged.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().trade_goods = 8;
        let before = state.player(&player()).unwrap().trade_goods;
        let mut table = Table::new();

        secondary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Technology"),
        )
        .unwrap();

        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            before - i32::try_from(TECHNOLOGY_SECONDARY_COST).unwrap()
        );
        assert_eq!(state.player(&player()).unwrap().technologies.len(), 1);
    }

    #[test]
    fn the_technology_secondary_is_free_if_it_cannot_be_afforded() {
        let mut state = game(&["a"]);
        let before = state.player(&player()).unwrap().technologies.len();
        let mut table = Table::new();

        secondary(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player(),
            &card("Technology"),
        )
        .unwrap();

        assert_eq!(
            state.player(&player()).unwrap().technologies.len(),
            before,
            "nothing was researched on credit"
        );
    }

    #[test]
    fn an_unregistered_card_reports_unresolved() {
        let mut state = game(&["a"]);
        let mut table = Table::new();
        assert_eq!(
            primary(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &player(),
                &card("Warfare")
            )
            .unwrap(),
            Ability::Unresolved
        );
    }

    #[test]
    fn every_registered_card_is_a_real_one() {
        for name in registered_cards() {
            let id = card(name);
            assert_eq!(
                card_name(ContentStore::embedded(), &id).as_deref(),
                Some(name)
            );
        }
        let _ = a_placed_planet();
    }
}
