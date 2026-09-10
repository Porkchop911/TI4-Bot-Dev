//! M11-014 and M11-015 — building the commands the executor runs.
//!
//! Every function here returns a [`Command`] whose fields are exactly the ones
//! `bridgeDispatch` reads for that action. The field names are not a design choice: they are read
//! off `tts/bridge_executor.lua`, which is the side that cannot be redeployed to every table
//! already running the mod.
//!
//! # Two conversions that go the opposite way to the importer
//!
//! **Tiles are written the mod's way.** The executor compares against the mod's own system
//! registry, which spells tile one as `1`. The engine spells it `01`. [`crate::mapping`] pads on
//! the way in; [`mod_tile`] strips on the way out. Sending `01` to the table finds no system and
//! the command is refused.
//!
//! **Units are named the mod's way, and upgrades collapse.** The executor matches on the TTS
//! object name — `Space Dock`, `War Sun`, `PDS` — and an upgraded unit is still the same object on
//! the table, because the upgrade is a card and not a different model. So `fighter2` becomes
//! `Fighter`. This is the sixth abbreviation scheme in the project and none of the others fit it.
//!
//! # Everything carries a colour
//!
//! The executor needs to know whose pieces to touch, and there is no default seat. A command
//! without `color` is refused by the table, which is a slow way to discover a missing argument, so
//! every builder here takes one.

use crate::wire::Command;

/// Engine base unit type to the object name TTS matches on.
const MOD_UNIT_NAMES: [(&str, &str); 11] = [
    ("carrier", "Carrier"),
    ("cruiser", "Cruiser"),
    ("destroyer", "Destroyer"),
    ("dreadnought", "Dreadnought"),
    ("fighter", "Fighter"),
    ("flagship", "Flagship"),
    ("infantry", "Infantry"),
    ("mech", "Mech"),
    ("pds", "PDS"),
    ("spacedock", "Space Dock"),
    ("warsun", "War Sun"),
];

/// A unit type with no name on the TTS side.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no TTS object name for unit type {0:?}")]
pub struct UnknownUnit(pub String);

/// The TTS object name for an engine unit type.
///
/// Upgrades resolve to their base name: a `fighter2` is still a `Fighter` object on the table.
///
/// # Errors
/// [`UnknownUnit`] for a faction-specific or otherwise unmapped type. Returned rather than guessed
/// because a wrong object name silently matches nothing and the move is refused on the table, far
/// from the code that chose it.
pub fn mod_unit(unit_type: &str) -> Result<&'static str, UnknownUnit> {
    // Faction implementations retain their rules identity in Rust (`sol_carrier2`,
    // `l1z1x_dreadnought`), but use the same physical model in TTS.
    let base = unit_type
        .rsplit_once('_')
        .map_or(unit_type, |(_, physical)| physical)
        .trim_end_matches('2');
    MOD_UNIT_NAMES
        .iter()
        .find(|(engine, _)| *engine == base)
        .map(|(_, name)| *name)
        .ok_or_else(|| UnknownUnit(unit_type.to_owned()))
}

/// The mod's spelling of a tile id.
///
/// The engine zero-pads single digits and the mod does not. Sending the engine's `01` to the table
/// matches no system, so this strips it back.
#[must_use]
pub fn mod_tile(tile: &str) -> String {
    let trimmed = tile.trim_start_matches('0');
    if trimmed.is_empty() {
        tile.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// A `units` payload: `[{"type": "Carrier", "count": 2}, ...]`.
///
/// Zero counts are dropped — the executor would look for no units and succeed vacuously, which
/// reads as a move that happened.
///
/// # Errors
/// [`UnknownUnit`] if any type has no TTS name.
fn unit_payload(units: &[(&str, i32)]) -> Result<serde_json::Value, UnknownUnit> {
    let mut out = Vec::new();
    for (unit_type, count) in units {
        if *count <= 0 {
            continue;
        }
        out.push(serde_json::json!({ "type": mod_unit(unit_type)?, "count": count }));
    }
    Ok(serde_json::Value::Array(out))
}

fn command(action: &str, colour: &str, fields: &[(&str, serde_json::Value)]) -> Command {
    let mut built = Command::new(action);
    built
        .rest
        .insert("color".to_owned(), colour.to_owned().into());
    for (name, value) in fields {
        built.rest.insert((*name).to_owned(), value.clone());
    }
    built
}

/// A no-op that proves the channel works end to end.
#[must_use]
pub fn ping(message: &str) -> Command {
    let mut built = Command::new("ping");
    built
        .rest
        .insert("message".to_owned(), message.to_owned().into());
    built
}

/// Activate a system: place a command token in it.
#[must_use]
pub fn activate(colour: &str, tile: &str) -> Command {
    command("activate", colour, &[("tile", mod_tile(tile).into())])
}

/// Place a command token from reinforcements without spending a command-sheet pool.
#[must_use]
pub fn place_command_token(colour: &str, tile: &str) -> Command {
    command(
        "place_command_token",
        colour,
        &[("tile", mod_tile(tile).into())],
    )
}

/// Move units between systems.
///
/// # Errors
/// [`UnknownUnit`] if any unit type has no TTS name.
pub fn move_units(
    colour: &str,
    from: &str,
    to: &str,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    Ok(command(
        "move",
        colour,
        &[
            ("from", mod_tile(from).into()),
            ("to", mod_tile(to).into()),
            ("units", unit_payload(units)?),
        ],
    ))
}

/// Move units between exact board regions. `None` means the space area; planet indexes are
/// one-based. Unlike the legacy [`move_units`] builder this always emits source/destination
/// region fields, so ground forces cannot be picked from the wrong planet.
pub fn relocate_units(
    colour: &str,
    from: &str,
    from_planet: Option<u32>,
    to: &str,
    to_planet: Option<u32>,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    let mut built = move_units(colour, from, to, units)?;
    built
        .rest
        .insert("from_planet".to_owned(), from_planet.unwrap_or(0).into());
    built
        .rest
        .insert("to_planet".to_owned(), to_planet.unwrap_or(0).into());
    Ok(built)
}

/// Land ground forces on a planet.
///
/// # Errors
/// [`UnknownUnit`] if any unit type has no TTS name.
pub fn land(
    colour: &str,
    tile: &str,
    planet: u32,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    Ok(command(
        "land",
        colour,
        &[
            ("tile", mod_tile(tile).into()),
            ("planet", planet.into()),
            ("units", unit_payload(units)?),
        ],
    ))
}

/// Put newly produced units into an exact board region.
pub fn place_units(
    colour: &str,
    tile: &str,
    planet: Option<u32>,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    Ok(command(
        "place",
        colour,
        &[
            ("tile", mod_tile(tile).into()),
            ("planet", planet.unwrap_or(0).into()),
            ("units", unit_payload(units)?),
        ],
    ))
}

/// Mark matching units in a system as having sustained damage.
///
/// The mod's Burninator treats a face-down unit (Z rotation between 90 and 270 degrees) as
/// damaged. The executor owns that physical convention; callers name only the logical units.
///
/// # Errors
/// [`UnknownUnit`] if any unit type has no TTS name.
pub fn damage_units(
    colour: &str,
    tile: &str,
    planet: Option<u32>,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    let mut built = command(
        "damage",
        colour,
        &[
            ("tile", mod_tile(tile).into()),
            ("units", unit_payload(units)?),
        ],
    );
    built
        .rest
        .insert("planet".to_owned(), planet.unwrap_or(0).into());
    Ok(built)
}

/// Repair matching damaged units in a system.
///
/// # Errors
/// [`UnknownUnit`] if any unit type has no TTS name.
pub fn repair_units(
    colour: &str,
    tile: &str,
    planet: Option<u32>,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    let mut built = command(
        "repair",
        colour,
        &[
            ("tile", mod_tile(tile).into()),
            ("units", unit_payload(units)?),
        ],
    );
    built
        .rest
        .insert("planet".to_owned(), planet.unwrap_or(0).into());
    Ok(built)
}

/// Return destroyed units from a system to their reinforcement bags.
///
/// `damaged` selects the physical copy when fresh and damaged copies of the same unit coexist.
/// Omitting that distinction makes the unit count right while leaving the wrong survivor damaged.
///
/// # Errors
/// [`UnknownUnit`] if any unit type has no TTS name.
pub fn remove_units(
    colour: &str,
    tile: &str,
    planet: Option<u32>,
    damaged: Option<bool>,
    units: &[(&str, i32)],
) -> Result<Command, UnknownUnit> {
    let mut built = command(
        "remove",
        colour,
        &[
            ("tile", mod_tile(tile).into()),
            ("units", unit_payload(units)?),
        ],
    );
    built
        .rest
        .insert("planet".to_owned(), planet.unwrap_or(0).into());
    if let Some(damaged) = damaged {
        built.rest.insert("damaged".to_owned(), damaged.into());
    }
    Ok(built)
}

/// Take the planet card for a planet this seat now controls.
#[must_use]
pub fn claim(colour: &str, tile: &str, planet: u32) -> Command {
    command(
        "claim",
        colour,
        &[("tile", mod_tile(tile).into()), ("planet", planet.into())],
    )
}

/// Place a control token on a planet held without units (LRR 25.4).
#[must_use]
pub fn control(colour: &str, tile: &str, planet: u32) -> Command {
    command(
        "control",
        colour,
        &[("tile", mod_tile(tile).into()), ("planet", planet.into())],
    )
}

/// Adjust a command token pool by `count`, which may be negative.
#[must_use]
pub fn token(colour: &str, kind: &str, count: i32) -> Command {
    command(
        "token",
        colour,
        &[("kind", kind.into()), ("count", count.into())],
    )
}

/// Take a command token into a pool.
#[must_use]
pub fn gain_token(colour: &str, pool: &str) -> Command {
    command("gain_token", colour, &[("pool", pool.into())])
}

/// Spend a command token from a pool.
#[must_use]
pub fn spend_token(colour: &str, pool: &str) -> Command {
    command("spend_token", colour, &[("pool", pool.into())])
}

/// Return a command token from a system to reinforcements.
#[must_use]
pub fn return_token(colour: &str, tile: &str) -> Command {
    command("return_token", colour, &[("tile", mod_tile(tile).into())])
}

/// Set a named card face up or face down.
#[must_use]
pub fn flip(colour: &str, name: &str, face_up: bool) -> Command {
    command(
        "flip",
        colour,
        &[
            ("name", name.into()),
            ("face", if face_up { "up" } else { "down" }.into()),
        ],
    )
}

/// Move a card to a zone — a hand, the table, a discard.
#[must_use]
pub fn card(colour: &str, name: &str, to: &str) -> Command {
    command("card", colour, &[("name", name.into()), ("to", to.into())])
}

/// Move a named card to a particular board in a player's area.
#[must_use]
pub fn card_to_board(colour: &str, name: &str, board: &str) -> Command {
    let mut built = card(colour, name, "area");
    built.rest.insert("board".to_owned(), board.into());
    built
}

/// Pull a named exploration attachment and attach its token to an exact planet.
#[must_use]
pub fn attach(colour: &str, name: &str, tile: &str, planet: u32) -> Command {
    let mut built = card(colour, name, "planet");
    built.rest.insert("tile".to_owned(), mod_tile(tile).into());
    built.rest.insert("planet".to_owned(), planet.into());
    built
}

/// Deal anonymous cards from a physical deck into a player's hidden hand.
#[must_use]
pub fn deal(colour: &str, deck: &str, count: i32) -> Command {
    command(
        "deal",
        colour,
        &[("deck", deck.into()), ("count", count.into())],
    )
}

/// Reveal one named public objective from its physical deck.
#[must_use]
pub fn reveal_objective(name: &str) -> Command {
    let mut built = Command::new("reveal_objective");
    built.rest.insert("name".to_owned(), name.into());
    built
}

/// Return every strategy card to the initiative-ordered strategy mat.
#[must_use]
pub fn return_strategy_cards() -> Command {
    Command::new("return_strategy_cards")
}

/// Remove accumulated trade-good tokens from a strategy card when it is drafted.
#[must_use]
pub fn clear_card_goods(name: &str) -> Command {
    let mut built = Command::new("clear_card_goods");
    built.rest.insert("name".to_owned(), name.into());
    built
}

/// Score an objective for points.
#[must_use]
pub fn score(colour: &str, objective: &str, points: i32) -> Command {
    command(
        "score",
        colour,
        &[("objective", objective.into()), ("points", points.into())],
    )
}

/// Move only the scoreboard marker, for points not tied to an objective card.
#[must_use]
pub fn score_total(colour: &str, points: i32) -> Command {
    command("score", colour, &[("points", points.into())])
}

/// Give a colour the speaker token.
#[must_use]
pub fn speaker(colour: &str) -> Command {
    command("speaker", colour, &[])
}

/// Hand TTS's turn marker to this exact colour.
#[must_use]
pub fn end_turn(colour: &str) -> Command {
    command("end_turn", colour, &[("to", colour.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(command: &'a Command, name: &str) -> &'a serde_json::Value {
        command
            .rest
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from {command:?}"))
    }

    #[test]
    fn an_upgraded_unit_is_still_the_same_object_on_the_table() {
        // The upgrade is a card, not a different model. A `Fighter II` object does not exist.
        assert_eq!(mod_unit("fighter").expect("known"), "Fighter");
        assert_eq!(mod_unit("fighter2").expect("known"), "Fighter");
        assert_eq!(mod_unit("dreadnought2").expect("known"), "Dreadnought");
        assert_eq!(mod_unit("sol_carrier2").expect("known"), "Carrier");
        assert_eq!(mod_unit("l1z1x_dreadnought").expect("known"), "Dreadnought");
    }

    #[test]
    fn the_two_word_and_acronym_names_are_exact() {
        // The executor matches on the object name. `Spacedock` or `Warsun` find nothing at all.
        assert_eq!(mod_unit("spacedock").expect("known"), "Space Dock");
        assert_eq!(mod_unit("warsun").expect("known"), "War Sun");
        assert_eq!(mod_unit("pds").expect("known"), "PDS");
    }

    #[test]
    fn a_unit_with_no_table_object_is_refused_rather_than_guessed() {
        // A wrong object name matches nothing and the move is refused on the table, a long way
        // from the code that chose it.
        let error = mod_unit("arborec_orbital").expect_err("no such object");
        assert_eq!(error, UnknownUnit("arborec_orbital".to_owned()));
    }

    #[test]
    fn tiles_go_out_the_way_the_mod_spells_them() {
        // The inverse of the importer's padding. Sending `01` matches no system on the table.
        assert_eq!(mod_tile("01"), "1");
        assert_eq!(mod_tile("06"), "6");
        assert_eq!(mod_tile("18"), "18");
        assert_eq!(mod_tile("112"), "112");
        // A tile that is all zeroes is not a tile, but it must not become the empty string.
        assert_eq!(mod_tile("0"), "0");
    }

    #[test]
    fn a_move_carries_exactly_what_the_executor_reads() {
        // `color`, `from`, `to` and a `units` table -- the executor refuses the command without
        // all four, and reports that refusal into a game's chat rather than to us.
        let command = move_units("Blue", "01", "18", &[("carrier", 2), ("fighter2", 3)])
            .expect("known units");
        assert_eq!(command.action, "move");
        assert_eq!(field(&command, "color"), "Blue");
        assert_eq!(field(&command, "from"), "1");
        assert_eq!(field(&command, "to"), "18");
        assert_eq!(
            field(&command, "units"),
            &serde_json::json!([
                { "type": "Carrier", "count": 2 },
                { "type": "Fighter", "count": 3 }
            ])
        );
    }

    #[test]
    fn a_zero_count_is_dropped_rather_than_sent() {
        // The executor would look for no units and succeed, which reads as a move that happened.
        let command =
            move_units("Red", "18", "19", &[("carrier", 0), ("cruiser", 1)]).expect("known units");
        assert_eq!(
            field(&command, "units"),
            &serde_json::json!([{ "type": "Cruiser", "count": 1 }])
        );
    }

    #[test]
    fn every_builder_names_a_colour() {
        // There is no default seat on the table.
        let commands = [
            activate("Blue", "18"),
            place_command_token("Blue", "18"),
            claim("Blue", "18", 1),
            control("Blue", "18", 1),
            token("Blue", "command", -1),
            gain_token("Blue", "strategy"),
            spend_token("Blue", "tactics"),
            return_token("Blue", "18"),
            card("Blue", "Sabotage", "discard"),
            score("Blue", "Corner the Market", 1),
            speaker("Blue"),
            end_turn("Blue"),
            land("Blue", "18", 1, &[("infantry", 1)]).expect("known"),
            move_units("Blue", "18", "19", &[("carrier", 1)]).expect("known"),
            damage_units("Blue", "18", None, &[("dreadnought", 1)]).expect("known"),
            repair_units("Blue", "18", None, &[("dreadnought", 1)]).expect("known"),
            remove_units("Blue", "18", None, Some(true), &[("dreadnought", 1)]).expect("known"),
        ];
        for command in &commands {
            assert_eq!(
                field(command, "color"),
                "Blue",
                "{} carries no colour",
                command.action
            );
        }
        // `ping` is the exception and deliberately so: it touches nobody's pieces.
        assert!(!ping("hello").rest.contains_key("color"));
    }

    #[test]
    fn activate_names_the_tile_not_the_system() {
        // The executor reads `command.tile`. `system` would be silently absent and the activation
        // would do nothing.
        let command = activate("Green", "69");
        assert!(command.rest.contains_key("tile"));
        assert!(!command.rest.contains_key("system"));
    }

    #[test]
    fn a_landing_names_its_tile_planet_and_units() {
        let command = land("White", "14", 1, &[("infantry", 2)]).expect("known units");
        assert_eq!(field(&command, "tile"), "14");
        assert_eq!(field(&command, "planet"), 1);
        assert_eq!(
            field(&command, "units"),
            &serde_json::json!([{ "type": "Infantry", "count": 2 }])
        );
    }

    #[test]
    fn exact_relocation_and_placement_encode_space_as_zero() {
        let moved =
            relocate_units("Blue", "01", Some(1), "18", None, &[("infantry", 2)]).expect("known");
        assert_eq!(field(&moved, "from_planet"), 1);
        assert_eq!(field(&moved, "to_planet"), 0);

        let placed = place_units("Blue", "18", None, &[("fighter", 2)]).expect("known");
        assert_eq!(placed.action, "place");
        assert_eq!(field(&placed, "planet"), 0);
    }

    #[test]
    fn a_built_command_survives_the_wire() {
        // The builders write into `Command::rest`, which is flattened. A field that did not
        // survive serialisation would reach the table as a command missing an argument.
        let command = move_units("Purple", "01", "02", &[("carrier", 1)]).expect("known units");
        let text = serde_json::to_string(&command).expect("serialisable");
        let back: Command = serde_json::from_str(&text).expect("round trips");
        assert_eq!(back, command);
        assert!(text.contains(r#""action":"move""#), "{text}");
    }

    #[test]
    fn damage_and_repair_name_the_same_logical_units() {
        let damaged = damage_units("Red", "06", None, &[("dreadnought2", 2)]).expect("known");
        let repaired = repair_units("Red", "06", None, &[("dreadnought2", 2)]).expect("known");
        assert_eq!(damaged.action, "damage");
        assert_eq!(repaired.action, "repair");
        assert_eq!(field(&damaged, "tile"), "6");
        assert_eq!(field(&damaged, "units"), field(&repaired, "units"));
        assert_eq!(
            field(&damaged, "units"),
            &serde_json::json!([{ "type": "Dreadnought", "count": 2 }])
        );

        let mech = damage_units("Red", "18", Some(1), &[("mech", 1)]).expect("known");
        assert_eq!(field(&mech, "planet"), 1);
    }

    #[test]
    fn removal_can_select_the_damage_state_and_planet() {
        let command =
            remove_units("Yellow", "18", Some(1), Some(true), &[("mech", 1)]).expect("known");
        assert_eq!(field(&command, "planet"), 1);
        assert_eq!(field(&command, "damaged"), true);
        assert_eq!(
            field(&command, "units"),
            &serde_json::json!([{ "type": "Mech", "count": 1 }])
        );
    }

    #[test]
    fn returning_a_command_token_names_its_system_not_a_pool() {
        let command = return_token("Red", "06");
        assert_eq!(field(&command, "tile"), "6");
        assert!(!command.rest.contains_key("pool"));
    }

    #[test]
    fn flip_explicitly_names_the_desired_face() {
        let down = flip("White", "Leadership", false);
        let up = flip("White", "Jord", true);
        assert_eq!(field(&down, "face"), "down");
        assert_eq!(field(&up, "face"), "up");
    }
}
