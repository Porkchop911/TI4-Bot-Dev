//! M11-007 — decoding the mod's board summary.
//!
//! The TI4 Tabletop Simulator mod already computes a compact description of everything publicly
//! visible on the table — every unit, control token and attachment, per system and per planet — and
//! POSTs it. That encoder is production code exercised by thousands of real games, and it is the
//! most valuable thing the mod offers this project: the observation half of the bridge is already
//! written and debugged. This module reads it.
//!
//! # The format
//!
//! ```text
//! Top:     system,system,...
//! System:  <tile>[<face><rotation>]<±x><±y><space region>;<planet region>;...
//! Region:  <colour A-Z><count 0-9*><unit a-z>* then '*' and attachments
//! ```
//!
//! It is **sticky-encoded**, which is where all the difficulty lives. Colour and count both persist
//! until changed, and both reset at every region boundary — each system, each planet, each
//! attachment marker. A lone `y` inherits the colour and count of whatever preceded it *within* its
//! region, so decoding cannot be done by matching units in isolation.
//!
//! # Three traps, each of which reads as correct and silently is not
//!
//! **Colour does not survive a planet boundary.** The mod's own Lua comment at the break says
//! `(Keep tile, color)`, which reads as though it does. Real captures say otherwise — they restate
//! the colour, as in `;Ps`. The format *requires* that, because "nobody's" is encoded as the
//! **absence** of a colour code; if colour carried across, an unowned token opening a planet region
//! would silently inherit the previous player's colour and mis-attribute ownership.
//!
//! **Orange is `E`.** The mod avoids `O` because it is indistinguishable from `0` in a string that
//! also encodes counts.
//!
//! **These unit codes are a fourth abbreviation scheme.** A cruiser is `r` here, `cr` in a
//! starting-fleet string, `ca` is a *carrier* in `AsyncTI4`'s ids, and technology aliases use `cr2`.
//! The mapping is explicit and total rather than inferred, and an unknown code is a refusal.
//!
//! # Refusing rather than guessing
//!
//! A partially decoded board is worse than none: reconciliation would compare the engine against a
//! table it only half understands and report plausible-looking differences that are really parse
//! failures. So every malformed input is [`MalformedSummary`], never a shorter board.
//!
//! Reconciliation is deliberately not here. This module answers "what is on the table?"; matching
//! that against the engine's expectation is M11-012 and M11-013, and keeping them apart means the
//! decoder can be tested against the mod's format with no notion of legality.

use std::fmt;

/// A player colour, as the mod names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Colour {
    White,
    Blue,
    Purple,
    Yellow,
    Red,
    Green,
    Orange,
    Pink,
    Brown,
}

impl Colour {
    /// The colour a summary code names.
    ///
    /// `E` is Orange. See the module docs: `O` would be indistinguishable from a zero.
    #[must_use]
    pub const fn from_code(code: char) -> Option<Self> {
        Some(match code {
            'W' => Self::White,
            'B' => Self::Blue,
            'P' => Self::Purple,
            'Y' => Self::Yellow,
            'R' => Self::Red,
            'G' => Self::Green,
            'E' => Self::Orange,
            'K' => Self::Pink,
            'N' => Self::Brown,
            _ => return None,
        })
    }

    /// The code the mod writes for this colour.
    #[must_use]
    pub const fn code(self) -> char {
        match self {
            Self::White => 'W',
            Self::Blue => 'B',
            Self::Purple => 'P',
            Self::Yellow => 'Y',
            Self::Red => 'R',
            Self::Green => 'G',
            Self::Orange => 'E',
            Self::Pink => 'K',
            Self::Brown => 'N',
        }
    }

    /// The mod's own name for this colour.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Yellow => "Yellow",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Orange => "Orange",
            Self::Pink => "Pink",
            Self::Brown => "Brown",
        }
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What occupies a unit position in the summary.
///
/// The last two are not units. They occupy the same position and use the same colour coding, which
/// is why they are in this enum rather than modelled separately — but [`Piece::is_token`] keeps
/// them out of every count of what is actually standing somewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Piece {
    Carrier,
    Cruiser,
    Destroyer,
    Dreadnought,
    Fighter,
    Flagship,
    Infantry,
    Mech,
    Pds,
    SpaceDock,
    WarSun,
    /// Marks control of a planet rather than a unit standing on it.
    OwnerToken,
    /// A command token, which sits in a system's space region.
    CommandToken,
}

impl Piece {
    /// The piece a summary code names. Total over the mod's alphabet, and nothing else.
    #[must_use]
    pub const fn from_code(code: char) -> Option<Self> {
        Some(match code {
            'c' => Self::Carrier,
            'r' => Self::Cruiser,
            'y' => Self::Destroyer,
            'd' => Self::Dreadnought,
            'f' => Self::Fighter,
            'h' => Self::Flagship,
            'i' => Self::Infantry,
            'm' => Self::Mech,
            'p' => Self::Pds,
            's' => Self::SpaceDock,
            'w' => Self::WarSun,
            'o' => Self::OwnerToken,
            't' => Self::CommandToken,
            _ => return None,
        })
    }

    /// The code the mod writes for this piece.
    #[must_use]
    pub const fn code(self) -> char {
        match self {
            Self::Carrier => 'c',
            Self::Cruiser => 'r',
            Self::Destroyer => 'y',
            Self::Dreadnought => 'd',
            Self::Fighter => 'f',
            Self::Flagship => 'h',
            Self::Infantry => 'i',
            Self::Mech => 'm',
            Self::Pds => 'p',
            Self::SpaceDock => 's',
            Self::WarSun => 'w',
            Self::OwnerToken => 'o',
            Self::CommandToken => 't',
        }
    }

    /// Whether this marks something rather than standing somewhere.
    #[must_use]
    pub const fn is_token(self) -> bool {
        matches!(self, Self::OwnerToken | Self::CommandToken)
    }
}

/// Some number of one piece, belonging to one colour.
///
/// The colour is optional because the format encodes "nobody's" as the absence of a colour code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stack {
    /// What it is.
    pub piece: Piece,
    /// Whose, when the summary said.
    pub colour: Option<Colour>,
    /// How many.
    pub count: u32,
}

impl Stack {
    /// Whether this is a token rather than a unit.
    #[must_use]
    pub const fn is_token(&self) -> bool {
        self.piece.is_token()
    }
}

/// Something attached to a region: a planet attachment, or an anomaly marker in space.
///
/// Attachment identifiers are their own alphabet, they overlap the unit codes, and they are
/// distinguished from units *only by position* — past the `*`. They are kept as raw codes rather
/// than resolved here for that reason: resolving them needs the content store, and mistaking one
/// for a unit would put a ship on a board that has none.
///
/// Uppercase is legal. `O` appeared on a live Tomb of Emphidia token as `Wio*O`; treating every
/// capital past the star as a player colour made a valid mid-game board impossible to reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attachment {
    /// The raw code, case preserved.
    pub code: char,
    /// How many, under the same sticky count rule as units.
    pub count: u32,
}

/// One space area or one planet: what is standing there, and what is attached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Region {
    /// In the order the summary listed them.
    pub stacks: Vec<Stack>,
    /// Everything past the region's `*`.
    pub attachments: Vec<Attachment>,
}

impl Region {
    /// This colour's real units here — tokens excluded.
    #[must_use]
    pub fn units_of(&self, colour: Colour) -> Vec<Stack> {
        self.stacks
            .iter()
            .filter(|stack| stack.colour == Some(colour) && !stack.is_token())
            .copied()
            .collect()
    }

    /// Whose control token sits here, if anyone's.
    ///
    /// A *fact about the table*, not a conclusion about control. LRR 25.4 places a token only when
    /// a player controls a planet they have no units on, so plenty of controlled planets carry
    /// none. Inferring control from this is M11-013's problem, with the other rules.
    #[must_use]
    pub fn owner_token(&self) -> Option<Colour> {
        self.stacks
            .iter()
            .find(|stack| stack.piece == Piece::OwnerToken)
            .and_then(|stack| stack.colour)
    }

    /// Everyone with a real unit here, sorted and deduplicated.
    ///
    /// Ordinarily at most one, after combat.
    #[must_use]
    pub fn occupiers(&self) -> Vec<Colour> {
        let mut colours: Vec<Colour> = self
            .stacks
            .iter()
            .filter(|stack| !stack.is_token())
            .filter_map(|stack| stack.colour)
            .collect();
        colours.sort_unstable();
        colours.dedup();
        colours
    }
}

/// One tile: its space, then its planets in the mod's planet order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    /// The tile number, as written. Kept as text because it is an identifier, not a quantity.
    pub tile: String,
    /// Grid column.
    pub x: i32,
    /// Grid row.
    pub y: i32,
    /// Face and rotation, for hyperlane tiles, which are placed rather than fixed.
    pub hyperlane: Option<String>,
    /// The space area.
    pub space: Region,
    /// The planets, in the mod's order.
    pub planets: Vec<Region>,
}

impl System {
    /// The colours with a command token in this system's space.
    #[must_use]
    pub fn command_tokens(&self) -> Vec<Colour> {
        self.space
            .stacks
            .iter()
            .filter(|stack| stack.piece == Piece::CommandToken)
            .filter_map(|stack| stack.colour)
            .collect()
    }
}

/// Everything publicly visible on the table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    /// In the order the summary listed them.
    pub systems: Vec<System>,
}

impl Board {
    /// The system with this tile number, if it is on the table.
    #[must_use]
    pub fn system(&self, tile: &str) -> Option<&System> {
        self.systems.iter().find(|system| system.tile == tile)
    }

    /// Every tile on the table, in summary order.
    #[must_use]
    pub fn tiles(&self) -> Vec<&str> {
        self.systems
            .iter()
            .map(|system| system.tile.as_str())
            .collect()
    }
}

/// Why a summary did not parse.
///
/// Every variant names the offending text, because the caller's next move is to look at a capture
/// and a refusal that does not say what it choked on costs a round trip with a live table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MalformedSummary {
    /// A system with no readable `<tile><±x><±y>` header.
    #[error("no tile header in {0:?}")]
    NoHeader(String),
    /// An uppercase letter outside an attachment run that names no colour.
    #[error("unknown colour code {code:?} in {region:?}")]
    UnknownColour { code: char, region: String },
    /// A lowercase letter outside an attachment run that names no piece.
    #[error("unknown unit code {code:?} in {region:?}")]
    UnknownUnit { code: char, region: String },
    /// Anything that is neither a digit, a letter, nor the attachment marker.
    #[error("unexpected character {code:?} in region {region:?}")]
    UnexpectedCharacter { code: char, region: String },
    /// A count too large to be a number of pieces.
    ///
    /// The mod cannot emit one; a string that contains one is either corrupt or hostile, and
    /// either way the board it describes is not a board.
    #[error("count {digits:?} in {region:?} is not a possible number of pieces")]
    CountOutOfRange { digits: String, region: String },
}

/// The largest count a region may state.
///
/// Not an arbitrary safety number: a TI4 board holds a few hundred plastic pieces in total, so four
/// digits is already far past anything the mod can encode, and refusing beyond it bounds the work a
/// hostile string can ask for without ever refusing a real board.
const MAX_COUNT: u32 = 9_999;

const SYSTEM_DELIMITER: char = ',';
const PLANET_DELIMITER: char = ';';
const ATTACHMENT_DELIMITER: char = '*';

/// Parse a hex summary into a board.
///
/// An empty summary is an empty board rather than an error: the mod sends one before any tiles are
/// placed, and refusing it would make a fresh table look like a broken one.
///
/// # Errors
/// [`MalformedSummary`] for anything the grammar does not cover. Never a partial board.
pub fn decode(summary: &str) -> Result<Board, MalformedSummary> {
    if summary.trim().is_empty() {
        return Ok(Board::default());
    }
    let systems = summary
        .split(SYSTEM_DELIMITER)
        .filter(|part| !part.is_empty())
        .map(decode_system)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Board { systems })
}

/// `<tile>` optionally `<face><rotation>`, then signed grid x and y, then the regions.
fn decode_system(text: &str) -> Result<System, MalformedSummary> {
    let bytes = text.as_bytes();
    let mut at = 0usize;

    let tile_start = at;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
    }
    if at == tile_start {
        return Err(MalformedSummary::NoHeader(text.to_owned()));
    }
    let tile = text[tile_start..at].to_owned();

    // Hyperlanes are placed rather than fixed, so they carry a face and a rotation between the
    // tile number and the coordinates.
    let hyperlane = if at + 1 < bytes.len()
        && matches!(bytes[at], b'A' | b'B')
        && bytes[at + 1].is_ascii_digit()
    {
        let face = text[at..at + 2].to_owned();
        at += 2;
        Some(face)
    } else {
        None
    };

    let x = take_signed(text, &mut at)?;
    let y = take_signed(text, &mut at)?;

    // Always at least one region — the space — even when the rest of the string is empty.
    let mut regions = text[at..].split(PLANET_DELIMITER);
    let space = decode_region(regions.next().unwrap_or_default())?;
    let planets = regions.map(decode_region).collect::<Result<Vec<_>, _>>()?;

    Ok(System {
        tile,
        x,
        y,
        hyperlane,
        space,
        planets,
    })
}

/// One signed coordinate. The sign is mandatory, which is what separates it from the tile number.
fn take_signed(text: &str, at: &mut usize) -> Result<i32, MalformedSummary> {
    let bytes = text.as_bytes();
    if *at >= bytes.len() || !matches!(bytes[*at], b'+' | b'-') {
        return Err(MalformedSummary::NoHeader(text.to_owned()));
    }
    let negative = bytes[*at] == b'-';
    *at += 1;
    let start = *at;
    while *at < bytes.len() && bytes[*at].is_ascii_digit() {
        *at += 1;
    }
    if *at == start {
        return Err(MalformedSummary::NoHeader(text.to_owned()));
    }
    let magnitude: i32 = text[start..*at]
        .parse()
        .map_err(|_| MalformedSummary::NoHeader(text.to_owned()))?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// One region: a space area or one planet.
///
/// Both sticky values are local to this region. Nothing flows out — see the module docs for why the
/// mod's own comment suggests otherwise and the wire format does not.
fn decode_region(text: &str) -> Result<Region, MalformedSummary> {
    let mut region = Region::default();
    let mut colour: Option<Colour> = None;
    let mut count: u32 = 1;
    let mut in_attachments = false;

    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        if character == ATTACHMENT_DELIMITER {
            // Attachments reset both sticky values and land in their own bucket.
            in_attachments = true;
            colour = None;
            count = 1;
            continue;
        }

        if character.is_ascii_digit() {
            let start = index;
            let mut end = index + character.len_utf8();
            while characters
                .peek()
                .is_some_and(|(_, next)| next.is_ascii_digit())
            {
                let (position, digit) = characters.next().expect("peeked");
                end = position + digit.len_utf8();
            }
            let digits = &text[start..end];
            count = digits
                .parse::<u32>()
                .ok()
                .filter(|n| *n <= MAX_COUNT)
                .ok_or(MalformedSummary::CountOutOfRange {
                    digits: digits.to_owned(),
                    region: text.to_owned(),
                })?;
            continue;
        }

        // Checked before the colour branch on purpose: past a '*' an uppercase letter is an
        // attachment code, not a player colour.
        if in_attachments && character.is_alphabetic() {
            region.attachments.push(Attachment {
                code: character,
                count,
            });
            continue;
        }

        if character.is_ascii_uppercase() {
            colour = Some(
                Colour::from_code(character).ok_or(MalformedSummary::UnknownColour {
                    code: character,
                    region: text.to_owned(),
                })?,
            );
            // A colour change resets the count. Two ships of different colours are written
            // `2cWc`, and the White carrier is one carrier, not two.
            count = 1;
            continue;
        }

        if character.is_ascii_lowercase() {
            let piece = Piece::from_code(character).ok_or(MalformedSummary::UnknownUnit {
                code: character,
                region: text.to_owned(),
            })?;
            region.stacks.push(Stack {
                piece,
                colour,
                count,
            });
            continue;
        }

        return Err(MalformedSummary::UnexpectedCharacter {
            code: character,
            region: text.to_owned(),
        });
    }

    Ok(region)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> System {
        let board = decode(text).expect("a well-formed system");
        assert_eq!(board.systems.len(), 1, "{text:?}");
        board.systems.into_iter().next().expect("one system")
    }

    fn stack(piece: Piece, colour: Option<Colour>, count: u32) -> Stack {
        Stack {
            piece,
            colour,
            count,
        }
    }

    #[test]
    fn an_empty_summary_is_an_empty_board() {
        // The mod sends one before any tiles are placed; refusing it would make a fresh table look
        // like a broken one.
        for text in ["", "   ", "\n"] {
            assert_eq!(decode(text).expect("empty"), Board::default(), "{text:?}");
        }
    }

    #[test]
    fn a_tile_carries_its_grid_position_including_negatives() {
        let system = one("18+0+0");
        assert_eq!(system.tile, "18");
        assert_eq!((system.x, system.y), (0, 0));

        let system = one("10-1-5");
        assert_eq!(system.tile, "10");
        assert_eq!((system.x, system.y), (-1, -5));
        assert_eq!(system.hyperlane, None);
    }

    #[test]
    fn a_hyperlane_carries_its_face_and_rotation() {
        let system = one("83A2-3+1");
        assert_eq!(system.tile, "83");
        assert_eq!(system.hyperlane.as_deref(), Some("A2"));
        assert_eq!((system.x, system.y), (-3, 1));
    }

    #[test]
    fn a_count_applies_to_the_unit_that_follows_and_is_sticky_after_it() {
        // `2c3f` is two carriers and three fighters; `3fi` is three fighters and three infantry.
        let space = one("18+0+0B2c3f").space;
        assert_eq!(
            space.stacks,
            vec![
                stack(Piece::Carrier, Some(Colour::Blue), 2),
                stack(Piece::Fighter, Some(Colour::Blue), 3),
            ]
        );

        let space = one("18+0+0B3fi").space;
        assert_eq!(
            space.stacks,
            vec![
                stack(Piece::Fighter, Some(Colour::Blue), 3),
                stack(Piece::Infantry, Some(Colour::Blue), 3),
            ],
            "count persists until something changes it"
        );
    }

    #[test]
    fn a_colour_change_resets_the_count() {
        // Otherwise the White carrier in `2cWc` would be read as two carriers.
        let space = one("18+0+0B2cWc").space;
        assert_eq!(
            space.stacks,
            vec![
                stack(Piece::Carrier, Some(Colour::Blue), 2),
                stack(Piece::Carrier, Some(Colour::White), 1),
            ]
        );
    }

    #[test]
    fn neither_sticky_value_crosses_a_region_boundary() {
        // The one the mod's own Lua comment gets wrong. If colour carried, the unowned token
        // opening the second planet would be read as Blue's.
        let system = one("18+0+0B2c;i;Ro");
        assert_eq!(
            system.space.stacks,
            vec![stack(Piece::Carrier, Some(Colour::Blue), 2)]
        );
        assert_eq!(
            system.planets[0].stacks,
            vec![stack(Piece::Infantry, None, 1)],
            "colour and count both reset at the planet break"
        );
        assert_eq!(system.planets[1].owner_token(), Some(Colour::Red));
    }

    #[test]
    fn colour_resets_between_systems_too() {
        let board = decode("18+0+0Bc,19+1+0c").expect("two systems");
        assert_eq!(board.systems[0].space.stacks[0].colour, Some(Colour::Blue));
        assert_eq!(board.systems[1].space.stacks[0].colour, None);
    }

    #[test]
    fn orange_is_e_and_o_is_never_a_colour() {
        assert_eq!(Colour::from_code('E'), Some(Colour::Orange));
        assert_eq!(Colour::from_code('O'), None);
        // `o` lowercase is the owner token, which is what the letter is reserved for.
        assert_eq!(Piece::from_code('o'), Some(Piece::OwnerToken));

        let error = decode("18+0+0Oc").expect_err("O is not a colour");
        assert!(
            matches!(error, MalformedSummary::UnknownColour { code: 'O', .. }),
            "{error}"
        );
    }

    #[test]
    fn every_code_in_the_alphabet_decodes_and_round_trips() {
        for code in [
            'c', 'r', 'y', 'd', 'f', 'h', 'i', 'm', 'p', 's', 'w', 'o', 't',
        ] {
            let piece = Piece::from_code(code).unwrap_or_else(|| panic!("{code} is a unit code"));
            assert_eq!(piece.code(), code);
        }
        for code in ['W', 'B', 'P', 'Y', 'R', 'G', 'E', 'K', 'N'] {
            let colour =
                Colour::from_code(code).unwrap_or_else(|| panic!("{code} is a colour code"));
            assert_eq!(colour.code(), code);
        }
    }

    #[test]
    fn an_empty_planet_is_still_a_planet() {
        // `26-1-3;;` is a system with two planets and nothing on either. Dropping empty regions
        // would renumber every planet after them.
        let system = one("26-1-3;;");
        assert_eq!(system.planets.len(), 2);
        assert_eq!(system.planets[0], Region::default());
        assert_eq!(system.planets[1], Region::default());
    }

    #[test]
    fn tokens_are_facts_about_the_table_and_never_units() {
        let system = one("69+2-2Gt;Wio");
        assert_eq!(system.command_tokens(), vec![Colour::Green]);

        let planet = &system.planets[0];
        assert_eq!(planet.owner_token(), Some(Colour::White));
        assert_eq!(
            planet.occupiers(),
            vec![Colour::White],
            "the infantry occupies; the token does not"
        );
        assert_eq!(
            planet.units_of(Colour::White).len(),
            1,
            "the token is not a unit"
        );
    }

    #[test]
    fn a_planet_with_units_and_no_token_has_no_token() {
        let planet = one("18+0+0;Bi").planets[0].clone();
        assert_eq!(planet.owner_token(), None);
        assert_eq!(planet.occupiers(), vec![Colour::Blue]);
    }

    #[test]
    fn attachments_follow_a_star_and_reset_both_sticky_values() {
        let planet = one("18+0+0;B2i*e").planets[0].clone();
        assert_eq!(
            planet.stacks,
            vec![stack(Piece::Infantry, Some(Colour::Blue), 2)]
        );
        assert_eq!(
            planet.attachments,
            vec![Attachment {
                code: 'e',
                count: 1
            }],
            "the star resets the count as well as the colour"
        );
    }

    #[test]
    fn an_attachment_code_may_collide_with_a_unit_code() {
        // Past the star the letter is recorded as-is; `i` here is an attachment, not infantry.
        let planet = one("18+0+0;Bo*i").planets[0].clone();
        assert_eq!(
            planet.stacks,
            vec![stack(Piece::OwnerToken, Some(Colour::Blue), 1)]
        );
        assert_eq!(
            planet.attachments,
            vec![Attachment {
                code: 'i',
                count: 1
            }]
        );
    }

    #[test]
    fn an_attachment_code_may_be_uppercase() {
        // A live Tomb of Emphidia token. Treating every capital past the star as a colour made a
        // valid mid-game board impossible to reload.
        let planet = one("69+2-2;Wio*O").planets[0].clone();
        assert_eq!(planet.owner_token(), Some(Colour::White));
        assert_eq!(
            planet.attachments,
            vec![Attachment {
                code: 'O',
                count: 1
            }]
        );
    }

    #[test]
    fn a_space_region_can_carry_attachments() {
        // Anomaly markers: `39-2+2*e;` is an attachment in space and one empty planet.
        let system = one("39-2+2*e;");
        assert_eq!(system.space.stacks, vec![]);
        assert_eq!(
            system.space.attachments,
            vec![Attachment {
                code: 'e',
                count: 1
            }]
        );
        assert_eq!(system.planets.len(), 1);
    }

    #[test]
    fn a_malformed_summary_is_refused_rather_than_half_decoded() {
        // A partial board would be compared against the engine and produce plausible-looking
        // differences that are really parse failures.
        for text in [
            "nonsense",
            "18",
            "18+0",
            "18+0+",
            "+0+0",
            "18+0+0Bz",
            "18+0+0B c",
        ] {
            assert!(decode(text).is_err(), "{text:?} must not parse");
        }
    }

    #[test]
    fn a_count_no_board_could_hold_is_refused() {
        let error = decode("18+0+0B99999c").expect_err("not a possible count");
        assert!(
            matches!(error, MalformedSummary::CountOutOfRange { .. }),
            "{error}"
        );
        // The bound is far past anything real, so a large but plausible count still parses.
        assert_eq!(
            one("18+0+0B120f").space.stacks[0].count,
            120,
            "the limit must never refuse a board the mod could emit"
        );
    }

    #[test]
    fn a_real_capture_decodes_whole() {
        // A six-player mid-game board, captured from a live table.
        let summary = "1-3-1Py2c3f5i;Ps,6-2+4Ycd3f5i;Yps,10-1-5Bcdfy3i;Bs;,\
                       14+2-4Wf2r;Wipso;Wio,69+2-2GtWc2f1t;Wio;Wio,39-2+2*e;";
        let board = decode(summary).expect("a real capture");
        assert_eq!(board.tiles(), vec!["1", "6", "10", "14", "69", "39"]);

        let one = board.system("1").expect("tile 1");
        assert_eq!((one.x, one.y), (-3, -1));
        assert_eq!(
            one.space.stacks,
            vec![
                stack(Piece::Destroyer, Some(Colour::Purple), 1),
                stack(Piece::Carrier, Some(Colour::Purple), 2),
                stack(Piece::Fighter, Some(Colour::Purple), 3),
                stack(Piece::Infantry, Some(Colour::Purple), 5),
            ]
        );
        assert_eq!(one.planets.len(), 1);
        assert_eq!(
            one.planets[0].stacks,
            vec![stack(Piece::SpaceDock, Some(Colour::Purple), 1)]
        );

        // The interesting one: two colours in one space region, with the count reset between.
        let sixty_nine = board.system("69").expect("tile 69");
        assert_eq!(
            sixty_nine.space.stacks,
            vec![
                stack(Piece::CommandToken, Some(Colour::Green), 1),
                stack(Piece::Carrier, Some(Colour::White), 1),
                stack(Piece::Fighter, Some(Colour::White), 2),
                stack(Piece::CommandToken, Some(Colour::White), 1),
            ]
        );
        assert_eq!(
            sixty_nine.command_tokens(),
            vec![Colour::Green, Colour::White]
        );
        assert_eq!(
            sixty_nine.space.occupiers(),
            vec![Colour::White],
            "Green has a token here and nothing else"
        );

        // A planet held with units, and one held by token alone.
        let fourteen = board.system("14").expect("tile 14");
        assert_eq!(fourteen.planets[0].owner_token(), Some(Colour::White));
        assert_eq!(fourteen.planets[1].owner_token(), Some(Colour::White));
        assert_eq!(fourteen.planets[1].occupiers(), vec![Colour::White]);
    }
}
