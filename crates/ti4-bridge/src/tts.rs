//! Translation from resolved Rust timing events to physical TTS commands.
//!
//! An event is translated, explicitly classified as having no physical representation, or
//! refused. Silently dropping a new event type would let the visible table drift from Rust.

use std::collections::{BTreeMap, BTreeSet};

use ti4_engine::event::Event;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::{UnitChange, UnitClass, UnitLocation};
use crate::{commands, wire::Command};

/// A deterministic player-to-table-seat map.
#[derive(Debug, Clone, Default)]
pub struct TtsCommands {
    colours: BTreeMap<PlayerId, String>,
    planet_indices: BTreeMap<(ti4_model::id::SystemId, ti4_model::id::PlanetId), u32>,
    no_physical_effect: BTreeSet<String>,
}

impl TtsCommands {
    /// Build a translator from the exact seats imported from TTS.
    pub fn new(colours: impl IntoIterator<Item = (PlayerId, String)>) -> Self {
        Self {
            colours: colours.into_iter().collect(),
            planet_indices: BTreeMap::new(),
            no_physical_effect: BTreeSet::new(),
        }
    }

    /// Use a dedicated TTS plastic colour for the unseated neutral force.
    ///
    /// This does not create a Rust player or seat; it only selects the physical reinforcement
    /// inventory used by placement/removal commands.
    #[must_use]
    pub fn with_neutral_colour(mut self, colour: impl Into<String>) -> Self {
        self.colours.insert(
            PlayerId::new(ti4_engine::neutral_units::NEUTRAL),
            colour.into(),
        );
        self
    }

    /// Register the mod's one-based index for a planet in a system.
    #[must_use]
    pub fn with_planet_index(
        mut self,
        system: ti4_model::id::SystemId,
        planet: ti4_model::id::PlanetId,
        index: u32,
    ) -> Self {
        assert!(index > 0, "TTS planet indexes are one-based");
        self.planet_indices.insert((system, planet), index);
        self
    }

    /// Declare an event type intentionally invisible on the physical table.
    #[must_use]
    pub fn with_no_physical_effect(mut self, event_type: impl Into<String>) -> Self {
        self.no_physical_effect.insert(event_type.into());
        self
    }

    /// Translate a complete authoritative state transition before any command is queued.
    pub fn transition(
        &self,
        before: &GameState,
        after: &GameState,
        content: &ti4_content::ContentStore,
        sources: SourceSet,
    ) -> Result<Vec<Command>, TranslationError> {
        let units = crate::unit_changes(before, after);
        let groups = [
            self.scalar_changes(before, after)?,
            self.unit_changes(&units)?,
            self.strategy_changes(before, after, content)?,
            self.planet_changes(before, after, content, sources)?,
            self.card_changes(before, after, content, sources)?,
            self.attachment_changes(before, after, content, sources)?,
        ];
        Ok(groups.into_iter().flatten().collect())
    }

    /// Translate one final resolved event.
    ///
    /// Cancelled events produce no command: their journal entry proves the window ran, but the
    /// cancelled transition itself did not mutate authoritative state.
    ///
    /// # Errors
    /// Refuses invalid payloads, unknown seats or units, and every unclassified event type.
    pub fn event(&self, event: &Event) -> Result<Vec<Command>, TranslationError> {
        if event.cancelled || self.no_physical_effect.contains(&event.event_type) {
            return Ok(Vec::new());
        }
        if !matches!(
            event.event_type.as_str(),
            "SUSTAIN_DAMAGE_USED" | "SHIP_DESTROYED"
        ) {
            return Err(TranslationError::UnclassifiedEvent(
                event.event_type.clone(),
            ));
        }

        let player = text(event, "player")?;
        let colour = self
            .colours
            .get(&PlayerId::new(player))
            .ok_or_else(|| TranslationError::UnknownPlayer(player.to_owned()))?;
        let system = text(event, "system")?;
        let unit = text(event, "unit")?;

        let command = match event.event_type.as_str() {
            "SUSTAIN_DAMAGE_USED" => commands::damage_units(colour, system, None, &[(unit, 1)]),
            "SHIP_DESTROYED" => commands::remove_units(
                colour,
                system,
                None,
                Some(boolean(event, "damaged")?),
                &[(unit, 1)],
            ),
            _ => unreachable!("classified above"),
        }
        .map_err(|error| TranslationError::UnknownUnit(error.0))?;
        Ok(vec![command])
    }

    /// Translate a structural damage-state change that no typed event accounted for.
    ///
    /// Additions and removals remain for semantic pairing (move versus produce/destroy) and are
    /// refused here. Treating either interpretation as interchangeable can partially apply a move.
    pub fn unit_change(&self, change: &UnitChange) -> Result<Vec<Command>, TranslationError> {
        let UnitChange::Damage {
            location,
            unit,
            damaged,
            count,
        } = change
        else {
            return Err(TranslationError::UnpairedUnitChange);
        };
        if unit.galvanized {
            return Err(TranslationError::GalvanizedUnit);
        }
        let colour = self
            .colours
            .get(&unit.owner)
            .ok_or_else(|| TranslationError::UnknownPlayer(unit.owner.to_string()))?;
        let planet = self.planet_index(location)?;
        let count = i32::try_from(*count).map_err(|_| TranslationError::CountOverflow(*count))?;
        let units = [(unit.unit_type.as_str(), count)];
        let command = if *damaged {
            commands::damage_units(colour, location.system.as_str(), planet, &units)
        } else {
            commands::repair_units(colour, location.system.as_str(), planet, &units)
        }
        .map_err(|error| TranslationError::UnknownUnit(error.0))?;
        Ok(vec![command])
    }

    /// Translate a complete state transition without guessing move endpoints.
    ///
    /// Add/remove pairs of the same physical unit become relocations. Many-to-one and
    /// one-to-many transport are deterministic; many-to-many transitions are refused because
    /// the two snapshots do not reveal which source supplied which destination.
    pub fn unit_changes(&self, changes: &[UnitChange]) -> Result<Vec<Command>, TranslationError> {
        type Key = (UnitClass, bool);
        let mut commands_out = Vec::new();
        let mut adds = BTreeMap::<Key, BTreeMap<UnitLocation, usize>>::new();
        let mut removes = BTreeMap::<Key, BTreeMap<UnitLocation, usize>>::new();

        for change in changes {
            match change {
                UnitChange::Damage { .. } => commands_out.extend(self.unit_change(change)?),
                UnitChange::Add {
                    location,
                    unit,
                    damaged,
                    count,
                } => {
                    *adds
                        .entry((unit.clone(), *damaged))
                        .or_default()
                        .entry(location.clone())
                        .or_default() += count;
                }
                UnitChange::Remove {
                    location,
                    unit,
                    damaged,
                    count,
                } => {
                    *removes
                        .entry((unit.clone(), *damaged))
                        .or_default()
                        .entry(location.clone())
                        .or_default() += count;
                }
            }
        }

        let keys: BTreeSet<_> = adds.keys().chain(removes.keys()).cloned().collect();
        for (unit, damaged) in keys {
            if unit.galvanized {
                return Err(TranslationError::GalvanizedUnit);
            }
            let mut destinations = adds.remove(&(unit.clone(), damaged)).unwrap_or_default();
            let mut sources = removes.remove(&(unit.clone(), damaged)).unwrap_or_default();
            if destinations.len() > 1 && sources.len() > 1 {
                return Err(TranslationError::AmbiguousUnitChange {
                    unit: unit.unit_type.to_string(),
                    sources: sources.len(),
                    destinations: destinations.len(),
                });
            }
            let colour = self.colour(&unit)?;

            while let (Some(source), Some(destination)) = (
                positive_location(&sources),
                positive_location(&destinations),
            ) {
                let count = sources[&source].min(destinations[&destination]);
                commands_out.push(
                    commands::relocate_units(
                        colour,
                        source.system.as_str(),
                        self.planet_index(&source)?,
                        destination.system.as_str(),
                        self.planet_index(&destination)?,
                        &[(unit.unit_type.as_str(), signed(count)?)],
                    )
                    .map_err(|error| TranslationError::UnknownUnit(error.0))?,
                );
                subtract(&mut sources, &source, count);
                subtract(&mut destinations, &destination, count);
            }
            for (location, count) in destinations {
                if count == 0 {
                    continue;
                }
                commands_out.push(
                    commands::place_units(
                        colour,
                        location.system.as_str(),
                        self.planet_index(&location)?,
                        &[(unit.unit_type.as_str(), signed(count)?)],
                    )
                    .map_err(|error| TranslationError::UnknownUnit(error.0))?,
                );
            }
            for (location, count) in sources {
                if count == 0 {
                    continue;
                }
                commands_out.push(
                    commands::remove_units(
                        colour,
                        location.system.as_str(),
                        self.planet_index(&location)?,
                        Some(damaged),
                        &[(unit.unit_type.as_str(), signed(count)?)],
                    )
                    .map_err(|error| TranslationError::UnknownUnit(error.0))?,
                );
            }
        }
        Ok(commands_out)
    }

    /// Translate strategy-card ownership and face changes between authoritative states.
    pub fn strategy_changes(
        &self,
        before: &GameState,
        after: &GameState,
        content: &ti4_content::ContentStore,
    ) -> Result<Vec<Command>, TranslationError> {
        let held_before: usize = before
            .players
            .iter()
            .map(|player| player.strategy_cards.len())
            .sum();
        let held_after: usize = after
            .players
            .iter()
            .map(|player| player.strategy_cards.len())
            .sum();
        if held_before > 0 && held_after == 0 {
            return Ok(vec![commands::return_strategy_cards()]);
        }
        let mut out = Vec::new();
        let mut drafter = None;
        for old in &before.players {
            let Some(new) = after.player(&old.id) else {
                continue;
            };
            let colour = self
                .colours
                .get(&old.id)
                .ok_or_else(|| TranslationError::UnknownPlayer(old.id.to_string()))?;
            for card in &new.strategy_cards {
                if !old.strategy_cards.contains(card) {
                    drafter = Some(old.id.clone());
                    let name = strategy_name(content, card.as_str())?;
                    out.push(commands::card(colour, &name, "area"));
                    if before.strategy_card_goods.get(card).copied().unwrap_or(0) > 0 {
                        out.push(commands::clear_card_goods(&name));
                    }
                }
            }
            for card in old
                .strategy_cards
                .iter()
                .filter(|card| new.strategy_cards.contains(card))
            {
                let was_down = old.exhausted_strategy_cards.contains(card);
                let is_down = new.exhausted_strategy_cards.contains(card);
                if was_down != is_down {
                    out.push(commands::flip(
                        colour,
                        &strategy_name(content, card.as_str())?,
                        !is_down,
                    ));
                }
            }
        }
        if before.phase == ti4_model::state::Phase::Strategy {
            if let Some(drafter) = drafter {
                let start = after
                    .seating_order
                    .iter()
                    .position(|player| player == &drafter)
                    .unwrap_or(0);
                let next = (1..=after.seating_order.len())
                    .map(|offset| {
                        &after.seating_order[(start + offset) % after.seating_order.len()]
                    })
                    .find(|player| {
                        after.player(player).is_some_and(|seat| {
                            seat.strategy_cards.len() < after.strategy_cards_per_player
                        })
                    })
                    .cloned()
                    .or_else(|| after.initiative_order().first().cloned());
                if let Some(next) = next {
                    let colour = self
                        .colours
                        .get(&next)
                        .ok_or_else(|| TranslationError::UnknownPlayer(next.to_string()))?;
                    out.push(commands::end_turn(colour));
                }
            }
        }
        Ok(out)
    }

    /// Translate visible per-seat counters and the speaker marker.
    pub fn scalar_changes(
        &self,
        before: &GameState,
        after: &GameState,
    ) -> Result<Vec<Command>, TranslationError> {
        let mut out = Vec::new();
        let mut activation_spends = BTreeMap::<PlayerId, i32>::new();
        let systems: BTreeSet<_> = before
            .board
            .keys()
            .chain(after.board.keys())
            .cloned()
            .collect();
        for system in systems {
            let old = before.system_state(&system);
            let new = after.system_state(&system);
            for player in new.command_tokens.difference(&old.command_tokens) {
                let colour = self
                    .colours
                    .get(player)
                    .ok_or_else(|| TranslationError::UnknownPlayer(player.to_string()))?;
                if after.active.as_ref() == Some(player)
                    && after.active_system.as_ref() == Some(&system)
                {
                    out.push(commands::activate(colour, system.as_str()));
                    *activation_spends.entry(player.clone()).or_default() += 1;
                } else {
                    out.push(commands::place_command_token(colour, system.as_str()));
                }
            }
            for player in old.command_tokens.difference(&new.command_tokens) {
                let colour = self
                    .colours
                    .get(player)
                    .ok_or_else(|| TranslationError::UnknownPlayer(player.to_string()))?;
                out.push(commands::return_token(colour, system.as_str()));
            }
        }
        if before.speaker != after.speaker {
            let colour = self
                .colours
                .get(&after.speaker)
                .ok_or_else(|| TranslationError::UnknownPlayer(after.speaker.to_string()))?;
            out.push(commands::speaker(colour));
        }
        if before.active != after.active {
            if let Some(active) = &after.active {
                let colour = self
                    .colours
                    .get(active)
                    .ok_or_else(|| TranslationError::UnknownPlayer(active.to_string()))?;
                out.push(commands::end_turn(colour));
            }
        }
        for old in &before.players {
            let Some(new) = after.player(&old.id) else {
                continue;
            };
            let colour = self
                .colours
                .get(&old.id)
                .ok_or_else(|| TranslationError::UnknownPlayer(old.id.to_string()))?;
            if old.trade_goods != new.trade_goods {
                out.push(commands::token(
                    colour,
                    "tradegood",
                    new.trade_goods - old.trade_goods,
                ));
            }
            if old.commodities != new.commodities {
                out.push(commands::token(
                    colour,
                    "commodity",
                    new.commodities - old.commodities,
                ));
            }
            for (pool, old_count, new_count, already_spent) in [
                (
                    "tactics",
                    old.tactic_tokens,
                    new.tactic_tokens,
                    activation_spends.get(&old.id).copied().unwrap_or(0),
                ),
                ("fleet", old.fleet_tokens, new.fleet_tokens, 0),
                ("strategy", old.strategic_tokens, new.strategic_tokens, 0),
            ] {
                let delta = new_count - old_count + already_spent;
                for _ in 0..delta.max(0) {
                    out.push(commands::gain_token(colour, pool));
                }
                for _ in 0..(-delta).max(0) {
                    out.push(commands::spend_token(colour, pool));
                }
            }
            let scored_named_objective =
                after
                    .scored_objectives
                    .get(&old.id)
                    .is_some_and(|objectives| {
                        objectives.iter().any(|objective| {
                            !before
                                .scored_objectives
                                .get(&old.id)
                                .is_some_and(|old| old.contains(objective))
                        })
                    });
            if old.victory_points != new.victory_points && !scored_named_objective {
                out.push(commands::score_total(colour, new.victory_points));
            }
        }
        Ok(out)
    }

    /// Translate planet-card ownership and exhausted/ready state.
    pub fn planet_changes(
        &self,
        before: &GameState,
        after: &GameState,
        content: &ti4_content::ContentStore,
        sources: SourceSet,
    ) -> Result<Vec<Command>, TranslationError> {
        let mut out = Vec::new();
        let exhausted: BTreeSet<_> = before
            .exhausted_planets
            .symmetric_difference(&after.exhausted_planets)
            .cloned()
            .collect();
        for planet in exhausted {
            let owner = after
                .board
                .values()
                .find_map(|system| system.planet_control.get(&planet))
                .or_else(|| {
                    before
                        .board
                        .values()
                        .find_map(|system| system.planet_control.get(&planet))
                })
                .ok_or_else(|| TranslationError::PlanetWithoutController(planet.to_string()))?;
            let colour = self
                .colours
                .get(owner)
                .ok_or_else(|| TranslationError::UnknownPlayer(owner.to_string()))?;
            out.push(commands::flip(
                colour,
                &planet_name(content, planet.as_str(), sources)?,
                !after.exhausted_planets.contains(&planet),
            ));
        }

        let systems: BTreeSet<_> = before
            .board
            .keys()
            .chain(after.board.keys())
            .cloned()
            .collect();
        for system_id in systems {
            let old = before.system_state(&system_id);
            let new = after.system_state(&system_id);
            for (planet, owner) in &new.planet_control {
                if old.planet_control.get(planet) == Some(owner) {
                    continue;
                }
                let colour = self
                    .colours
                    .get(owner)
                    .ok_or_else(|| TranslationError::UnknownPlayer(owner.to_string()))?;
                let location = UnitLocation {
                    system: system_id.clone(),
                    planet: Some(planet.clone()),
                };
                let index = self
                    .planet_index(&location)?
                    .expect("planet location has index");
                out.push(commands::claim(colour, system_id.as_str(), index));
                if new.on_planet(planet).is_empty() {
                    out.push(commands::control(colour, system_id.as_str(), index));
                }
            }
        }
        Ok(out)
    }

    /// Translate physical card holdings, research, and public-objective reveals.
    pub fn card_changes(
        &self,
        before: &GameState,
        after: &GameState,
        content: &ti4_content::ContentStore,
        sources: SourceSet,
    ) -> Result<Vec<Command>, TranslationError> {
        let mut out = Vec::new();

        // Move/draw exact cards into their destination hands first. `card(..., "hand")` searches
        // every physical zone, so this also handles player-to-player transfers without depending
        // on seat iteration order. The removal pass below only discards cards with no recipient.
        let mut added_actions = BTreeSet::new();
        let mut added_secrets = BTreeSet::new();
        for old in &before.players {
            let Some(new) = after.player(&old.id) else {
                continue;
            };
            let colour = self
                .colours
                .get(&old.id)
                .ok_or_else(|| TranslationError::UnknownPlayer(old.id.to_string()))?;
            for card in added_ids(
                old.action_cards.iter().map(|id| id.as_str()),
                new.action_cards.iter().map(|id| id.as_str()),
            ) {
                added_actions.insert(card.clone());
                out.push(commands::card(
                    colour,
                    &content_name(content, ContentType::ActionCards, &card, sources)?,
                    "hand",
                ));
            }
            for card in added_ids(
                old.secret_objectives.iter().map(|id| id.as_str()),
                new.secret_objectives.iter().map(|id| id.as_str()),
            ) {
                added_secrets.insert(card.clone());
                out.push(commands::card(
                    colour,
                    &content_name(content, ContentType::SecretObjectives, &card, sources)?,
                    "hand",
                ));
            }
        }

        for old in &before.players {
            let Some(new) = after.player(&old.id) else {
                continue;
            };
            let colour = self
                .colours
                .get(&old.id)
                .ok_or_else(|| TranslationError::UnknownPlayer(old.id.to_string()))?;
            let newly_scored = after
                .scored_objectives
                .get(&old.id)
                .into_iter()
                .flat_map(|objectives| objectives.iter())
                .filter(|objective| {
                    !before
                        .scored_objectives
                        .get(&old.id)
                        .is_some_and(|old| old.contains(*objective))
                })
                .collect::<BTreeSet<_>>();
            for objective in &newly_scored {
                let (name, secret) = objective_name(content, objective.as_str(), sources)?;
                if secret {
                    out.push(commands::card(colour, &name, "area"));
                }
                out.push(commands::score(colour, &name, new.victory_points));
            }
            for card in removed_ids(
                old.secret_objectives.iter().map(|id| id.as_str()),
                new.secret_objectives.iter().map(|id| id.as_str()),
            ) {
                if added_secrets.contains(&card) {
                    continue;
                }
                if newly_scored
                    .iter()
                    .any(|objective| objective.as_str() == card)
                {
                    continue;
                }
                out.push(commands::card(
                    colour,
                    &content_name(content, ContentType::SecretObjectives, &card, sources)?,
                    "discard",
                ));
            }
            for card in removed_ids(
                old.action_cards.iter().map(|id| id.as_str()),
                new.action_cards.iter().map(|id| id.as_str()),
            ) {
                if added_actions.contains(&card) {
                    continue;
                }
                out.push(commands::card(
                    colour,
                    &content_name(content, ContentType::ActionCards, &card, sources)?,
                    "discard",
                ));
            }
            for technology in new.technologies.difference(&old.technologies) {
                out.push(commands::card_to_board(
                    colour,
                    &content_name(
                        content,
                        ContentType::Technologies,
                        technology.as_str(),
                        sources,
                    )?,
                    "technology",
                ));
            }
            for technology in old.technologies.intersection(&new.technologies) {
                let was_down = old.exhausted_technologies.contains(technology);
                let is_down = new.exhausted_technologies.contains(technology);
                if was_down != is_down {
                    out.push(commands::flip(
                        colour,
                        &content_name(
                            content,
                            ContentType::Technologies,
                            technology.as_str(),
                            sources,
                        )?,
                        !is_down,
                    ));
                }
            }
        }
        for objective in after
            .revealed_objectives
            .iter()
            .filter(|objective| !before.revealed_objectives.contains(objective))
        {
            out.push(commands::reveal_objective(&content_name(
                content,
                ContentType::PublicObjectives,
                objective.as_str(),
                sources,
            )?));
        }
        Ok(out)
    }

    /// Translate newly attached exploration tokens to exact planets.
    pub fn attachment_changes(
        &self,
        before: &GameState,
        after: &GameState,
        content: &ti4_content::ContentStore,
        sources: SourceSet,
    ) -> Result<Vec<Command>, TranslationError> {
        let mut out = Vec::new();
        for (planet, attachments) in &after.planet_attachments {
            let old = before
                .planet_attachments
                .get(planet)
                .map(Vec::as_slice)
                .unwrap_or_default();
            for attachment in added_ids(
                old.iter().map(String::as_str),
                attachments.iter().map(String::as_str),
            ) {
                let ((system, _), index) = self
                    .planet_indices
                    .iter()
                    .find(|((_, candidate), _)| candidate == planet)
                    .ok_or_else(|| TranslationError::UnknownPlanetName(planet.to_string()))?;
                let owner = after
                    .system_state(system)
                    .planet_control
                    .get(planet)
                    .cloned()
                    .or_else(|| {
                        before
                            .system_state(system)
                            .planet_control
                            .get(planet)
                            .cloned()
                    })
                    .ok_or_else(|| TranslationError::PlanetWithoutController(planet.to_string()))?;
                let colour = self
                    .colours
                    .get(&owner)
                    .ok_or_else(|| TranslationError::UnknownPlayer(owner.to_string()))?;
                out.push(commands::attach(
                    colour,
                    &content_name(content, ContentType::Explores, &attachment, sources)?,
                    system.as_str(),
                    *index,
                ));
            }
        }
        Ok(out)
    }

    fn colour(&self, unit: &UnitClass) -> Result<&str, TranslationError> {
        self.colours
            .get(&unit.owner)
            .map(String::as_str)
            .ok_or_else(|| TranslationError::UnknownPlayer(unit.owner.to_string()))
    }

    fn planet_index(&self, location: &UnitLocation) -> Result<Option<u32>, TranslationError> {
        let Some(planet) = &location.planet else {
            return Ok(None);
        };
        self.planet_indices
            .get(&(location.system.clone(), planet.clone()))
            .copied()
            .map(Some)
            .ok_or_else(|| TranslationError::UnknownPlanet {
                system: location.system.to_string(),
                planet: planet.to_string(),
            })
    }
}

fn positive_location(counts: &BTreeMap<UnitLocation, usize>) -> Option<UnitLocation> {
    counts
        .iter()
        .find(|(_, count)| **count > 0)
        .map(|(location, _)| location.clone())
}

fn subtract(counts: &mut BTreeMap<UnitLocation, usize>, location: &UnitLocation, count: usize) {
    *counts.get_mut(location).expect("selected location exists") -= count;
}

fn signed(count: usize) -> Result<i32, TranslationError> {
    i32::try_from(count).map_err(|_| TranslationError::CountOverflow(count))
}

fn strategy_name(
    content: &ti4_content::ContentStore,
    card: &str,
) -> Result<String, TranslationError> {
    ti4_engine::strategy_cards::card_name(content, card)
        .ok_or_else(|| TranslationError::UnknownStrategyCard(card.to_owned()))
}

fn planet_name(
    content: &ti4_content::ContentStore,
    planet: &str,
    sources: SourceSet,
) -> Result<String, TranslationError> {
    ti4_content::galaxy::planet(content, planet, sources)
        .and_then(|record| record.name())
        .map(ToOwned::to_owned)
        .ok_or_else(|| TranslationError::UnknownPlanetName(planet.to_owned()))
}

fn content_name(
    content: &ti4_content::ContentStore,
    kind: ContentType,
    id: &str,
    sources: SourceSet,
) -> Result<String, TranslationError> {
    let resolved = content
        .resolve_id(kind, id, sources)
        .ok_or_else(|| TranslationError::UnknownCardName(id.to_owned()))?;
    content
        .get(kind, resolved)
        .and_then(|record| record.text("name"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| TranslationError::UnknownCardName(id.to_owned()))
}

fn objective_name(
    content: &ti4_content::ContentStore,
    id: &str,
    sources: SourceSet,
) -> Result<(String, bool), TranslationError> {
    if let Ok(name) = content_name(content, ContentType::PublicObjectives, id, sources) {
        return Ok((name, false));
    }
    content_name(content, ContentType::SecretObjectives, id, sources).map(|name| (name, true))
}

fn removed_ids<'a>(
    before: impl Iterator<Item = &'a str>,
    after: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for id in after {
        *counts.entry(id).or_default() += 1;
    }
    let mut removed = Vec::new();
    for id in before {
        match counts.get_mut(id) {
            Some(count) if *count > 0 => *count -= 1,
            _ => removed.push(id.to_owned()),
        }
    }
    removed
}

fn added_ids<'a>(
    before: impl Iterator<Item = &'a str>,
    after: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    removed_ids(after, before)
}

fn text<'a>(event: &'a Event, key: &'static str) -> Result<&'a str, TranslationError> {
    event
        .text(key)
        .ok_or(TranslationError::MissingPayload { key })
}

fn boolean(event: &Event, key: &'static str) -> Result<bool, TranslationError> {
    event
        .boolean(key)
        .ok_or(TranslationError::MissingPayload { key })
}

/// A journal entry cannot be translated without guessing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranslationError {
    /// Required typed payload was absent or had the wrong JSON type.
    #[error("event payload is missing or has invalid field {key:?}")]
    MissingPayload { key: &'static str },
    /// The event names a player that was not mapped to a TTS colour.
    #[error("event names unmapped player {0:?}")]
    UnknownPlayer(String),
    /// The event names a unit with no corresponding TTS object.
    #[error("event names unmapped unit {0:?}")]
    UnknownUnit(String),
    /// No translation or explicit no-physical-effect classification exists.
    #[error("event type {0:?} has no TTS translation classification")]
    UnclassifiedEvent(String),
    /// A structural add/remove still needs semantic pairing.
    #[error("unit addition/removal has not been paired with a move, production or destruction")]
    UnpairedUnitChange,
    /// Galvanize is a separate physical token and cannot be folded into an ordinary unit command.
    #[error("galvanized unit mutation needs an explicit galvanize-token translation")]
    GalvanizedUnit,
    /// A planet name has no one-based index for this table system.
    #[error("planet {planet:?} in system {system:?} has no TTS index")]
    UnknownPlanet { system: String, planet: String },
    /// A unit multiset is larger than the executor's signed count representation.
    #[error("unit count {0} does not fit the command protocol")]
    CountOverflow(usize),
    /// Snapshots contain several possible routes for interchangeable units.
    #[error("unit {unit:?} has {sources} sources and {destinations} destinations")]
    AmbiguousUnitChange {
        unit: String,
        sources: usize,
        destinations: usize,
    },
    /// A strategy-card id has no physical card name in the content corpus.
    #[error("strategy card {0:?} has no TTS name")]
    UnknownStrategyCard(String),
    /// A planet id has no physical card name in the selected corpus.
    #[error("planet {0:?} has no TTS card name")]
    UnknownPlanetName(String),
    /// An exhausted planet card could not be associated with a seat area.
    #[error("planet {0:?} changed face without a controller")]
    PlanetWithoutController(String),
    /// A state card id has no physical display name in the selected corpus.
    #[error("card {0:?} has no TTS name")]
    UnknownCardName(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn translator() -> TtsCommands {
        TtsCommands::new([(PlayerId::new("l1z1x"), "Red".to_owned())])
    }

    fn event(event_type: &str, payload: &[(&str, serde_json::Value)]) -> Event {
        Event::new(
            1,
            event_type,
            payload
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn sustain_becomes_damage_for_the_exact_seat_system_and_unit() {
        let commands = translator()
            .event(&event(
                "SUSTAIN_DAMAGE_USED",
                &[
                    ("player", json!("l1z1x")),
                    ("system", json!("06")),
                    ("unit", json!("dreadnought2")),
                ],
            ))
            .expect("translated");
        assert_eq!(commands[0].action, "damage");
        assert_eq!(commands[0].rest["color"], "Red");
        assert_eq!(commands[0].rest["tile"], "6");
        assert_eq!(commands[0].rest["units"][0]["type"], "Dreadnought");
    }

    #[test]
    fn destruction_preserves_which_damage_state_died() {
        let commands = translator()
            .event(&event(
                "SHIP_DESTROYED",
                &[
                    ("player", json!("l1z1x")),
                    ("system", json!("06")),
                    ("unit", json!("dreadnought")),
                    ("damaged", json!(true)),
                ],
            ))
            .expect("translated");
        assert_eq!(commands[0].action, "remove");
        assert_eq!(commands[0].rest["damaged"], true);
    }

    #[test]
    fn cancelled_and_explicitly_invisible_events_are_not_commands() {
        let mut cancelled = event("SHIP_DESTROYED", &[]);
        cancelled.cancel();
        assert!(
            translator()
                .event(&cancelled)
                .expect("cancelled")
                .is_empty()
        );

        let invisible = translator().with_no_physical_effect("COMBAT_DICE_ROLLED");
        assert!(
            invisible
                .event(&event("COMBAT_DICE_ROLLED", &[]))
                .expect("classified")
                .is_empty()
        );
    }

    #[test]
    fn a_new_event_type_is_refused_not_silently_dropped() {
        assert_eq!(
            translator().event(&event("NEW_EFFECT", &[])),
            Err(TranslationError::UnclassifiedEvent("NEW_EFFECT".to_owned()))
        );
    }

    #[test]
    fn an_unannounced_planet_repair_uses_the_registered_one_based_index() {
        let translator = translator().with_planet_index(
            ti4_model::id::SystemId::new("18"),
            ti4_model::id::PlanetId::new("mecatol"),
            1,
        );
        let change = UnitChange::Damage {
            location: UnitLocation {
                system: ti4_model::id::SystemId::new("18"),
                planet: Some(ti4_model::id::PlanetId::new("mecatol")),
            },
            unit: crate::UnitClass {
                owner: PlayerId::new("l1z1x"),
                unit_type: ti4_model::id::UnitTypeId::new("mech"),
                galvanized: false,
            },
            damaged: false,
            count: 1,
        };
        let commands = translator.unit_change(&change).expect("translated");
        assert_eq!(commands[0].action, "repair");
        assert_eq!(commands[0].rest["planet"], 1);
    }

    fn infantry(location: UnitLocation, add: bool, count: usize) -> UnitChange {
        let unit = crate::UnitClass {
            owner: PlayerId::new("l1z1x"),
            unit_type: ti4_model::id::UnitTypeId::new("l1z1x_infantry"),
            galvanized: false,
        };
        if add {
            UnitChange::Add {
                location,
                unit,
                damaged: false,
                count,
            }
        } else {
            UnitChange::Remove {
                location,
                unit,
                damaged: false,
                count,
            }
        }
    }

    #[test]
    fn multi_planet_transport_becomes_exact_moves_to_space() {
        let translator = translator()
            .with_planet_index(
                ti4_model::id::SystemId::new("30"),
                ti4_model::id::PlanetId::new("mellon"),
                1,
            )
            .with_planet_index(
                ti4_model::id::SystemId::new("30"),
                ti4_model::id::PlanetId::new("zohbat"),
                2,
            );
        let changes = [
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("29"),
                    planet: None,
                },
                true,
                2,
            ),
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("30"),
                    planet: Some(ti4_model::id::PlanetId::new("mellon")),
                },
                false,
                1,
            ),
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("30"),
                    planet: Some(ti4_model::id::PlanetId::new("zohbat")),
                },
                false,
                1,
            ),
        ];
        let commands = translator.unit_changes(&changes).expect("unambiguous");
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().all(|command| command.action == "move"));
        assert_eq!(commands[0].rest["from_planet"], 1);
        assert_eq!(commands[1].rest["from_planet"], 2);
        assert!(
            commands
                .iter()
                .all(|command| command.rest["to_planet"] == 0)
        );
    }

    #[test]
    fn many_sources_to_many_destinations_is_refused() {
        let changes = [
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("01"),
                    planet: None,
                },
                false,
                1,
            ),
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("02"),
                    planet: None,
                },
                false,
                1,
            ),
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("03"),
                    planet: None,
                },
                true,
                1,
            ),
            infantry(
                UnitLocation {
                    system: ti4_model::id::SystemId::new("04"),
                    planet: None,
                },
                true,
                1,
            ),
        ];
        assert!(matches!(
            translator().unit_changes(&changes),
            Err(TranslationError::AmbiguousUnitChange { .. })
        ));
    }

    #[test]
    fn neutral_units_use_dedicated_plastic_without_becoming_a_seat() {
        let translator = translator().with_neutral_colour("Brown");
        let change = UnitChange::Add {
            location: UnitLocation {
                system: ti4_model::id::SystemId::new("19"),
                planet: None,
            },
            unit: crate::UnitClass {
                owner: PlayerId::new("neutral"),
                unit_type: ti4_model::id::UnitTypeId::new("destroyer"),
                galvanized: false,
            },
            damaged: false,
            count: 1,
        };
        let commands = translator.unit_changes(&[change]).expect("translated");
        assert_eq!(commands[0].action, "place");
        assert_eq!(commands[0].rest["color"], "Brown");
        assert_eq!(commands[0].rest["units"][0]["type"], "Destroyer");
    }

    fn strategy_state() -> GameState {
        GameState::new(
            &[PlayerId::new("l1z1x")],
            &[ti4_model::id::StrategyCardId::new("pok1leadership")],
            BTreeMap::new(),
            None,
            1,
        )
    }

    #[test]
    fn drafting_and_spending_a_strategy_card_moves_and_flips_the_named_card() {
        let content = ti4_content::ContentStore::embedded();
        let before = strategy_state();
        let mut held = before.clone();
        held.deal_strategy_card(
            &PlayerId::new("l1z1x"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        let drafted = translator()
            .strategy_changes(&before, &held, content)
            .expect("drafted");
        assert_eq!(drafted[0].action, "card");
        assert_eq!(drafted[0].rest["name"], "Leadership");
        assert_eq!(drafted[0].rest["to"], "area");

        let mut spent = held.clone();
        spent.exhaust_strategy_card(
            &PlayerId::new("l1z1x"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        let flipped = translator()
            .strategy_changes(&held, &spent, content)
            .expect("spent");
        assert_eq!(flipped[0].action, "flip");
        assert_eq!(flipped[0].rest["face"], "down");
    }

    #[test]
    fn clearing_all_held_strategy_cards_uses_the_mods_atomic_return() {
        let content = ti4_content::ContentStore::embedded();
        let mut before = strategy_state();
        before.deal_strategy_card(
            &PlayerId::new("l1z1x"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        let mut after = before.clone();
        after.clear_strategy_cards(&PlayerId::new("l1z1x"));
        let commands = translator()
            .strategy_changes(&before, &after, content)
            .expect("returned");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].action, "return_strategy_cards");
    }

    #[test]
    fn visible_counters_and_speaker_become_physical_commands() {
        let mut before = strategy_state();
        before.speaker = PlayerId::new("l1z1x");
        let mut after = before.clone();
        let player = after.player_mut(&PlayerId::new("l1z1x")).expect("seat");
        player.trade_goods += 2;
        player.commodities += 1;
        player.tactic_tokens -= 1;
        player.fleet_tokens += 1;
        player.victory_points = 3;
        let commands = translator()
            .scalar_changes(&before, &after)
            .expect("translated");
        let actions: Vec<_> = commands
            .iter()
            .map(|command| command.action.as_str())
            .collect();
        assert_eq!(
            actions,
            ["token", "token", "spend_token", "gain_token", "score"]
        );
        assert_eq!(commands[0].rest["count"], 2);
        assert_eq!(commands[4].rest["points"], 3);
        assert!(!commands[4].rest.contains_key("objective"));
    }

    #[test]
    fn activation_spends_its_tactic_token_once_but_other_placement_spends_none() {
        let before = strategy_state();
        let mut after = before.clone();
        let player = PlayerId::new("l1z1x");
        after.active = Some(player.clone());
        after.active_system = Some(ti4_model::id::SystemId::new("18"));
        after
            .system_mut(&ti4_model::id::SystemId::new("18"))
            .command_tokens
            .insert(player.clone());
        after.player_mut(&player).expect("seat").tactic_tokens -= 1;
        let commands = translator()
            .scalar_changes(&before, &after)
            .expect("translated");
        assert_eq!(
            commands
                .iter()
                .filter(|command| command.action == "activate")
                .count(),
            1
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| command.action == "end_turn")
                .count(),
            1
        );

        let mut diplomacy = before.clone();
        diplomacy
            .system_mut(&ti4_model::id::SystemId::new("19"))
            .command_tokens
            .insert(player);
        let commands = translator()
            .scalar_changes(&before, &diplomacy)
            .expect("translated");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].action, "place_command_token");
    }
}
