//! Legendary planet abilities.
//!
//! Sixteen legendary planets exist, described by nineteen corpus records -- Mallice has a tile
//! face each side of the Nexus flip, and Mirage carries two alternate printings that nothing
//! places. Each has a `legendaryAbilityName` and its text, and until this module nothing read
//! either: `Planet::is_legendary` answered a bool used for excluding them from
//! Stellar Converter and counting them for objectives, and the printed ability did nothing at all.
//!
//! # Shape
//!
//! A registry keyed by planet, like [`crate::action_cards::effect_for`] and the relic table: a
//! planet with no entry is a visible gap rather than a silent one. Most of these are "you may
//! exhaust this card at the end of your turn", which is a window the engine already drives --
//! `technology::end_turn` runs it for Bio-Stims and Predictive Intelligence -- so the abilities
//! hang off that rather than needing timing machinery of their own.
//!
//! Three more read "when you pass". That window did not exist; [`pass`] is it, called from the
//! pass branch of the action phase.
//!
//! Two abilities are neither: the Ionian Fuel Refinery is offered as the extra reach it buys, from
//! inside the movement step, and Dok 'N Pic's Salvage Yard is half a window and half a card zone --
//! stocking it is a pass ability, playing out of it is a second place the play paths look.
//!
//! The ability card exhausts separately from the planet (`seat.exhausted_legendary`, readied in
//! the status phase beside technologies and relics), because a seat can spend Primor for its two
//! resources and still use The Atrament in the same round.
//!
//! # Not here
//!
//! Avernus, Ordinian and Custodia Vigilia are faction-specific -- they borrow another faction's
//! ability, sit in a faction's starting position, or arrive with a faction technology -- and are
//! deliberately left out rather than half-modelled.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};

/// Whether this seat holds the planet and has not spent its ability card this round.
#[must_use]
pub fn available(state: &GameState, player: &PlayerId, planet: &PlanetId) -> bool {
    let holds = state
        .controlled_planets(player)
        .into_iter()
        .any(|(_, held)| held == planet);
    holds
        && state
            .player(player)
            .is_some_and(|seat| !seat.exhausted_legendary.contains(planet))
}

/// Spend the ability card.
fn exhaust(state: &mut GameState, player: &PlayerId, planet: &PlanetId) {
    if let Some(seat) = state.player_mut(player) {
        seat.exhausted_legendary.insert(planet.clone());
    }
}

/// Every legendary ability this seat could use at the end of its turn, as options.
///
/// One option per planet rather than one per outcome: the choice of *which* ability is separate
/// from the choice each ability then offers, and folding them together would make declining one
/// ability read as declining all of them.
fn end_of_turn_offers(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    END_OF_TURN
        .iter()
        .map(|(planet, _)| PlanetId::new(*planet))
        .filter(|planet| available(state, player, planet))
        .collect()
}

/// Planets whose ability is used at the end of the holder's turn, with the label to offer.
///
/// Seven planets, eight rows: Mallice appears twice because the Nexus has two faces and the planet
/// on the locked tile is `lockedmallice`, so whichever face is up is the id in play.
///
/// The corpus also carries `illusion` and `phantasm` -- alternate printings of Mirage, same stats
/// and same ability with the name changed. They are not here because nothing places them: the
/// Mirage frontier card places `mirage` and only `mirage` (`exploration.rs`). Sixteen legendary
/// planets exist; nineteen records describe them.
const END_OF_TURN: [(&str, &str); 8] = [
    ("hopesend", "Imperial Arms Vault"),
    ("primor", "The Atrament"),
    ("mallice", "Exterrix Headquarters"),
    ("lockedmallice", "Exterrix Headquarters"),
    ("mirage", "Mirage Flight Academy"),
    ("emelpar", "The Acropolis"),
    ("mrte", "The Galactic Council"),
    ("thundersedge", "Jupiter Brain"),
];

/// Tempesta, whose card is spent from inside the movement step rather than from a window.
const IONIAN: &str = "tempesta";

/// Garbozia, whose card is a zone other cards sit in.
const SALVAGE: &str = "garbozia";

/// Action cards this seat may play out of Dok 'N Pic's Salvage Yard.
///
/// "You can purge cards on this card to play them as if they were in your hand." The zone belongs
/// to the planet, so it answers empty for everyone except whoever currently holds Garbozia --
/// including a player who took the planet off the seat that stocked it.
///
/// Unlike the abilities either side of this, holding the planet is the whole requirement: the
/// ability card is exhausted to *stock* the yard, not to play out of it, so an exhausted card does
/// not lock what is already lying on it.
#[must_use]
pub fn salvaged(state: &GameState, player: &PlayerId) -> Vec<ti4_model::id::ActionCardId> {
    let holds = state
        .controlled_planets(player)
        .into_iter()
        .any(|(_, held)| held.as_str() == SALVAGE);
    if holds {
        state.salvage_yard.clone()
    } else {
        Vec::new()
    }
}

/// Purge one card off the salvage yard, which is how playing it is paid for.
///
/// `false` if this seat could not have played it, so a caller that also reads the hand can try
/// this second and fall through cleanly.
pub fn purge_salvaged(
    state: &mut GameState,
    player: &PlayerId,
    alias: &ti4_model::id::ActionCardId,
) -> bool {
    if !salvaged(state, player).contains(alias) {
        return false;
    }
    // Purged, not discarded: it must not come back round through the discard pile it came from.
    if let Some(at) = state.salvage_yard.iter().position(|held| held == alias) {
        state.salvage_yard.remove(at);
        return true;
    }
    false
}

/// Whether the Ionian Fuel Refinery could add +1 to one ship's move right now.
///
/// "You may exhaust this card after you activate a system to apply +1 to the move value of 1 of
/// your ships during this tactical action." There is no separate question: the ability *is* the
/// extra reach, so it is offered as the move it makes possible and read back by
/// [`crate::tactical::movable`]. Offering it as its own yes/no first would ask a seat to spend a
/// card before it knew what the card bought.
#[must_use]
pub fn ionian_available(state: &GameState, player: &PlayerId) -> bool {
    available(state, player, &PlanetId::new(IONIAN))
}

/// Spend the Ionian Fuel Refinery on a move. `false` if it was not there to spend.
///
/// Exhausting is what makes it one ship and once a round: the card readies in the status phase,
/// so a second boosted move is not offered until the next round.
pub fn use_ionian(state: &mut GameState, player: &PlayerId) -> bool {
    if !ionian_available(state, player) {
        return false;
    }
    exhaust(state, player, &PlanetId::new(IONIAN));
    true
}

/// Settle the victory point A Song Like Marrow attaches to Styx.
///
/// "When you gain this card, gain 1 victory point. When you lose this card, lose 1 victory point."
/// The point follows control, so it is not scored once and kept -- taking Styx off somebody moves
/// a point across the table.
///
/// Reconciled from the board rather than hooked onto a control-change event, because control
/// changes hands through dozens of paths in this engine and a VP swing that misses one of them is
/// silently wrong in a way nothing would surface. [`GameState::styx_holder`] records who the point
/// was last settled with; anything else is a difference to pay out. Idempotent, so calling it
/// every step costs a lookup and nothing else.
///
/// Losing is floored at zero: a seat cannot be pushed into negative points by handing back a card
/// whose point they never actually banked, which is possible when Styx is taken from a seat that
/// gained it while already at the cap.
pub fn settle_control_points(state: &mut GameState) {
    let styx = PlanetId::new("styx");
    let holder = state
        .board
        .values()
        .find_map(|here| here.planet_control.get(&styx).cloned());
    if holder == state.styx_holder {
        return;
    }
    if let Some(previous) = state.styx_holder.clone()
        && let Some(seat) = state.player_mut(&previous)
    {
        seat.victory_points = (seat.victory_points - 1).max(0);
    }
    if let Some(gained) = holder.clone()
        && let Some(seat) = state.player_mut(&gained)
    {
        seat.victory_points =
            (seat.victory_points + 1).min(crate::objectives::VICTORY_TARGET);
    }
    state.styx_holder = holder;
}

/// The clause a legendary card resolves the moment its planet changes hands.
///
/// Only Jupiter Brain has one: "gain your breakthrough when you gain this card if you do not
/// already have it". It matters when Thunder's Edge is *taken*, not when it is placed -- the
/// placer claimed an expedition slice, and the first slice already granted them the breakthrough.
/// An invader who never funded the expedition gets it here.
///
/// Called beside `technology::control_gained`, from every path that hands a planet over.
pub fn control_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planet: &PlanetId,
) {
    if planet.as_str() != "thundersedge" {
        return;
    }
    let Some(faction) = state.player(player).map(|seat| seat.faction.to_string()) else {
        return;
    };
    if state
        .player(player)
        .is_some_and(|seat| seat.breakthrough.is_some())
    {
        return; // "if you do not already have it"
    }
    if let Some(breakthrough) = crate::thunders_edge::breakthrough_for(content, sources, &faction)
        && let Some(seat) = state.player_mut(player)
    {
        seat.breakthrough = Some(breakthrough);
    }
}

/// Offer this seat's end-of-turn legendary abilities, one at a time.
///
/// # Errors
/// [`IllegalChoice`] if a decider answers with something that was not offered.
pub fn end_turn(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    loop {
        let offers = end_of_turn_offers(state, player);
        if offers.is_empty() {
            return Ok(());
        }
        let mut options: Vec<ChoiceOption> = offers
            .iter()
            .map(|planet| {
                let label = END_OF_TURN
                    .iter()
                    .find(|(id, _)| *id == planet.as_str())
                    .map_or("legendary ability", |(_, label)| *label);
                ChoiceOption::labelled(planet.to_string(), "legendary", label)
            })
            .collect();
        options.push(ChoiceOption::decline());
        let choice = Choice::new(
            player.clone(),
            "use a legendary planet ability",
            options,
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("legendary".to_owned()),
            "legendary_end_of_turn",
            state.phase,
            state.round,
        ));
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        if answer.is_decline() {
            return Ok(());
        }
        let planet = PlanetId::new(answer.id);
        // Exhausted first: an ability that asks a follow-up question must not be re-offered
        // inside its own resolution.
        exhaust(state, player, &planet);
        resolve(state, content, sources, galaxy, table, player, &planet)?;
    }
}


/// Planets whose ability is used when the holder passes, with the label to offer.
///
/// Ordinian's two faces are faction-specific and left out with Avernus and Custodia Vigilia.
const WHEN_YOU_PASS: [(&str, &str); 3] = [
    ("faunus", "Maxis Central Control"),
    ("industrex", "Aurex Mechanica"),
    ("garbozia", "Dok 'N Pic's Salvage Yard"),
];

/// Offer this seat's when-you-pass legendary abilities, one at a time.
///
/// Takes a [`crate::timing::TimingContext`] rather than the table alone because Maxis Central
/// Control gains control of a planet, and gaining control explores the planet's trait -- which
/// draws a card, which needs the dice and the random stream.
pub fn pass(context: &mut crate::timing::TimingContext<'_>, player: &PlayerId) {
    loop {
        let offers: Vec<PlanetId> = WHEN_YOU_PASS
            .iter()
            .map(|(planet, _)| PlanetId::new(*planet))
            .filter(|planet| available(context.state, player, planet))
            .collect();
        if offers.is_empty() {
            return;
        }
        let mut options: Vec<ChoiceOption> = offers
            .iter()
            .map(|planet| {
                let label = WHEN_YOU_PASS
                    .iter()
                    .find(|(id, _)| *id == planet.as_str())
                    .map_or("legendary ability", |(_, label)| *label);
                ChoiceOption::labelled(planet.to_string(), "legendary", label)
            })
            .collect();
        options.push(ChoiceOption::decline());
        let choice = Choice::new(
            player.clone(),
            "use a legendary planet ability as you pass",
            options,
        )
        .contextualized(DecisionContext::new(
            player.clone(),
            DecisionSource::Content("legendary".to_owned()),
            "legendary_pass",
            context.state.phase,
            context.state.round,
        ));
        let Ok(answer) = context.ask_seeing(&choice) else {
            return;
        };
        if answer.is_decline() {
            return;
        }
        let planet = PlanetId::new(answer.id);
        // Exhausted first, for the same reason as the end-of-turn window: an ability that asks a
        // follow-up must not be re-offered inside its own resolution.
        exhaust(context.state, player, &planet);
        resolve_pass(context, player, &planet);
    }
}

/// Every planet Maxis Central Control could take, as `{system}|{planet}` options.
///
/// The card names three exclusions -- home, legendary, and anything holding units -- and adds a
/// fourth in "no attachments". There is no adjacency clause, unlike Peace Accords: any planet on
/// the board qualifies, including one another player controls but has left empty.
///
/// Two exclusions the text does not spell out. A space station is not a planet anywhere else in
/// this engine (Space Stations rule 7 keeps it out of the scoring view), and Mecatol Rex starts
/// the game empty, so without a guard this would hand it over on the first pass of round one for
/// nothing, bypassing the custodians token entirely.
fn maxis_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(ti4_model::id::SystemId, PlanetId)> {
    let records = ti4_content::galaxy::all_planets(content, sources);
    let mut found = Vec::new();
    for (system, board) in &state.board {
        let Some(record) = content
            .get(ti4_model::content_types::ContentType::Systems, system.as_str())
            .filter(|record| record.in_sources(sources))
        else {
            continue;
        };
        for planet_id in record.strings("planets") {
            let planet = PlanetId::new(planet_id);
            if board.planet_control.get(&planet) == Some(player) {
                continue;
            }
            let Some(printed) = records.get(planet_id) else {
                continue;
            };
            // `planet_types` rather than an id: Mecatol is `mr` in the base game and `mrte` in
            // Thunder's Edge, and only the second of those is legendary.
            if printed.homeworld_of().is_some()
                || printed.is_legendary()
                || printed.is_space_station()
                || printed.planet_types().contains(&"MR")
            {
                continue;
            }
            if board
                .planet_units
                .get(&planet)
                .is_some_and(|units| !units.is_empty())
            {
                continue;
            }
            if state
                .planet_attachments
                .get(&planet)
                .is_some_and(|attached| !attached.is_empty())
            {
                continue;
            }
            found.push((system.clone(), planet));
        }
    }
    found
}

/// Apply one when-you-pass ability.
#[allow(
    clippy::too_many_lines,
    reason = "a registry: one arm per card, and splitting them apart would hide which card is               which"
)]
fn resolve_pass(
    context: &mut crate::timing::TimingContext<'_>,
    player: &PlayerId,
    planet: &PlanetId,
) {
    match planet.as_str() {
        // "gain control of a non-home, non-legendary planet that contains no units and has no
        // attachments"
        "faunus" => {
            let candidates =
                maxis_candidates(context.state, context.content, context.sources, player);
            if candidates.is_empty() {
                return;
            }
            let mut options: Vec<ChoiceOption> = candidates
                .iter()
                .map(|(system, target)| {
                    ChoiceOption::labelled(
                        format!("{system}|{target}"),
                        "legendary",
                        format!("gain control of {target}"),
                    )
                })
                .collect();
            options.push(ChoiceOption::decline());
            let choice = Choice::new(
                player.clone(),
                "Maxis Central Control: gain control of which planet",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_maxis",
                context.state.phase,
                context.state.round,
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return;
            };
            if answer.is_decline() {
                return;
            }
            let Some((system, target)) = candidates
                .into_iter()
                .find(|(system, target)| format!("{system}|{target}") == answer.id)
            else {
                return;
            };
            context
                .state
                .system_mut(&system)
                .set_control(target.clone(), player.clone());
            let _ = crate::technology::control_gained(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                player,
                &system,
                &target,
            );
            // No `control_gained` here: Maxis Central Control excludes legendary planets, so
            // Thunder's Edge can never arrive through this path.
            if let Some(deck) = crate::exploration::trait_of(context.content, context.sources, &target)
            {
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
                    Some(&target),
                );
            }
        }
        // "place 1 action card from the discard pile faceup on this card; you can purge cards on
        // this card to play them as if they were in your hand"
        //
        // Only the first half happens here. The second is not a window at all -- it is a second
        // place the play paths look for a card, which is why `salvaged` sits beside the hand in
        // `reactions::available_reactions` and `action_cards::available_actions`.
        "garbozia" => {
            let pile = context.state.discarded_action_cards.clone();
            if pile.is_empty() {
                return;
            }
            let mut options: Vec<ChoiceOption> = pile
                .iter()
                .map(|alias| {
                    ChoiceOption::labelled(
                        alias.to_string(),
                        "legendary",
                        format!(
                            "salvage {}",
                            crate::action_cards::name_of(context.content, alias)
                        ),
                    )
                })
                .collect();
            options.push(ChoiceOption::decline());
            let choice = Choice::new(
                player.clone(),
                "Dok 'N Pic's Salvage Yard: salvage which card",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_salvage",
                context.state.phase,
                context.state.round,
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return;
            };
            if answer.is_decline() {
                return;
            }
            let salvaged = ti4_model::id::ActionCardId::new(answer.id);
            let state = &mut *context.state;
            if let Some(at) = state
                .discarded_action_cards
                .iter()
                .position(|held| held == &salvaged)
            {
                state.discarded_action_cards.remove(at);
                state.salvage_yard.push(salvaged);
            }
        }
        // "place 1 ship that matches a unit upgrade technology you own from your reinforcements
        // into a system that contains your ships"
        //
        // The ship *matches* the upgrade, so it is the upgraded plastic: a seat with Carrier II
        // places a Carrier II. Infantry II and Space Dock II are unit upgrades too and are
        // filtered out by not being ships.
        "industrex" => {
            let types = ti4_content::units::catalogue(context.content, context.sources);
            let owned: Vec<ti4_model::id::TechnologyId> = context
                .state
                .player(player)
                .map(|seat| seat.technologies.iter().cloned().collect())
                .unwrap_or_default();
            let ships: Vec<ti4_model::id::UnitTypeId> = owned
                .iter()
                .filter(|technology| {
                    crate::technology::is_unit_upgrade(context.content, technology)
                })
                .filter_map(|technology| {
                    types
                        .values()
                        .find(|kind| {
                            kind.is_ship()
                                && kind.required_technology() == Some(technology.as_str())
                        })
                        .map(|kind| ti4_model::id::UnitTypeId::new(kind.id()))
                })
                .collect();
            let systems: Vec<ti4_model::id::SystemId> = context
                .state
                .board
                .iter()
                .filter(|(_, here)| {
                    here.units_of(player).into_iter().any(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ship)
                    })
                })
                .map(|(system, _)| system.clone())
                .collect();
            if ships.is_empty() || systems.is_empty() {
                return;
            }
            // One question, not two: which ship and where are a single decision, and splitting
            // them would let a seat pick a hull it cannot supply and then be asked where to put
            // nothing.
            let mut options: Vec<ChoiceOption> = Vec::new();
            for ship in &ships {
                if crate::supply::allowed(
                    context.state,
                    context.content,
                    context.sources,
                    player,
                    ship,
                    1,
                ) == 0
                {
                    continue;
                }
                for system in &systems {
                    options.push(ChoiceOption::labelled(
                        format!("{ship}|{system}"),
                        "legendary",
                        format!("place a {ship} in {system}"),
                    ));
                }
            }
            if options.is_empty() {
                return;
            }
            options.push(ChoiceOption::decline());
            let choice = Choice::new(
                player.clone(),
                "Aurex Mechanica: place which ship where",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_aurex",
                context.state.phase,
                context.state.round,
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return;
            };
            if answer.is_decline() {
                return;
            }
            let mut parts = answer.id.splitn(2, '|');
            let (Some(ship), Some(system)) = (parts.next(), parts.next()) else {
                return;
            };
            let system = ti4_model::id::SystemId::new(system);
            context.state.system_mut(&system).units.push(
                ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new(ship),
                    player.clone(),
                ),
            );
        }
        _ => {}
    }
}

/// This seat's own version of a unit type, falling back to the generic one.
fn unit_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base: &str,
) -> Option<ti4_model::id::UnitTypeId> {
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let generic = ti4_content::units::catalogue(content, sources)
        .get(base)
        .map(|unit| unit.id().to_owned());
    ti4_content::units::faction_unit(content, &faction, base, sources)
        .map(|unit| unit.id().to_owned())
        .or(generic)
        .map(ti4_model::id::UnitTypeId::new)
}

/// Ask which of this seat's planets to place on, then put `count` of `base` there.
///
/// Reinforcements are finite, so the number placed is whatever `supply` allows rather than what
/// the card asks for. Both cards that use this say "up to", which is the same thing said aloud.
fn place_on_own_planet(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    base: &str,
    count: usize,
    prompt: &str,
) -> Result<(), IllegalChoice> {
    let Some(type_id) = unit_of(state, content, sources, player, base) else {
        return Ok(());
    };
    let placeable = crate::supply::allowed(state, content, sources, player, &type_id, count);
    let options: Vec<ChoiceOption> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| {
            ChoiceOption::labelled(
                format!("{system}|{planet}"),
                "legendary",
                format!("place on {planet}"),
            )
        })
        .collect();
    if placeable == 0 || options.is_empty() {
        return Ok(());
    }
    let choice = Choice::new(player.clone(), prompt, options).contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::Content("legendary".to_owned()),
        "legendary_place",
        state.phase,
        state.round,
    ));
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    let mut parts = answer.id.split('|');
    let (Some(system), Some(planet), None) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(());
    };
    let held = state
        .system_mut(&ti4_model::id::SystemId::new(system))
        .planet_units
        .entry(PlanetId::new(planet))
        .or_default();
    for _ in 0..placeable {
        held.push(ti4_model::units::Unit::new(type_id.clone(), player.clone()));
    }
    Ok(())
}

/// Every exhausted card The Acropolis could ready, as `kind|id` options.
///
/// Strategy cards are excluded by the printed text; `excluding` is The Acropolis' own card, which
/// the caller exhausted before resolving and which "another" rules out. A planet card belongs to
/// whoever controls the planet, so the exhausted-planet set is intersected with this seat's
/// holdings rather than read whole.
fn readyable(state: &GameState, player: &PlayerId, excluding: &PlanetId) -> Vec<ChoiceOption> {
    let mut options: Vec<ChoiceOption> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet)
        .filter(|planet| state.exhausted_planets.contains(planet.as_str()))
        .map(|planet| {
            ChoiceOption::labelled(
                format!("planet|{planet}"),
                "legendary",
                format!("ready {planet}"),
            )
        })
        .collect();
    let Some(seat) = state.player(player) else {
        return options;
    };
    for technology in &seat.exhausted_technologies {
        options.push(ChoiceOption::labelled(
            format!("technology|{technology}"),
            "legendary",
            format!("ready {technology}"),
        ));
    }
    for relic in &seat.exhausted_relics {
        options.push(ChoiceOption::labelled(
            format!("relic|{relic}"),
            "legendary",
            format!("ready {relic}"),
        ));
    }
    for card in seat.exhausted_legendary.iter().filter(|id| *id != excluding) {
        options.push(ChoiceOption::labelled(
            format!("legendary|{card}"),
            "legendary",
            format!("ready the {card} ability"),
        ));
    }
    for (leader, _) in seat
        .leaders
        .iter()
        .filter(|(_, status)| **status == ti4_model::state::LeaderStatus::Exhausted)
    {
        options.push(ChoiceOption::labelled(
            format!("leader|{leader}"),
            "legendary",
            format!("ready {leader}"),
        ));
    }
    options
}

/// Apply one ability. A planet with no arm here is an honest gap, not a silent one.
#[allow(
    clippy::too_many_lines,
    reason = "a registry: one arm per card, and splitting them apart would hide which card is               which"
)]
fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    planet: &PlanetId,
) -> Result<(), IllegalChoice> {
    match planet.as_str() {
        // "gain 2 trade goods or convert all of your commodities into trade goods"
        "mallice" | "lockedmallice" => {
            let commodities = state.player(player).map_or(0, |seat| seat.commodities);
            let options = vec![
                ChoiceOption::labelled("goods", "legendary", "gain 2 trade goods"),
                ChoiceOption::labelled(
                    "convert",
                    "legendary",
                    format!("convert {commodities} commodities to trade goods"),
                ),
            ];
            let choice = Choice::new(player.clone(), "Exterrix Headquarters", options)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("legendary".to_owned()),
                    "legendary_exterrix",
                    state.phase,
                    state.round,
                ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if let Some(seat) = state.player_mut(player) {
                if answer.id == "convert" {
                    seat.trade_goods += seat.commodities;
                    seat.commodities = 0;
                } else {
                    seat.trade_goods += 2;
                }
            }
        }
        // "place up to 2 infantry from your reinforcements on any planet you control"
        "primor" => place_on_own_planet(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            "infantry",
            2,
            "The Atrament: place infantry where",
        )?,
        // "place 1 mech from your reinforcements on any planet you control, or draw 1 action card"
        "hopesend" => {
            let options = vec![
                ChoiceOption::labelled("mech", "legendary", "place 1 mech"),
                ChoiceOption::labelled("card", "legendary", "draw 1 action card"),
            ];
            let choice = Choice::new(player.clone(), "Imperial Arms Vault", options)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("legendary".to_owned()),
                    "legendary_arms_vault",
                    state.phase,
                    state.round,
                ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            if answer.id == "card" {
                crate::action_cards::draw(state, content, table, player, 1)?;
            } else {
                place_on_own_planet(
                    state,
                    content,
                    sources,
                    galaxy,
                    table,
                    player,
                    "mech",
                    1,
                    "Imperial Arms Vault: place the mech where",
                )?;
            }
        }
        // "place up to 2 fighters in any system that contains 1 or more of your ships" -- a
        // system rather than a planet, and only one this seat is already in.
        "mirage" => {
            let Some(type_id) = unit_of(state, content, sources, player, "fighter") else {
                return Ok(());
            };
            let placeable = crate::supply::allowed(state, content, sources, player, &type_id, 2);
            let types = ti4_content::units::catalogue(content, sources);
            let options: Vec<ChoiceOption> = state
                .board
                .iter()
                .filter(|(_, here)| {
                    here.units_of(player).into_iter().any(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ship)
                    })
                })
                .map(|(system, _)| {
                    ChoiceOption::labelled(
                        system.to_string(),
                        "legendary",
                        format!("place fighters in {system}"),
                    )
                })
                .collect();
            if placeable == 0 || options.is_empty() {
                return Ok(());
            }
            let choice = Choice::new(
                player.clone(),
                "Mirage Flight Academy: place fighters where",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_place",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            let system = ti4_model::id::SystemId::new(answer.id);
            let units = &mut state.system_mut(&system).units;
            for _ in 0..placeable {
                units.push(ti4_model::units::Unit::new(type_id.clone(), player.clone()));
            }
        }
        // "ready another component that isn't a strategy card"
        //
        // "Another" excludes The Acropolis itself, which the caller has already exhausted. Every
        // other exhaustible card this seat owns is fair game: planets it controls, technologies,
        // relics, leaders, and the other legendary ability cards. Strategy cards are named out.
        "emelpar" => {
            let options = readyable(state, player, planet);
            if options.is_empty() {
                return Ok(());
            }
            let choice = Choice::new(player.clone(), "The Acropolis: ready what", options)
                .contextualized(DecisionContext::new(
                    player.clone(),
                    DecisionSource::Content("legendary".to_owned()),
                    "legendary_acropolis",
                    state.phase,
                    state.round,
                ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            let mut parts = answer.id.splitn(2, '|');
            match (parts.next(), parts.next()) {
                (Some("planet"), Some(id)) => state.ready_planet(&PlanetId::new(id)),
                (Some("technology"), Some(id)) => {
                    if let Some(seat) = state.player_mut(player) {
                        seat.exhausted_technologies
                            .remove(&ti4_model::id::TechnologyId::new(id));
                    }
                }
                (Some("relic"), Some(id)) => {
                    if let Some(seat) = state.player_mut(player) {
                        seat.exhausted_relics
                            .remove(&ti4_model::id::RelicId::new(id));
                    }
                }
                (Some("legendary"), Some(id)) => {
                    if let Some(seat) = state.player_mut(player) {
                        seat.exhausted_legendary.remove(&PlanetId::new(id));
                    }
                }
                (Some("leader"), Some(id)) => {
                    crate::leaders::ready(state, player, &ti4_model::id::LeaderId::new(id));
                }
                _ => {}
            }
        }
        // "perform another action"
        //
        // The same flag Master Plan and the Minister of War set, read by `advance_turn` to keep
        // the turn with this seat. The card exhausts, so it is one extra action a round, not a
        // loop -- and the flag cannot stack, so exhausting it during an action Master Plan
        // already granted does not bank a third.
        "thundersedge" => {
            state
                .transient_flags
                .set(ti4_model::state::TransientFlags::ADDITIONAL_ACTION);
        }
        // "discard 1 secret objective to draw 1 secret objective"
        "mrte" => {
            let held: Vec<ti4_model::id::SecretObjectiveId> = state
                .player(player)
                .map(|seat| seat.secret_objectives.clone())
                .unwrap_or_default();
            if held.is_empty() {
                return Ok(());
            }
            let options: Vec<ChoiceOption> = held
                .iter()
                .map(|secret| {
                    ChoiceOption::labelled(secret.to_string(), "legendary", "discard this secret")
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "The Galactic Council: discard which secret",
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content("legendary".to_owned()),
                "legendary_galactic_council",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            let discarded = ti4_model::id::SecretObjectiveId::new(answer.id);
            if let Some(seat) = state.player_mut(player) {
                seat.secret_objectives.retain(|s| s != &discarded);
            }
            // Back to the deck before the draw, so a one-card deck still yields a card and the
            // swap cannot silently lose the discarded objective.
            state.secret_deck.push(discarded);
            crate::secrets::draw(state, content, table, player)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    /// A seat holding Mallice, with the commodities the ability can convert.
    fn holding_mallice(commodities: i32) -> (GameState, PlayerId, PlanetId) {
        let player = PlayerId::new("a");
        let planet = PlanetId::new("mallice");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let system = ti4_model::id::SystemId::new("82b");
        state.board.entry(system.clone()).or_default();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        if let Some(seat) = state.player_mut(&player) {
            seat.commodities = commodities;
            seat.trade_goods = 0;
        }
        (state, player, planet)
    }


    /// A seat holding `planet`, with a board entry for `system`.
    fn holding(planet: &str, system: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        let system = ti4_model::id::SystemId::new(system);
        state.board.entry(system.clone()).or_default();
        state
            .system_mut(&system)
            .set_control(PlanetId::new(planet), player.clone());
        (state, player)
    }

    /// Run the pass window with a real timing context.
    fn passing(
        state: &mut GameState,
        table: &mut Table,
        player: &PlayerId,
    ) {
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = crate::timing::TimingContext {
            state,
            content: content(),
            sources: POK,
            table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        pass(&mut context, player);
    }

    #[test]
    fn maxis_central_control_takes_an_empty_planet_when_you_pass() {
        let (mut state, player) = holding("faunus", "97");
        // System 20 is Lodor, an ordinary two-planet system; put it on the board empty.
        let elsewhere = ti4_model::id::SystemId::new("20");
        state.board.entry(elsewhere.clone()).or_default();
        let target = maxis_candidates(&state, content(), POK, &player)
            .into_iter()
            .find(|(system, _)| system == &elsewhere)
            .map(|(_, planet)| planet)
            .expect("an empty system on the board offers its planets");

        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "faunus".to_owned(),
            format!("20|{target}"),
            "decline".to_owned(),
        ])));
        passing(&mut state, &mut table, &player);

        assert_eq!(
            state.system_state(&elsewhere).planet_control.get(&target),
            Some(&player),
            "the planet changed hands"
        );
    }

    #[test]
    fn maxis_central_control_will_not_take_a_planet_that_has_units_on_it() {
        let (mut state, player) = holding("faunus", "97");
        let elsewhere = ti4_model::id::SystemId::new("20");
        state.board.entry(elsewhere.clone()).or_default();
        let target = maxis_candidates(&state, content(), POK, &player)
            .into_iter()
            .find(|(system, _)| system == &elsewhere)
            .map(|(_, planet)| planet)
            .expect("offered while empty");

        // Anybody's units, not only a rival's: the card says "contains no units".
        crate::fixtures::put_on_planet(
            &mut state,
            &elsewhere,
            &target,
            "infantry",
            &PlayerId::new("b"),
            1,
        );

        let after = maxis_candidates(&state, content(), POK, &player);
        assert!(
            !after.iter().any(|(_, planet)| planet == &target),
            "an occupied planet is not takeable: {after:?}"
        );
    }

    #[test]
    fn maxis_central_control_offers_no_homeworld_and_no_legendary_planet() {
        let (state, player) = holding("faunus", "97");
        let mut board = state;
        for system in ["1", "45", "82b"] {
            board
                .board
                .entry(ti4_model::id::SystemId::new(system))
                .or_default();
        }
        let records = ti4_content::galaxy::all_planets(content(), POK);
        for (_, planet) in maxis_candidates(&board, content(), POK, &player) {
            let printed = records
                .get(planet.as_str())
                .expect("a candidate the corpus knows");
            assert!(
                printed.homeworld_of().is_none() && !printed.is_legendary(),
                "{planet} is a homeworld or legendary and was offered anyway"
            );
        }
    }

    #[test]
    fn the_salvage_yard_takes_a_card_off_the_discard_pile_and_holds_it() {
        let (mut state, player) = holding("garbozia", "97");
        let card = ti4_model::id::ActionCardId::new("fs1");
        state.discarded_action_cards = vec![card.clone()];

        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "garbozia", "fs1", "decline",
        ])));
        passing(&mut state, &mut table, &player);

        assert_eq!(state.salvage_yard, vec![card.clone()]);
        assert!(
            state.discarded_action_cards.is_empty(),
            "it left the pile rather than being copied out of it"
        );
        assert_eq!(
            salvaged(&state, &player),
            vec![card.clone()],
            "the holder may play it as if it were in hand"
        );

        // The zone is on the planet card, so it answers for whoever holds the planet.
        assert!(
            salvaged(&state, &PlayerId::new("b")).is_empty(),
            "and for nobody else"
        );

        // Purging is what playing it costs, and it does not fall back into the pile.
        assert!(purge_salvaged(&mut state, &player, &card));
        assert!(state.salvage_yard.is_empty());
        assert!(state.discarded_action_cards.is_empty());
        assert!(
            !purge_salvaged(&mut state, &player, &card),
            "and cannot be purged twice"
        );
    }

    #[test]
    fn aurex_mechanica_places_the_upgraded_hull_you_own() {
        let (mut state, player) = holding("industrex", "97");
        let here = ti4_model::id::SystemId::new("45");
        state.board.entry(here.clone()).or_default();
        state.system_mut(&here).units.push(ti4_model::units::Unit::new(
            ti4_model::id::UnitTypeId::new("carrier"),
            player.clone(),
        ));
        if let Some(seat) = state.player_mut(&player) {
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new("cv2"));
        }

        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "industrex",
            "carrier2|45",
            "decline",
        ])));
        passing(&mut state, &mut table, &player);

        let placed = state.system_state(&here).units.clone();
        assert!(
            placed.iter().any(|unit| unit.type_id.as_str() == "carrier2"),
            "the Carrier II the upgrade unlocks, not a base carrier: {placed:?}"
        );
    }

    #[test]
    fn a_song_like_marrow_moves_a_point_with_control_of_styx() {
        let styx = PlanetId::new("styx");
        let system = ti4_model::id::SystemId::new("fracture4");
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.board.entry(system.clone()).or_default();

        // Nobody holds it: settling is a no-op, and repeating it stays one.
        settle_control_points(&mut state);
        settle_control_points(&mut state);
        assert_eq!(state.player(&a).unwrap().victory_points, 0);

        state
            .system_mut(&system)
            .set_control(styx.clone(), a.clone());
        settle_control_points(&mut state);
        assert_eq!(
            state.player(&a).unwrap().victory_points,
            1,
            "gaining the card gains the point"
        );
        settle_control_points(&mut state);
        assert_eq!(
            state.player(&a).unwrap().victory_points,
            1,
            "settling again does not pay twice"
        );

        state
            .system_mut(&system)
            .set_control(styx.clone(), b.clone());
        settle_control_points(&mut state);
        assert_eq!(
            (
                state.player(&a).unwrap().victory_points,
                state.player(&b).unwrap().victory_points
            ),
            (0, 1),
            "taking it off somebody moves the point rather than minting one"
        );
    }

    #[test]
    fn jupiter_brain_retains_the_turn_for_another_action() {
        let (mut state, player) = holding("thundersedge", "117");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "thundersedge",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        assert!(
            state
                .transient_flags
                .has(ti4_model::state::TransientFlags::ADDITIONAL_ACTION),
            "the same flag Master Plan sets, which `advance_turn` reads to keep the turn here"
        );
    }

    #[test]
    fn jupiter_brain_hands_its_breakthrough_to_a_seat_that_takes_the_planet() {
        let (mut state, player) = holding("thundersedge", "117");
        let planet = PlanetId::new("thundersedge");
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new("letnev");
        assert!(
            state.player(&player).unwrap().breakthrough.is_none(),
            "the fixture starts without one"
        );

        control_gained(&mut state, content(), POK, &player, &planet);
        let gained = state.player(&player).unwrap().breakthrough.clone();
        assert!(
            gained.is_some(),
            "an invader who never funded the expedition still gains their breakthrough"
        );

        // "if you do not already have it" -- a second gain does not swap the one they hold.
        control_gained(&mut state, content(), POK, &player, &planet);
        assert_eq!(state.player(&player).unwrap().breakthrough, gained);
    }

    #[test]
    fn the_acropolis_readies_another_card() {
        let (mut state, player) = holding("emelpar", "99");
        let other = PlanetId::new("primor");
        let elsewhere = ti4_model::id::SystemId::new("45");
        state.board.entry(elsewhere.clone()).or_default();
        state
            .system_mut(&elsewhere)
            .set_control(other.clone(), player.clone());
        state.exhaust_planet(other.clone());

        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "emelpar",
            "planet|primor",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        assert!(
            !state.exhausted_planets.contains(&other),
            "the chosen planet card readied"
        );
    }

    #[test]
    fn the_acropolis_offers_neither_a_strategy_card_nor_itself() {
        let (mut state, player) = holding("emelpar", "99");
        let emelpar = PlanetId::new("emelpar");
        let technology = ti4_model::id::TechnologyId::new("sarweenTools");
        if let Some(seat) = state.player_mut(&player) {
            seat.exhausted_strategy_cards
                .insert(ti4_model::id::StrategyCardId::new("warfare"));
            seat.exhausted_technologies.insert(technology.clone());
            // As `end_turn` leaves it: the ability card is spent before it resolves.
            seat.exhausted_legendary.insert(emelpar.clone());
        }

        let offered: Vec<String> = readyable(&state, &player, &emelpar)
            .into_iter()
            .map(|option| option.id)
            .collect();

        assert_eq!(
            offered,
            vec![format!("technology|{technology}")],
            "the strategy card is named out by the card, and \"another\" excludes The Acropolis"
        );
    }

    #[test]
    fn the_atrament_places_two_infantry_on_a_planet_you_control() {
        let (mut state, player) = holding("primor", "45");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "primor",
            "45|primor",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let placed = state
            .system_state(&ti4_model::id::SystemId::new("45"))
            .planet_units
            .get(&PlanetId::new("primor"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(placed.len(), 2, "two infantry: {placed:?}");
        assert!(placed.iter().all(|unit| unit.owner == player));
    }

    #[test]
    fn the_imperial_arms_vault_draws_a_card_when_that_branch_is_taken() {
        let (mut state, player) = holding("hopesend", "45");
        let before = state.player(&player).unwrap().action_cards.len();
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "hopesend", "card", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        assert_eq!(
            state.player(&player).unwrap().action_cards.len(),
            before + 1,
            "the other branch of the same ability"
        );
    }

    #[test]
    fn the_galactic_council_swaps_one_secret_for_another() {
        let (mut state, player) = holding("mrte", "112");
        let discarded = ti4_model::id::SecretObjectiveId::new("mrm");
        if let Some(seat) = state.player_mut(&player) {
            seat.secret_objectives = vec![discarded.clone()];
        }
        state.secret_deck = vec![ti4_model::id::SecretObjectiveId::new("baf")];
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mrte", "mrm", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let held = state.player(&player).unwrap().secret_objectives.clone();
        assert_eq!(held.len(), 1, "still one secret, a different one: {held:?}");
        assert!(
            !held.contains(&discarded),
            "the discarded objective is gone from the hand"
        );
    }

    #[test]
    fn an_ability_is_offered_once_a_round() {
        // Exhausted before it resolves, so an ability that asks a follow-up question cannot be
        // re-offered inside its own resolution -- and cannot be used twice in one turn.
        let (mut state, player) = holding("primor", "45");
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "primor",
            "45|primor",
            "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();
        assert!(!available(&state, &player, &PlanetId::new("primor")));

        // The status phase readies it, beside technologies and relics.
        for seat in &mut state.players {
            seat.exhausted_legendary.clear();
        }
        assert!(available(&state, &player, &PlanetId::new("primor")));
    }

    #[test]
    fn exterrix_headquarters_gains_two_trade_goods() {
        let (mut state, player, planet) = holding_mallice(4);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mallice", "goods", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 2, "two trade goods");
        assert_eq!(seat.commodities, 4, "commodities untouched on this branch");
        assert!(
            !available(&state, &player, &planet),
            "the ability card is spent for the round"
        );
    }

    #[test]
    fn exterrix_headquarters_converts_every_commodity() {
        let (mut state, player, _planet) = holding_mallice(5);
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new([
            "mallice", "convert", "decline",
        ])));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();

        let seat = state.player(&player).unwrap();
        assert_eq!(seat.trade_goods, 5, "all five converted");
        assert_eq!(seat.commodities, 0, "and none left behind");
    }

    #[test]
    fn a_seat_that_does_not_hold_the_planet_is_offered_nothing() {
        // The ability belongs to the planet, so losing it takes the ability with it.
        let (mut state, player, _planet) = holding_mallice(3);
        let system = ti4_model::id::SystemId::new("82b");
        state.system_mut(&system).planet_control.clear();
        let mut table = Table::with_default(Box::new(crate::choice::Scripted::new(
            Vec::<String>::new(),
        )));
        end_turn(&mut state, content(), POK, None, &mut table, &player).unwrap();
        assert_eq!(
            state.player(&player).unwrap().trade_goods,
            0,
            "nothing was offered, so nothing was gained"
        );
    }
}
