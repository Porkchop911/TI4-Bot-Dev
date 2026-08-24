//! The faction plugin contract (M07-001).
//!
//! Ported from the oracle's `engine/faction_abilities/__init__.py`.
//!
//! A faction ability is a *query* the engine makes, not a reaction it waits for: how much to
//! shift a combat die, what the fleet limit is, whether this player may trade an action card.
//! None of the seven faction modules in the oracle imports the timing system, which is why this
//! layer needs no reactions to be useful.
//!
//! Registries are keyed by ability id and looked up through the player's faction, so an ability
//! belongs to whoever has the card rather than to a name checked at each call site. That matters
//! for coverage: [`unimplemented`] can then say which printed abilities nothing here answers,
//! and [`BLOCKED`] says which of those cannot be written yet and why — kept apart from the merely
//! unwritten so the numbers stay honest.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// Abilities that cannot be written until a subsystem exists, with the subsystem named.
///
/// Separate from merely unwritten abilities on purpose: one is work, the other is a dependency,
/// and a single number covering both hides which.
#[must_use]
pub fn blocked() -> BTreeMap<&'static str, &'static str> {
    [
        (
            "propagation",
            "Nekro cannot research, and technology theft is unmodelled",
        ),
        (
            "mitosis",
            "unit placement outside production is not a step yet",
        ),
        (
            "stall_tactics",
            "action-card discard as a free action has no window",
        ),
        (
            "telepathic",
            "the agenda deck is not inspectable before it is revealed",
        ),
        (
            "quash",
            "an agenda cannot be discarded and replaced mid-window",
        ),
        (
            "your_ships_have_no_shields",
            "no window exists between rolling and assigning hits",
        ),
    ]
    .into_iter()
    .collect()
}

/// Ability ids this player has, through their faction.
#[must_use]
pub fn of_player(state: &GameState, content: &ContentStore, player: &PlayerId) -> Vec<String> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    ti4_content::factions::get(content, seat.faction.as_str())
        .map(|faction| {
            faction
                .abilities()
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether this player has a particular ability.
#[must_use]
pub fn has(state: &GameState, content: &ContentStore, player: &PlayerId, ability: &str) -> bool {
    of_player(state, content, player)
        .iter()
        .any(|id| id == ability)
}

// -- the hooks -----------------------------------------------------------------------------------
//
// Each is a query with a default, so a subsystem calls it unconditionally and a faction with
// nothing to say changes nothing. Adding a faction means adding an arm here, never a branch at
// the call site.

/// Shift applied to each of this player's combat dice, in `context` ("space" or "ground").
#[must_use]
pub fn combat_modifier(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    #[expect(
        unused_variables,
        reason = "space and ground differ for abilities not yet ported; the parameter is the                   contract, and dropping it would have every caller pass nothing and every                   future ability shift both"
    )]
    context: &str,
) -> i64 {
    of_player(state, content, player)
        .iter()
        .map(|ability| match ability.as_str() {
            // Jol-Nar's Fragile: -1 to every combat roll, in space and on the ground.
            "fragile" => -1,
            // Sardakk's Unrelenting: +1 to every combat roll.
            "unrelenting" => 1,
            // The Titans' Coalescence and Sol's Orbital Drop do not shift dice.
            _ => 0,
        })
        .sum()
}

/// The limit on non-fighter ships in one system, adjusted by anything this player has.
#[must_use]
pub fn fleet_supply(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    base: i32,
) -> i32 {
    of_player(state, content, player)
        .iter()
        .fold(base, |limit, ability| match ability.as_str() {
            // Letnev's Armada: two more non-fighter ships than the fleet pool allows.
            "armada" => limit + 2,
            _ => limit,
        })
}

/// Command tokens gained in the status phase, adjusted.
#[must_use]
pub fn status_tokens(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    base: i32,
) -> i32 {
    of_player(state, content, player)
        .iter()
        .fold(base, |count, ability| match ability.as_str() {
            // Sol's Versatile: one more token every status phase.
            "versatile" => count + 1,
            _ => count,
        })
}

/// Prerequisites this player may skip when researching `technology`.
///
/// Analytical's window is explicit that it does not open for unit upgrades. 90.7b: an upgrade
/// has no colour and satisfies no prerequisite of its own, but it still *carries* them — Carrier
/// II needs two blue — so a lookup that waived a slot for any technology would let Jol-Nar
/// research upgrades this ability was never meant to touch.
#[must_use]
pub fn waived_prerequisites(
    state: &GameState,
    content: &ContentStore,
    #[expect(
        unused_variables,
        reason = "kept in the contract: an ability scoped to a source set is a question this                   will have to answer, and adding it later would touch every call site"
    )]
    sources: SourceSet,
    player: &PlayerId,
    technology: &str,
) -> usize {
    let is_upgrade = content
        .get(ContentType::Technologies, technology)
        // The corpus names it `baseUpgrade`: the unit whose card this replaces. A guess at
        // `unitUpgrade` matched nothing, which made the ability waive for upgrades too and the
        // test that was meant to catch it vacuous.
        .is_some_and(|record| {
            record
                .text("baseUpgrade")
                .is_some_and(|base| !base.is_empty())
        });
    of_player(state, content, player)
        .iter()
        .map(|ability| match ability.as_str() {
            // Analytical waives; Brilliant does not. Brilliant swaps the Technology *secondary*
            // for its primary — see `substitutes_primary`. Registering it here as well gave
            // Jol-Nar two waivers, which is a technology a turn they were never owed.
            "analytical" if !is_upgrade => 1,
            _ => 0,
        })
        .sum()
}

/// Strategy-card secondaries this player resolves as the *primary* instead.
///
/// Jol-Nar's Brilliant swaps Technology's secondary for its primary — a different ability with
/// its own costs, not a modifier on the one already running. The card is named rather than the
/// ability written to apply everywhere: a faction that could swap any secondary for its primary
/// would be playing a different game.
#[must_use]
pub fn substitutes_primary(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    of_player(state, content, player).iter().any(|ability| {
        matches!(ability.as_str(), "brilliant") && card.eq_ignore_ascii_case("technology")
    })
}

/// Convert structures on a planet this player has just taken (L1Z1X's Assimilate).
///
/// 31.4 applies to a structure changing hands as much as to one built: the plastic becomes
/// L1Z1X's, so it comes out of L1Z1X's box, and taking a seventh PDS against the six they own is
/// as impossible as building one. Counted as it goes, because two structures on one planet would
/// otherwise both pass a check made once.
pub fn control_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
    planet: &ti4_model::id::PlanetId,
) {
    if !has(state, content, player, "assimilate") {
        return;
    }
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let types = ti4_content::units::catalogue(content, sources);
    let standing = state
        .system_state(system)
        .planet_units
        .get(planet)
        .cloned()
        .unwrap_or_default();

    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    let mut converted = Vec::with_capacity(standing.len());
    for unit in standing {
        let base = types
            .get(unit.type_id.as_str())
            .map(|kind| kind.base_type().to_owned());
        let convertible = base
            .as_deref()
            .is_some_and(|base| matches!(base, "pds" | "spacedock"));
        if &unit.owner != player && convertible {
            let base = base.unwrap_or_default();
            let already = taken.get(&base).copied().unwrap_or(0);
            let room = crate::supply::remaining(
                state,
                content,
                sources,
                player,
                &ti4_model::id::UnitTypeId::new(&base),
            ) - i64::try_from(already).unwrap_or(i64::MAX);
            if room > 0 {
                let own = ti4_content::units::faction_unit(content, &faction, &base, sources)
                    .map_or(base.clone(), |unit| unit.id().to_owned());
                *taken.entry(base).or_default() += 1;
                converted.push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new(own),
                    player.clone(),
                ));
                continue;
            }
        }
        converted.push(unit);
    }
    state
        .system_mut(system)
        .planet_units
        .insert(planet.clone(), converted);
}

/// Whether this player may include an action card in a transaction (94.3's exception).
#[must_use]
pub fn trades_action_cards(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    // Hacan's Arbiters.
    has(state, content, player, "arbiters")
}

/// Whether this player may transact with anybody, not only their neighbours.
#[must_use]
pub fn ignores_neighbours(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    // Hacan's Guild Ships.
    has(state, content, player, "guild_ships")
}

/// Whether a strategy card's secondary costs this player no token.
#[must_use]
pub fn secondary_is_free(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    of_player(state, content, player).iter().any(|ability| {
        match ability.as_str() {
            // Xxcha's Peace Accords are about Diplomacy; Hacan's Masters of Trade waive Trade.
            "master_of_trade" => card.eq_ignore_ascii_case("trade"),
            _ => false,
        }
    })
}

/// The kind of a faction component action.
pub const ACTION_KIND: &str = "component";

/// Component actions this player's faction offers on their turn.
#[must_use]
pub fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<crate::choice::ChoiceOption> {
    let mut options = Vec::new();
    if has(state, content, player, "orbital_drop")
        && state
            .player(player)
            .is_some_and(|seat| seat.tokens(ti4_model::state::TokenPool::Strategic) > 0)
        && !state.controlled_planets(player).is_empty()
    {
        options.push(crate::choice::ChoiceOption::labelled(
            "faction|orbital_drop",
            ACTION_KIND,
            "Orbital Drop: spend a strategy token to land 2 infantry",
        ));
    }
    options
}

/// Perform a faction component action. Returns `false` for an option that is not one.
#[allow(
    clippy::too_many_lines,
    reason = "Orbital Drop and its optional deploy are one atomic faction action"
)]
pub fn perform_component(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    option: &crate::choice::ChoiceOption,
) -> bool {
    if option.id != "faction|orbital_drop" {
        return false;
    }
    if !has(context.state, context.content, player, "orbital_drop") {
        return false;
    }
    let spots: Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> = context
        .state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect();
    let Some((mut system, mut planet)) = spots.first().cloned() else {
        return false;
    };
    if spots.len() > 1 {
        let choice = crate::choice::Choice::new(
            player.clone(),
            "Orbital Drop: onto which planet",
            spots
                .iter()
                .map(|(system, planet)| {
                    crate::choice::ChoiceOption::labelled(
                        planet.to_string(),
                        "planet",
                        planet.to_string(),
                    )
                    .with("system", system.to_string())
                    .with("planet", planet.to_string())
                })
                .collect(),
        );
        let Ok(answer) = context.ask_seeing(&choice) else {
            return false;
        };
        let Some((chosen_system, chosen_planet)) = spots
            .iter()
            .find(|(_, candidate)| candidate.as_str() == answer.id)
            .cloned()
        else {
            return false;
        };
        system = chosen_system;
        planet = chosen_planet;
    }
    let tokens = context.state.player(player).map_or(0, |seat| {
        seat.tokens(ti4_model::state::TokenPool::Strategic)
    });
    if tokens <= 0 {
        return false; // 22.3: it cannot resolve, so it is not performed
    }
    if let Some(seat) = context.state.player_mut(player) {
        seat.gain_token(ti4_model::state::TokenPool::Strategic, -1);
    }
    crate::action_cards::place_units(context, player, &system, Some(&planet), "infantry", 2);

    let mech = ti4_content::units::faction_unit(context.content, "sol", "mech", context.sources)
        .map_or_else(|| "sol_mech".to_owned(), |kind| kind.id().to_owned());
    if crate::supply::remaining(
        context.state,
        context.content,
        context.sources,
        player,
        &ti4_model::id::UnitTypeId::new(&mech),
    ) > 0
        && crate::production::available(
            context.state,
            context.content,
            context.sources,
            player,
            crate::production::Spend::Resources,
        ) >= 3
    {
        let deploy = crate::choice::ChoiceOption::labelled(
            format!("deploy|{mech}|1"),
            crate::production::PRODUCE_KIND,
            format!("deploy 1 {mech} for 3 resources"),
        )
        .with("unit", mech.clone())
        .with("count", 1)
        .with("cost", 3)
        .with("system", system.to_string())
        .with("planet", planet.to_string())
        .with("orbital_drop_deploy", true);
        let choice = crate::choice::Choice::new(
            player.clone(),
            format!("Orbital Drop: deploy a mech on {planet}"),
            vec![deploy, crate::choice::ChoiceOption::decline()],
        );
        if let Ok(answer) = context.ask_seeing(&choice)
            && !answer.is_decline()
            && crate::production::pay_seeing(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                player,
                3,
                crate::production::Spend::Resources,
            )
            .unwrap_or(false)
        {
            context
                .state
                .system_mut(&system)
                .planet_units
                .entry(planet)
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new(mech),
                    player.clone(),
                ));
        }
    }
    true
}

/// Empty, unowned planets in or next to a system this player already holds (Xxcha's Peace
/// Accords).
///
/// "Does not contain any units" is checked against *every* unit on the planet, not only other
/// players' — a planet with your own troops on it is not empty either, and reading it as "no
/// enemy units" would let Xxcha annex around the rules.
#[must_use]
pub fn annexable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(ti4_model::id::SystemId, ti4_model::id::PlanetId)> {
    let mine: std::collections::BTreeSet<String> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, _)| system.to_string())
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let mut reachable = mine.clone();
    for system in &mine {
        reachable.extend(galaxy.adjacent(system).into_iter().map(ToOwned::to_owned));
    }

    // F-M08-019-1: iterate each system's planets in the order of the system record's own
    // `planets` array (the oracle's order), not the file layout of planets.json. The two
    // sources agree on membership for every system in this corpus; only their orders differ.
    // C2: resolve those records through the *active* content domain and source scope — a fresh
    // embedded store would ignore perturbed live stores and alternate scopes entirely. The
    // scope check is a no-op for reachable systems (they exist in the active galaxy by
    // construction) but keeps the function honest about its domain.
    let mut found = Vec::new();
    for system in reachable {
        let id = ti4_model::id::SystemId::new(&system);
        let board = state.system_state(&id);
        let Some(system_record) = content
            .get(ContentType::Systems, &system)
            .filter(|record| record.in_sources(sources))
        else {
            continue;
        };
        for planet_id in system_record.strings("planets") {
            let planet = ti4_model::id::PlanetId::new(planet_id);
            if planet.as_str() == "mr" || board.planet_control.contains_key(&planet) {
                continue; // somebody holds it
            }
            if board
                .planet_units
                .get(&planet)
                .is_some_and(|units| !units.is_empty())
            {
                continue; // anybody's units, not only a rival's
            }
            found.push((id.clone(), planet));
        }
    }
    found
}

/// Resolve anything a faction does when a strategy card finishes for this player.
///
/// Xxcha's Peace Accords annex a planet after Diplomacy.
pub fn strategy_resolved(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    card: &str,
) {
    if !has(context.state, context.content, player, "peace_accords")
        || !card.eq_ignore_ascii_case("diplomacy")
    {
        return;
    }
    let Some(galaxy) = context.galaxy else {
        return; // "in or next to" needs the map
    };
    let candidates = annexable(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    if candidates.is_empty() {
        return;
    }
    let mut options: Vec<crate::choice::ChoiceOption> = candidates
        .iter()
        .map(|(system, planet)| {
            crate::choice::ChoiceOption::labelled(
                planet.to_string(),
                "annex",
                format!("gain control of {planet}"),
            )
            .with("system", system.to_string())
            .with("planet", planet.to_string())
        })
        .collect();
    options.push(crate::choice::ChoiceOption::decline());
    let choice =
        crate::choice::Choice::new(player.clone(), "Peace Accords: annex a planet", options);
    let Ok(answer) = context.ask_seeing(&choice) else {
        return;
    };
    if answer.is_decline() {
        return;
    }
    let Some((chosen_system, chosen_planet)) = candidates
        .iter()
        .find(|(_, candidate)| candidate.as_str() == answer.id)
        .cloned()
    else {
        return;
    };
    let system = chosen_system;
    let planet = chosen_planet;
    context
        .state
        .system_mut(&system)
        .set_control(planet.clone(), player.clone());

    let _ = crate::technology::control_gained(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        &system,
        &planet,
    );

    if let Some(deck) = crate::exploration::trait_of(context.content, context.sources, &planet) {
        let mut resolving = crate::choice::Resolving {
            content: context.content,
            sources: context.sources,
            dice: context.dice,
            rng: context.rng,
            table: context.table,
            timing: None,
        };
        let _ = crate::exploration::explore_with(
            context.state,
            &mut resolving,
            player,
            &deck,
            Some(&planet),
        );
    }
}

/// Bombard a planet again at the end of a ground-combat round (L1Z1X's Harrow).
///
/// Returns the hits produced. The caller assigns them, because who loses a unit is the invasion's
/// decision and not this layer's.
pub fn ground_combat_round_ended(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    player: &PlayerId,
    system: &ti4_model::id::SystemId,
) -> usize {
    if !has(state, content, player, "harrow") {
        return 0;
    }
    let types = ti4_content::units::catalogue(content, sources);
    let mut hits = 0;
    for unit in state.system_state(system).units_of(player) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        if !kind.has_bombardment() {
            continue;
        }
        let count = usize::try_from(kind.bombard_dice()).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let roll = dice.roll(
            rng,
            count,
            "harrow",
            kind.bombard_hits_on().and_then(|on| u32::try_from(on).ok()),
        );
        hits += roll.hits();
    }
    hits
}

/// The cost of Munitions Reserves, paid at each combat round's opening window.
const MUNITIONS_COST: i32 = 2;

/// Offer anything a faction does at the start of a space-combat round.
///
/// Letnev's Munitions Reserves is paid *per round*: the marker is scoped to
/// `combat_round_seq`, so paying once does not buy rerolls for the rest of the fight.
pub fn space_combat_round_started(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
) {
    if !has(state, content, player, "munitions") {
        return;
    }
    let held = state.player(player).map_or(0, |seat| seat.trade_goods);
    if held < MUNITIONS_COST {
        return; // it cannot resolve, so it is not offered
    }
    let choice = crate::choice::Choice::new(
        player.clone(),
        "spend 2 trade goods for Munitions Reserves",
        vec![
            crate::choice::ChoiceOption::labelled(
                "munitions",
                "ability",
                "reroll this round's misses",
            ),
            crate::choice::ChoiceOption::decline(),
        ],
    );
    let Ok(answer) = table.ask_seeing(
        &choice,
        &crate::choice::Observed::new(state, content, sources, None),
    ) else {
        return;
    };
    if answer.is_decline() {
        return;
    }
    let round = state.combat_round_seq;
    if let Some(seat) = state.player_mut(player) {
        seat.trade_goods -= MUNITIONS_COST;
        seat.munitions_round = Some(round);
    }
}

// -- coverage ------------------------------------------------------------------------------------

/// Every ability the corpus prints, by id.
#[must_use]
pub fn catalogue(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    content
        .from_sources(ContentType::Abilities, sources)
        .filter_map(|record| record.text("id").or_else(|| record.text("alias")))
        .map(ToOwned::to_owned)
        .collect()
}

/// Ability ids this layer answers.
#[must_use]
pub fn registered() -> Vec<&'static str> {
    vec![
        "analytical",
        "arbiters",
        "armada",
        "assimilate",
        "brilliant",
        "fragile",
        "guild_ships",
        "harrow",
        "master_of_trade",
        "munitions",
        "orbital_drop",
        "peace_accords",
        "unrelenting",
        "versatile",
    ]
}

/// Printed abilities nothing here answers, excluding the ones [`blocked`] explains.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<String> {
    let known = registered();
    let blocked = blocked();
    catalogue(content, sources)
        .into_iter()
        .filter(|id| !known.contains(&id.as_str()))
        .filter(|id| !blocked.contains_key(id.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::game;
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;

    fn seated(faction: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = game(&["a", "b"]);
        state.player_mut(&player).unwrap().faction = FactionId::new(faction);
        (state, player)
    }

    /// The faction that prints an ability, if this corpus has one.
    fn faction_with(ability: &str) -> Option<String> {
        ti4_content::factions::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, faction)| faction.abilities().contains(&ability))
            .map(|(alias, _)| (*alias).to_owned())
    }

    #[test]
    fn a_faction_with_nothing_to_say_changes_nothing() {
        // Every hook is a query with a default, so a subsystem calls it unconditionally.
        let (state, player) = seated("sol");
        let content = ContentStore::embedded();

        assert_eq!(fleet_supply(&state, content, &player, 4), 4);
        assert_eq!(
            waived_prerequisites(&state, content, POK, &player, "any"),
            0
        );
        assert!(!trades_action_cards(&state, content, &player));
        assert!(!ignores_neighbours(&state, content, &player));
    }

    #[test]
    fn an_ability_belongs_to_whoever_has_the_card() {
        let Some(letnev) = faction_with("armada") else {
            return; // this corpus does not print Armada
        };
        let content = ContentStore::embedded();
        let (with, player) = seated(&letnev);
        let (without, _) = seated("sol");

        assert_eq!(
            fleet_supply(&with, content, &player, 4),
            6,
            "Armada allows two more"
        );
        assert_eq!(
            fleet_supply(&without, content, &player, 4),
            4,
            "and nobody else gets it"
        );
    }

    #[test]
    fn a_die_shift_is_signed() {
        // Fragile subtracts and Unrelenting adds. A hook that returned a magnitude would make
        // Jol-Nar the best shots in the game.
        let content = ContentStore::embedded();
        if let Some(jolnar) = faction_with("fragile") {
            let (state, player) = seated(&jolnar);
            assert_eq!(combat_modifier(&state, content, &player, "space"), -1);
        }
        if let Some(sardakk) = faction_with("unrelenting") {
            let (state, player) = seated(&sardakk);
            assert_eq!(combat_modifier(&state, content, &player, "space"), 1);
        }
    }

    #[test]
    fn armada_reaches_the_fleet_limit_that_is_actually_enforced() {
        // The hook existing is not the same as a subsystem asking it. `fleet::limit` is what
        // enforcement reads, so that is what this checks.
        let Some(letnev) = faction_with("armada") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&letnev);
        state.player_mut(&player).unwrap().fleet_tokens = 3;

        assert_eq!(
            crate::fleet::limit(&state, content, &player),
            5,
            "three tokens and Armada's two"
        );

        let (mut plain, other) = seated("sol");
        plain.player_mut(&other).unwrap().fleet_tokens = 3;
        assert_eq!(crate::fleet::limit(&plain, content, &other), 3);
    }

    #[test]
    fn a_combat_shift_reaches_the_threshold_a_unit_actually_rolls_against() {
        let content = ContentStore::embedded();
        let Some(sardakk) = faction_with("unrelenting") else {
            return;
        };
        let (state, player) = seated(&sardakk);
        let (plain, other) = seated("sol");
        let unit =
            ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new("cruiser"), player.clone());

        let theirs = crate::combat::effective_hits_on(&state, content, POK, &player, &unit);
        let ordinary = crate::combat::effective_hits_on(&plain, content, POK, &other, &unit);

        assert!(
            theirs < ordinary,
            "Unrelenting hits on a lower number: {theirs:?} against {ordinary:?}"
        );
    }

    #[test]
    fn guild_ships_makes_the_whole_table_a_partner() {
        // 60.1 says neighbours; the card says anybody. A player with no neighbours at all still
        // has somebody to trade with.
        let Some(hacan) = faction_with("guild_ships") else {
            return;
        };
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let (mut state, player) = seated(&hacan);
        // Nobody is anywhere near anybody.
        assert!(
            crate::transactions::neighbours(&state, &hub.galaxy, &player).is_empty(),
            "no fleets are placed, so nobody is a neighbour"
        );

        let reachable = crate::transactions::partners(&state, content, &hub.galaxy, &player);
        assert!(
            !reachable.is_empty(),
            "Guild Ships reaches the table anyway"
        );
        assert!(!reachable.contains(&player), "but not yourself");

        state.player_mut(&player).unwrap().faction = FactionId::new("sol");
        assert!(
            crate::transactions::partners(&state, content, &hub.galaxy, &player).is_empty(),
            "and nobody else gets it"
        );
    }

    #[test]
    fn analytical_waives_a_prerequisite_but_not_for_a_unit_upgrade() {
        // 90.7b is the whole point of the ability's window: an upgrade carries prerequisites but
        // the card does not open for it, so waiving there would research upgrades it never meant
        // to touch.
        let Some(jolnar) = faction_with("analytical") else {
            return;
        };
        let content = ContentStore::embedded();
        let (state, player) = seated(&jolnar);

        let upgrade = content
            .from_sources(ContentType::Technologies, POK)
            .find(|record| {
                record
                    .text("baseUpgrade")
                    .is_some_and(|base| !base.is_empty())
            })
            .and_then(|record| record.text("alias").map(ToOwned::to_owned));
        let ordinary = content
            .from_sources(ContentType::Technologies, POK)
            .find(|record| {
                record.text("baseUpgrade").is_none_or(str::is_empty)
                    && record.text("faction").is_none()
            })
            .and_then(|record| record.text("alias").map(ToOwned::to_owned));
        let (Some(upgrade), Some(ordinary)) = (upgrade, ordinary) else {
            panic!("the corpus has both a unit upgrade and an ordinary technology");
        };

        assert_eq!(
            waived_prerequisites(&state, content, POK, &player, &ordinary),
            1,
            "an ordinary technology gets the waiver"
        );
        assert_eq!(
            waived_prerequisites(&state, content, POK, &player, &upgrade),
            0,
            "a unit upgrade does not"
        );
    }

    #[test]
    fn a_waived_prerequisite_reaches_what_can_actually_be_researched() {
        // The hook existing is not the subsystem asking it.
        let Some(jolnar) = faction_with("analytical") else {
            return;
        };
        let content = ContentStore::embedded();
        let (state, player) = seated(&jolnar);
        let (plain, other) = seated("sol");

        let theirs = crate::technology::researchable(&state, content, POK, &player).len();
        let ordinary = crate::technology::researchable(&plain, content, POK, &other).len();

        assert!(
            theirs > ordinary,
            "a waived prerequisite opens more technologies: {theirs} against {ordinary}"
        );
    }

    #[test]
    fn assimilate_takes_the_structures_and_pays_for_them_out_of_its_own_box() {
        // 31.4 applies to a structure changing hands as much as to one built: a seventh PDS is
        // as impossible taken as it is built.
        let Some(l1z1x) = faction_with("assimilate") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&l1z1x);
        let rival = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &rival, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &rival, 1);

        control_gained(&mut state, content, POK, &player, &system, &planet);

        let units = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .cloned()
            .unwrap_or_default();
        let mine: Vec<&ti4_model::units::Unit> =
            units.iter().filter(|unit| unit.owner == player).collect();
        assert_eq!(mine.len(), 1, "the structure changed hands");
        assert!(
            units.iter().any(|unit| unit.owner == rival),
            "and the ground forces did not"
        );
    }

    #[test]
    fn assimilate_stops_at_the_box() {
        let Some(l1z1x) = faction_with("assimilate") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&l1z1x);
        let rival = PlayerId::new("b");
        let (system, planet) = crate::fixtures::a_placed_planet();
        // Every PDS this player owns is already on the board somewhere.
        let elsewhere = crate::fixtures::plain_systems(2);
        crate::fixtures::put_on_planet(
            &mut state,
            &ti4_model::id::SystemId::new(elsewhere[0].clone()),
            &planet,
            "pds",
            &player,
            6,
        );
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &rival, 1);

        control_gained(&mut state, content, POK, &player, &system, &planet);

        let taken = state
            .system_state(&system)
            .planet_units
            .get(&planet)
            .map_or(0, |units| {
                units.iter().filter(|unit| unit.owner == player).count()
            });
        assert_eq!(taken, 0, "there is no seventh PDS to take it with");
    }

    /// Run a faction hook with a real context.
    fn with_table_context<T>(
        state: &mut GameState,
        galaxy: Option<&ti4_content::galaxy::Galaxy>,
        table: &mut crate::choice::Table,
        run: impl FnOnce(&mut crate::timing::TimingContext<'_>) -> T,
    ) -> T {
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy,
        };
        run(&mut context)
    }

    #[test]
    fn orbital_drop_costs_a_strategy_token_and_lands_two() {
        let Some(sol) = faction_with("orbital_drop") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&sol);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        state
            .player_mut(&player)
            .unwrap()
            .gain_token(ti4_model::state::TokenPool::Strategic, 1);
        let before = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Strategic);

        let offered = component_actions(&state, content, &player);
        assert_eq!(offered.len(), 1, "it is offered on your turn");

        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                planet.to_string()
            ])));
        let done = with_table_context(&mut state, None, &mut table, |context| {
            perform_component(context, &player, &offered[0])
        });

        assert!(done);
        assert_eq!(
            state
                .player(&player)
                .unwrap()
                .tokens(ti4_model::state::TokenPool::Strategic),
            before - 1,
            "the token was spent"
        );
        assert_eq!(
            state
                .system_state(&system)
                .planet_units
                .get(&planet)
                .map_or(0, |units| {
                    let types = ti4_content::units::catalogue(ContentStore::embedded(), POK);
                    units
                        .iter()
                        .filter(|unit| {
                            types
                                .get(unit.type_id.as_str())
                                .is_some_and(|kind| kind.base_type() == "infantry")
                        })
                        .count()
                }),
            2,
            "two infantry landed"
        );
    }

    #[test]
    fn orbital_drop_is_not_offered_without_a_token() {
        let Some(sol) = faction_with("orbital_drop") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&sol);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet, player.clone());
        let held = state
            .player(&player)
            .unwrap()
            .tokens(ti4_model::state::TokenPool::Strategic);
        state
            .player_mut(&player)
            .unwrap()
            .gain_token(ti4_model::state::TokenPool::Strategic, -held);

        assert!(component_actions(&state, content, &player).is_empty());
    }

    #[test]
    fn orbital_drop_asks_which_controlled_planet_receives_the_units() {
        let Some(sol) = faction_with("orbital_drop") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&sol);
        let first_system = ti4_model::id::SystemId::new("01");
        let second_system = ti4_model::id::SystemId::new("02");
        let first_planet = ti4_model::id::PlanetId::new("first");
        let second_planet = ti4_model::id::PlanetId::new("second");
        state
            .system_mut(&first_system)
            .set_control(first_planet.clone(), player.clone());
        state
            .system_mut(&second_system)
            .set_control(second_planet.clone(), player.clone());
        state
            .player_mut(&player)
            .unwrap()
            .gain_token(ti4_model::state::TokenPool::Strategic, 1);
        let offered = component_actions(&state, content, &player);
        let mut table =
            crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new([
                second_planet.to_string(),
            ])));

        assert!(with_table_context(
            &mut state,
            None,
            &mut table,
            |context| { perform_component(context, &player, &offered[0]) }
        ));

        assert_eq!(
            state
                .system_state(&first_system)
                .on_planet(&first_planet)
                .len(),
            0
        );
        assert_eq!(
            state
                .system_state(&second_system)
                .on_planet(&second_planet)
                .len(),
            2
        );
        assert_eq!(
            table.log.records[0].prompt,
            "Orbital Drop: onto which planet"
        );
    }

    #[test]
    fn peace_accords_annex_only_an_empty_unowned_planet() {
        // "Does not contain any units" means anybody's units. Reading it as "no enemy units"
        // would let Xxcha annex a planet their own troops are standing on.
        let Some(xxcha) = faction_with("peace_accords") else {
            return;
        };
        let hub = crate::fixtures::plain_hub();
        let (mut state, player) = seated(&xxcha);
        let mine = ti4_model::id::SystemId::new(hub.centre.clone());
        let held =
            ti4_content::galaxy::planets_in(ContentStore::embedded(), hub.centre.as_str(), POK)
                .first()
                .map(|planet| ti4_model::id::PlanetId::new(planet.id()));
        let Some(held) = held else {
            return;
        };
        state.system_mut(&mine).set_control(held, player.clone());

        let open = annexable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        assert!(!open.is_empty(), "a neighbouring empty planet is annexable");

        // Put a unit of this player's own on the first candidate: it stops being empty.
        let (system, planet) = open[0].clone();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, 1);
        let after = annexable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        assert!(
            !after.contains(&(system, planet)),
            "your own troops make it not empty either"
        );
    }

    #[test]
    fn peace_accords_can_decline_instead_of_hardcoding_the_first_planet() {
        let Some(xxcha) = faction_with("peace_accords") else {
            return;
        };
        let hub = crate::fixtures::plain_hub();
        let (mut state, player) = seated(&xxcha);
        let mine = ti4_model::id::SystemId::new(hub.centre.clone());
        let Some(held) =
            ti4_content::galaxy::planets_in(ContentStore::embedded(), hub.centre.as_str(), POK)
                .first()
                .map(|planet| ti4_model::id::PlanetId::new(planet.id()))
        else {
            return;
        };
        state.system_mut(&mine).set_control(held, player.clone());
        let before = state.controlled_planets(&player).len();
        let mut table = crate::choice::Table::with_default(Box::new(crate::choice::AlwaysDecline));

        with_table_context(&mut state, Some(&hub.galaxy), &mut table, |context| {
            strategy_resolved(context, &player, "Diplomacy");
        });

        assert_eq!(state.controlled_planets(&player).len(), before);
        let record = table.log.records.last().expect("Peace Accords was offered");
        assert_eq!(record.prompt, "Peace Accords: annex a planet");
        assert!(record.offered.iter().any(|id| id == "decline"));
    }

    #[test]
    fn peace_accords_candidates_follow_the_system_record_planet_order() {
        // F-M08-019-1: candidate order must follow the system record's own `planets` array,
        // not the file layout of planets.json. System 58 prints [valk, ylir, avar]; in
        // planets.json file order avar precedes valk — so with ylir controlled, the two empty
        // candidates swap relative order between the old and new code.
        //
        // C2 re-point: the original fixture used system 110, which is a Thunder's Edge system
        // outside POK scope. That only worked because the pre-C2 `annexable` resolved records
        // from a fresh embedded store with no source scope — an impossible production scenario.
        // System 58 (pok) exercises the same property inside the active domain.
        let Some(xxcha) = faction_with("peace_accords") else {
            return;
        };
        let hub = crate::fixtures::plain_hub();
        let (mut state, player) = seated(&xxcha);
        let system = ti4_model::id::SystemId::new("58");
        let held = ti4_model::id::PlanetId::new("ylir");
        state.system_mut(&system).set_control(held, player.clone());

        let open = annexable(&state, ContentStore::embedded(), POK, &hub.galaxy, &player);
        let valk = (system.clone(), ti4_model::id::PlanetId::new("valk"));
        let avar = (system.clone(), ti4_model::id::PlanetId::new("avar"));
        assert!(open.contains(&valk), "valk is empty and unowned");
        assert!(open.contains(&avar), "avar is empty and unowned");
        let i_valk = open.iter().position(|c| *c == valk).expect("checked above");
        let i_avar = open.iter().position(|c| *c == avar).expect("checked above");
        assert!(
            i_valk < i_avar,
            "the system record puts valk before avar; planets.json file order would not"
        );
    }

    #[test]
    fn harrow_bombards_again_at_the_end_of_a_ground_round() {
        let Some(l1z1x) = faction_with("harrow") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&l1z1x);
        let (system, _) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &system, "dreadnought", &player, 1);
        let mut dice = crate::dice::Dice::from_faces(vec![10, 10, 10]);
        let mut rng = crate::rng::GameRng::new(0);

        let hits =
            ground_combat_round_ended(&state, content, POK, &mut dice, &mut rng, &player, &system);
        assert!(hits > 0, "a bombarding hull rolls again");

        // The control needs the same fleet, or it returns zero because there is nothing to
        // bombard with rather than because the faction lacks the ability.
        let (mut plain, other) = seated("sol");
        crate::fixtures::put(&mut plain, &system, "dreadnought", &other, 1);
        let mut dice = crate::dice::Dice::from_faces(vec![10, 10, 10]);
        assert_eq!(
            ground_combat_round_ended(&plain, content, POK, &mut dice, &mut rng, &other, &system),
            0,
            "and nobody else gets it"
        );
    }

    #[test]
    fn munitions_reserves_is_paid_per_round_not_per_fight() {
        // The marker is scoped to the combat round, so paying once does not buy rerolls for the
        // rest of the fight — which is what an unscoped flag would do.
        let Some(letnev) = faction_with("munitions") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&letnev);
        state.player_mut(&player).unwrap().trade_goods = 5;
        state.combat_round_seq = 3;
        let mut table = crate::choice::Table::new();

        space_combat_round_started(&mut state, content, POK, &mut table, &player);

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 3, "two paid");
        assert_eq!(seat.munitions_round, Some(3), "for this round only");
    }

    #[test]
    fn munitions_reserves_never_falls_back_to_a_blind_decision() {
        struct SeeingDecline;

        impl crate::choice::Decider for SeeingDecline {
            fn choose(
                &mut self,
                _choice: &crate::choice::Choice,
            ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
                panic!("learned decisions must not use the blind path")
            }

            fn choose_seeing(
                &mut self,
                _choice: &crate::choice::Choice,
                _seen: &crate::choice::SeatObservation<'_>,
            ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
                Ok(crate::choice::ChoiceOption::decline())
            }
        }

        let Some(letnev) = faction_with("munitions") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&letnev);
        state.player_mut(&player).unwrap().trade_goods = 5;
        let mut table = crate::choice::Table::new();
        table.seat(player.clone(), Box::new(SeeingDecline));

        space_combat_round_started(&mut state, content, POK, &mut table, &player);

        assert_eq!(state.player(&player).unwrap().trade_goods, 5);
        assert_eq!(table.log.records.last().unwrap().chosen, "decline");
    }

    #[test]
    fn munitions_reserves_is_not_offered_to_a_player_who_cannot_pay() {
        let Some(letnev) = faction_with("munitions") else {
            return;
        };
        let content = ContentStore::embedded();
        let (mut state, player) = seated(&letnev);
        state.player_mut(&player).unwrap().trade_goods = 1;
        let mut table = crate::choice::Table::new();

        space_combat_round_started(&mut state, content, POK, &mut table, &player);

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 1, "nothing was taken");
        assert_eq!(seat.munitions_round, None);
    }

    #[test]
    fn another_faction_is_never_charged_for_munitions() {
        let content = ContentStore::embedded();
        let (mut state, player) = seated("sol");
        state.player_mut(&player).unwrap().trade_goods = 5;
        let mut table = crate::choice::Table::new();

        space_combat_round_started(&mut state, content, POK, &mut table, &player);

        assert_eq!(state.player(&player).unwrap().trade_goods, 5);
    }

    #[test]
    fn the_blocked_abilities_are_named_with_a_reason() {
        // Kept apart from merely unwritten ones: one is work, the other is a dependency, and a
        // single number covering both hides which.
        let blocked = blocked();
        assert!(!blocked.is_empty());
        for (ability, reason) in &blocked {
            assert!(!reason.is_empty(), "{ability} is blocked on nothing stated");
            assert!(
                !registered().contains(ability),
                "{ability} is both blocked and registered"
            );
        }
    }

    #[test]
    fn every_registered_ability_is_one_the_corpus_prints() {
        // The trap this project has hit four times: an id somebody was sure of does not exist,
        // and the ability is unreachable for ever with nothing to say so.
        let printed = catalogue(ContentStore::embedded(), ti4_model::content_types::FULL);
        for ability in registered() {
            assert!(
                printed.contains(&ability.to_owned()),
                "{ability} is not an ability the corpus knows"
            );
        }
    }

    #[test]
    fn the_gap_is_reported_rather_than_implied() {
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(!missing.is_empty(), "most abilities are still unanswered");
        for ability in registered() {
            assert!(!missing.contains(&ability.to_owned()));
        }
    }
}
