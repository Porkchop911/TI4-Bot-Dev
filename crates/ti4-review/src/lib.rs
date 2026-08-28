//! A dependency-free-from-the-engine offline game-review viewer.
//!
//! The boundary is JSON data only: this crate deliberately cannot receive `GameState`, an event
//! log, a decision log, a seed, or an RNG. Callers must construct an audience-projected bundle.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::collapsible_if,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines,
    clippy::unnecessary_semicolon
)]

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub const MAX_BUNDLE_BYTES: usize = 67_108_864;
const MAX_MANIFEST_BYTES: usize = 65_536;
const MAX_PAYLOAD_BYTES: usize = 66_060_288;
const MAX_FRAME_BYTES: usize = 524_288;
const MAX_TIMELINE_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    InvalidUtf8,
    JsonSyntax,
    DuplicateKey,
    NoncanonicalJson,
    RootShape,
    UnknownField,
    UnsupportedVersion,
    InvalidValue,
    LimitExceeded,
    PayloadChecksumMismatch,
    BadReference,
    BadFrameOrder,
    BadTimelineOrder,
    PrivacyViolation,
    TerminalConflict,
    NotSupported,
}
impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUtf8 => "invalid_utf8",
            Self::JsonSyntax => "json_syntax",
            Self::DuplicateKey => "duplicate_key",
            Self::NoncanonicalJson => "noncanonical_json",
            Self::RootShape => "root_shape",
            Self::UnknownField => "unknown_field",
            Self::UnsupportedVersion => "unsupported_version",
            Self::InvalidValue => "invalid_value",
            Self::LimitExceeded => "limit_exceeded",
            Self::PayloadChecksumMismatch => "payload_checksum_mismatch",
            Self::BadReference => "bad_reference",
            Self::BadFrameOrder => "bad_frame_order",
            Self::BadTimelineOrder => "bad_timeline_order",
            Self::PrivacyViolation => "privacy_violation",
            Self::TerminalConflict => "terminal_conflict",
            Self::NotSupported => "not_supported",
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{code}: {detail}", code = .code.as_str())]
pub struct ReviewError {
    pub code: ErrorCode,
    pub detail: String,
}
type Result<T> = std::result::Result<T, ReviewError>;
fn err(code: ErrorCode, detail: impl Into<String>) -> ReviewError {
    ReviewError {
        code,
        detail: detail.into(),
    }
}

/// A fully validated, canonical, audience-specific review artifact.
#[derive(Clone, Debug)]
pub struct ReviewBundle {
    document: Value,
}
impl ReviewBundle {
    pub fn document(&self) -> &Value {
        &self.document
    }
}

/// Parses and validates a v1 `.ti4review.json` file before a caller may render it.
pub fn validate_bytes(bytes: &[u8]) -> Result<ReviewBundle> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(err(ErrorCode::LimitExceeded, "bundle exceeds 64 MiB"));
    }
    std::str::from_utf8(bytes).map_err(|_| err(ErrorCode::InvalidUtf8, "bundle is not UTF-8"))?;
    let strict: StrictValue = serde_json::from_slice(bytes).map_err(json_error)?;
    let canonical = canonical(&strict.0)?;
    if canonical != bytes {
        return Err(err(
            ErrorCode::NoncanonicalJson,
            "input is not canonical JSON",
        ));
    }
    validate_document(&strict.0)?;
    Ok(ReviewBundle { document: strict.0 })
}

/// A complete public sample; useful for checking the graphical application without a simulator.
pub fn canonical_example() -> Result<Vec<u8>> {
    let state0 = json!({"phase":"setup","players":[player("seat-1","Hacan","hacan",3),player("seat-2","Jol-Nar","jolnar",2)],"round":1,"systems":[]});
    let state1 = json!({"phase":"action","players":[player("seat-1","Hacan","hacan",3),player("seat-2","Jol-Nar","jolnar",2)],"round":2,"systems":[{"kind":"centre","planets":[{"exhausted":false,"id":"mecatol-rex","influence":6,"label":"Mecatol Rex","owner":"seat-1","resources":1}],"q":0,"r":0,"tile":{"count":1,"id":"mecatol","label":"Mecatol Rex"},"units":[{"count":3,"damaged":false,"kind":"infantry","owner":"seat-1"}]}]});
    let payload = json!({"frames":[
        {"cause":{"kind":"initial"},"id":0,"state":state0,"state_sha256":sha(&canonical(&json!({"phase":"setup","players":[player("seat-1","Hacan","hacan",3),player("seat-2","Jol-Nar","jolnar",2)],"round":1,"systems":[]}))?)},
        {"cause":{"index":0,"kind":"timeline"},"id":1,"state":state1,"state_sha256":sha(&canonical(&json!({"phase":"action","players":[player("seat-1","Hacan","hacan",3),player("seat-2","Jol-Nar","jolnar",2)],"round":2,"systems":[{"kind":"centre","planets":[{"exhausted":false,"id":"mecatol-rex","influence":6,"label":"Mecatol Rex","owner":"seat-1","resources":1}],"q":0,"r":0,"tile":{"count":1,"id":"mecatol","label":"Mecatol Rex"},"units":[{"count":3,"damaged":false,"kind":"infantry","owner":"seat-1"}]}]}))?)}
    ],"timeline":[{"facts":[{"label":"Capture","value":"Sample review frame"}],"frame":1,"index":0,"kind":"terminal"}]});
    let document = json!({"manifest":{"audience":{"kind":"public"},"content_sha256":"0000000000000000000000000000000000000000000000000000000000000000","engine_revision":"0000000000000000000000000000000000000000","frame_count":2,"generator_version":"0.1.0","map_sha256":"0000000000000000000000000000000000000000000000000000000000000000","payload_sha256":sha(&canonical(&payload)?),"schema":"ti4-review-bundle","schema_version":1,"source_kind":"scripted","terminal":{"kind":"horizon_reached"},"timeline_count":1},"payload":payload});
    let bytes = canonical(&document)?;
    validate_bytes(&bytes)?;
    Ok(bytes)
}
fn player(seat: &str, name: &str, faction: &str, score: u8) -> Value {
    json!({"faction":faction,"influence":3,"items":[],"name":name,"resources":4,"score":score,"seat":seat,"strategy_cards":[],"trade_goods":1})
}

fn validate_document(document: &Value) -> Result<()> {
    let root = object(document, ErrorCode::RootShape, "root must be an object")?;
    keys(root, &["manifest", "payload"])?;
    let manifest = object(
        field(root, "manifest")?,
        ErrorCode::RootShape,
        "manifest must be an object",
    )?;
    keys(
        manifest,
        &[
            "audience",
            "content_sha256",
            "engine_revision",
            "frame_count",
            "generator_version",
            "map_sha256",
            "payload_sha256",
            "schema",
            "schema_version",
            "source_kind",
            "terminal",
            "timeline_count",
        ],
    )?;
    if string(field(manifest, "schema")?)? != "ti4-review-bundle" {
        return Err(err(ErrorCode::RootShape, "unsupported schema name"));
    }
    if integer(field(manifest, "schema_version")?)? != 1 {
        return Err(err(
            ErrorCode::UnsupportedVersion,
            "only schema_version 1 is supported",
        ));
    }
    text(string(field(manifest, "generator_version")?)?)?;
    let version = string(field(manifest, "generator_version")?)?;
    if !version.is_ascii()
        || version.len() > 64
        || version.split('.').count() != 3
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
    {
        return Err(err(
            ErrorCode::InvalidValue,
            "generator_version is not ASCII SemVer",
        ));
    }
    hex(
        string(field(manifest, "engine_revision")?)?,
        40,
        "engine_revision",
    )?;
    for name in ["content_sha256", "map_sha256", "payload_sha256"] {
        hex(string(field(manifest, name)?)?, 64, name)?;
    }
    let manifest_bytes = canonical(&Value::Object(manifest.clone()))?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(err(ErrorCode::LimitExceeded, "manifest exceeds 64 KiB"));
    }
    audience(field(manifest, "audience")?)?;
    match string(field(manifest, "source_kind")?)? {
        "audited" | "scripted" => (),
        _ => return Err(err(ErrorCode::InvalidValue, "unknown source kind")),
    }
    terminal(field(manifest, "terminal")?)?;
    let payload = object(
        field(root, "payload")?,
        ErrorCode::RootShape,
        "payload must be an object",
    )?;
    keys(payload, &["frames", "timeline"])?;
    let payload_bytes = canonical(&Value::Object(payload.clone()))?;
    if payload_bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(err(ErrorCode::LimitExceeded, "payload exceeds 63 MiB"));
    }
    if sha(&payload_bytes) != string(field(manifest, "payload_sha256")?)? {
        return Err(err(
            ErrorCode::PayloadChecksumMismatch,
            "payload SHA-256 mismatch",
        ));
    }
    let frames = array(field(payload, "frames")?)?;
    let timeline = array(field(payload, "timeline")?)?;
    if frames.is_empty() || frames.len() > 4096 || timeline.is_empty() || timeline.len() > 4095 {
        return Err(err(
            ErrorCode::LimitExceeded,
            "invalid frame/timeline count",
        ));
    }
    if integer(field(manifest, "frame_count")?)? as usize != frames.len()
        || integer(field(manifest, "timeline_count")?)? as usize != timeline.len()
        || frames.len() != timeline.len() + 1
    {
        return Err(err(
            ErrorCode::BadReference,
            "manifest counts do not match payload",
        ));
    }
    let mut seats = None;
    for (index, frame) in frames.iter().enumerate() {
        validate_frame(frame, index, &mut seats)?;
    }
    for (index, item) in timeline.iter().enumerate() {
        validate_timeline(item, index, timeline.len())?;
    }
    let players = seats.ok_or_else(|| err(ErrorCode::BadReference, "missing player seats"))?;
    references(manifest, &players)?;
    Ok(())
}

fn validate_frame(value: &Value, index: usize, seats: &mut Option<BTreeSet<String>>) -> Result<()> {
    if canonical(value)?.len() > MAX_FRAME_BYTES {
        return Err(err(ErrorCode::LimitExceeded, "frame exceeds 512 KiB"));
    }
    let frame = object(value, ErrorCode::RootShape, "frame must be an object")?;
    keys(frame, &["cause", "id", "state", "state_sha256"])?;
    if integer(field(frame, "id")?)? as usize != index {
        return Err(err(
            ErrorCode::BadFrameOrder,
            "frame IDs must be contiguous",
        ));
    }
    let cause = object(
        field(frame, "cause")?,
        ErrorCode::RootShape,
        "cause must be an object",
    )?;
    if index == 0 {
        keys(cause, &["kind"])?;
        if string(field(cause, "kind")?)? != "initial" {
            return Err(err(ErrorCode::BadFrameOrder, "first frame must be initial"));
        }
    } else {
        keys(cause, &["index", "kind"])?;
        if string(field(cause, "kind")?)? != "timeline"
            || integer(field(cause, "index")?)? as usize + 1 != index
        {
            return Err(err(
                ErrorCode::BadFrameOrder,
                "frame cause must reference prior timeline index",
            ));
        }
    }
    hex(string(field(frame, "state_sha256")?)?, 64, "state_sha256")?;
    let state = field(frame, "state")?;
    if sha(&canonical(state)?) != string(field(frame, "state_sha256")?)? {
        return Err(err(
            ErrorCode::PayloadChecksumMismatch,
            "state SHA-256 mismatch",
        ));
    }
    let found = validate_state(state)?;
    match seats {
        None => *seats = Some(found),
        Some(previous) if *previous == found => (),
        Some(_) => {
            return Err(err(
                ErrorCode::BadReference,
                "player seats change between frames",
            ));
        }
    }
    Ok(())
}

fn validate_state(value: &Value) -> Result<BTreeSet<String>> {
    let state = object(value, ErrorCode::RootShape, "state must be an object")?;
    keys(state, &["phase", "players", "round", "systems"])?;
    if integer(field(state, "round")?)? > 1000 {
        return Err(err(ErrorCode::LimitExceeded, "round exceeds 1000"));
    }
    match string(field(state, "phase")?)? {
        "setup" | "strategy" | "action" | "status" | "agenda" | "finished" | "error" => (),
        _ => return Err(err(ErrorCode::InvalidValue, "unknown phase")),
    }
    let players = array(field(state, "players")?)?;
    if !(2..=8).contains(&players.len()) {
        return Err(err(ErrorCode::LimitExceeded, "must have 2-8 players"));
    }
    let mut seats = BTreeSet::new();
    let mut previous = "";
    for player in players {
        let player = object(player, ErrorCode::RootShape, "player must be object")?;
        keys(
            player,
            &[
                "faction",
                "influence",
                "items",
                "name",
                "resources",
                "score",
                "seat",
                "strategy_cards",
                "trade_goods",
            ],
        )?;
        let seat = string(field(player, "seat")?)?;
        id(seat)?;
        if !previous.is_empty() && previous >= seat || !seats.insert(seat.to_owned()) {
            return Err(err(
                ErrorCode::BadFrameOrder,
                "players must be uniquely sorted by seat",
            ));
        }
        previous = seat;
        id(string(field(player, "faction")?)?)?;
        text(string(field(player, "name")?)?)?;
        for n in ["influence", "resources", "trade_goods"] {
            if integer(field(player, n)?)? > 999 {
                return Err(err(ErrorCode::LimitExceeded, "player counter exceeds 999"));
            }
        }
        if integer(field(player, "score")?)? > 20 {
            return Err(err(ErrorCode::LimitExceeded, "score exceeds 20"));
        }
        sorted_ids(array(field(player, "strategy_cards")?)?)?;
        display_items(array(field(player, "items")?)?)?;
    }
    let systems = array(field(state, "systems")?)?;
    if systems.len() > 256 {
        return Err(err(ErrorCode::LimitExceeded, "systems exceed 256"));
    }
    let mut coords = BTreeSet::new();
    let mut planets = BTreeSet::new();
    let mut prior = None;
    for system in systems {
        let system = object(system, ErrorCode::RootShape, "system must be object")?;
        keys(system, &["kind", "planets", "q", "r", "tile", "units"])?;
        let q = integer(field(system, "q")?)?;
        let r = integer(field(system, "r")?)?;
        if !(-32..=32).contains(&q)
            || !(-32..=32).contains(&r)
            || !coords.insert((q, r))
            || prior.is_some_and(|p| p >= (q, r))
        {
            return Err(err(
                ErrorCode::BadFrameOrder,
                "systems must be uniquely sorted by axial coordinate",
            ));
        }
        prior = Some((q, r));
        match string(field(system, "kind")?)? {
            "home" | "centre" | "normal" | "hyperlane" => (),
            _ => return Err(err(ErrorCode::InvalidValue, "unknown system kind")),
        };
        display_item(field(system, "tile")?)?;
        let ps = array(field(system, "planets")?)?;
        let us = array(field(system, "units")?)?;
        if ps.len() > 8 || us.len() > 256 {
            return Err(err(
                ErrorCode::LimitExceeded,
                "system exceeds planet/unit bound",
            ));
        }
        let mut prior_planet = "";
        for planet in ps {
            let planet = object(planet, ErrorCode::RootShape, "planet must object")?;
            keys(
                planet,
                &[
                    "exhausted",
                    "id",
                    "influence",
                    "label",
                    "owner",
                    "resources",
                ],
            )?;
            let pid = string(field(planet, "id")?)?;
            id(pid)?;
            if !prior_planet.is_empty() && prior_planet >= pid || !planets.insert(pid.to_owned()) {
                return Err(err(
                    ErrorCode::BadFrameOrder,
                    "planets must be uniquely sorted",
                ));
            }
            prior_planet = pid;
            text(string(field(planet, "label")?)?)?;
            if integer(field(planet, "influence")?)? > 99
                || integer(field(planet, "resources")?)? > 99
            {
                return Err(err(ErrorCode::LimitExceeded, "planet value exceeds 99"));
            }
            if !field(planet, "exhausted")?.is_boolean() {
                return Err(err(ErrorCode::InvalidValue, "exhausted must be boolean"));
            }
            if let Some(owner) = field(planet, "owner")?.as_str() {
                id(owner)?;
                if !seats.contains(owner) {
                    return Err(err(ErrorCode::BadReference, "planet owner is not a player"));
                }
            } else if !field(planet, "owner")?.is_null() {
                return Err(err(
                    ErrorCode::InvalidValue,
                    "planet owner must be ID or null",
                ));
            }
        }
        let mut prior_unit = None;
        for unit in us {
            let unit = object(unit, ErrorCode::RootShape, "unit must object")?;
            keys(unit, &["count", "damaged", "kind", "owner"])?;
            let owner = string(field(unit, "owner")?)?;
            let kind = string(field(unit, "kind")?)?;
            id(owner)?;
            id(kind)?;
            let count = integer(field(unit, "count")?)?;
            if count == 0 || count > 999 || !seats.contains(owner) {
                return Err(err(ErrorCode::BadReference, "invalid unit"));
            }
            let damaged = field(unit, "damaged")?
                .as_bool()
                .ok_or_else(|| err(ErrorCode::InvalidValue, "damaged must be boolean"))?;
            let key = (owner, kind, damaged);
            if prior_unit.is_some_and(|p| p >= key) {
                return Err(err(ErrorCode::BadFrameOrder, "units must be sorted"));
            }
            prior_unit = Some(key);
        }
    }
    Ok(seats)
}

fn validate_timeline(value: &Value, index: usize, len: usize) -> Result<()> {
    if canonical(value)?.len() > MAX_TIMELINE_BYTES {
        return Err(err(
            ErrorCode::LimitExceeded,
            "timeline item exceeds 16 KiB",
        ));
    }
    let item = object(value, ErrorCode::RootShape, "timeline item must object")?;
    keys(item, &["facts", "frame", "index", "kind"])?;
    if integer(field(item, "index")?)? as usize != index
        || integer(field(item, "frame")?)? as usize != index + 1
    {
        return Err(err(
            ErrorCode::BadTimelineOrder,
            "timeline must be contiguous",
        ));
    }
    let terminal = string(field(item, "kind")?)? == "terminal";
    if terminal != (index + 1 == len) {
        return Err(err(
            ErrorCode::TerminalConflict,
            "only final timeline item may be terminal",
        ));
    }
    match string(field(item, "kind")?)? {
        "decision" | "event" | "phase" | "terminal" => (),
        _ => return Err(err(ErrorCode::InvalidValue, "unknown timeline kind")),
    }
    let facts = array(field(item, "facts")?)?;
    if facts.len() > 32 {
        return Err(err(ErrorCode::LimitExceeded, "too many facts"));
    }
    for fact in facts {
        let fact = object(fact, ErrorCode::RootShape, "fact must object")?;
        keys(fact, &["label", "value"])?;
        text(string(field(fact, "label")?)?)?;
        text(string(field(fact, "value")?)?)?;
    }
    Ok(())
}
fn references(manifest: &Map<String, Value>, seats: &BTreeSet<String>) -> Result<()> {
    let audience = object(
        field(manifest, "audience")?,
        ErrorCode::RootShape,
        "audience must object",
    )?;
    if string(field(audience, "kind")?)? == "seat"
        && !seats.contains(string(field(audience, "seat")?)?)
    {
        return Err(err(
            ErrorCode::BadReference,
            "seat audience not in player list",
        ));
    }
    let terminal = object(
        field(manifest, "terminal")?,
        ErrorCode::RootShape,
        "terminal must object",
    )?;
    if string(field(terminal, "kind")?)? == "completed" {
        if let Some(winner) = field(terminal, "winner")?.as_str() {
            id(winner)?;
            if !seats.contains(winner) {
                return Err(err(ErrorCode::BadReference, "winner not in player list"));
            }
        }
    }
    Ok(())
}
fn audience(value: &Value) -> Result<()> {
    let a = object(value, ErrorCode::RootShape, "audience must object")?;
    match string(field(a, "kind")?)? {
        "public" | "referee" => keys(a, &["kind"]),
        "seat" => {
            keys(a, &["kind", "seat"])?;
            id(string(field(a, "seat")?)?)
        }
        _ => Err(err(ErrorCode::InvalidValue, "unknown audience")),
    }
}
fn terminal(value: &Value) -> Result<()> {
    let t = object(value, ErrorCode::RootShape, "terminal must object")?;
    match string(field(t, "kind")?)? {
        "completed" => {
            keys(t, &["kind", "winner"])?;
            if !(field(t, "winner")?.is_null() || field(t, "winner")?.is_string()) {
                return Err(err(ErrorCode::InvalidValue, "winner must be ID or null"));
            }
        }
        "horizon_reached" => keys(t, &["kind"])?,
        "capture_failed" => {
            keys(t, &["code", "kind"])?;
            match string(field(t, "code")?)? {
                "capture_limit" | "export_failed" | "aborted" => (),
                _ => return Err(err(ErrorCode::InvalidValue, "unknown capture failure")),
            }
        }
        "engine_failed" => {
            keys(t, &["code", "kind"])?;
            match string(field(t, "code")?)? {
                "engine_error" | "replay_error" => (),
                _ => return Err(err(ErrorCode::InvalidValue, "unknown engine failure")),
            }
        }
        _ => return Err(err(ErrorCode::InvalidValue, "unknown terminal")),
    }
    Ok(())
}
fn display_items(values: &[Value]) -> Result<()> {
    if values.len() > 128 {
        return Err(err(ErrorCode::LimitExceeded, "too many display items"));
    }
    let mut prior = "";
    for value in values {
        display_item(value)?;
        let item = object(value, ErrorCode::RootShape, "display item must object")?;
        let current = string(field(item, "id")?)?;
        if !prior.is_empty() && prior >= current {
            return Err(err(
                ErrorCode::BadFrameOrder,
                "display items must be sorted",
            ));
        }
        prior = current;
    }
    Ok(())
}
fn display_item(value: &Value) -> Result<()> {
    let item = object(value, ErrorCode::RootShape, "display item must object")?;
    keys(item, &["count", "id", "label"])?;
    id(string(field(item, "id")?)?)?;
    text(string(field(item, "label")?)?)?;
    if integer(field(item, "count")?)? > 999 {
        return Err(err(ErrorCode::LimitExceeded, "display count exceeds 999"));
    }
    Ok(())
}
fn sorted_ids(values: &[Value]) -> Result<()> {
    if values.len() > 128 {
        return Err(err(ErrorCode::LimitExceeded, "too many IDs"));
    }
    let mut prior = "";
    for value in values {
        let current = string(value)?;
        id(current)?;
        if !prior.is_empty() && prior >= current {
            return Err(err(ErrorCode::BadFrameOrder, "IDs must be sorted"));
        }
        prior = current;
    }
    Ok(())
}
fn keys(value: &Map<String, Value>, expected: &[&str]) -> Result<()> {
    if value.len() != expected.len() {
        return Err(err(ErrorCode::UnknownField, "wrong field count"));
    }
    for name in expected {
        if !value.contains_key(*name) {
            return Err(err(ErrorCode::RootShape, format!("missing field {name}")));
        }
    }
    Ok(())
}
fn object<'a>(value: &'a Value, code: ErrorCode, detail: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| err(code, detail))
}
fn array(value: &Value) -> Result<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| err(ErrorCode::InvalidValue, "expected array"))
}
fn field<'a>(value: &'a Map<String, Value>, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| err(ErrorCode::RootShape, format!("missing field {name}")))
}
fn string(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| err(ErrorCode::InvalidValue, "expected string"))
}
fn integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| err(ErrorCode::InvalidValue, "expected integer"))
}
fn id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
    {
        return Err(err(ErrorCode::InvalidValue, "invalid review ID"));
    }
    Ok(())
}
fn text(value: &str) -> Result<()> {
    if value.len() > 512 || value.nfc().collect::<String>() != value {
        return Err(err(
            ErrorCode::InvalidValue,
            "text must be NFC and at most 512 bytes",
        ));
    }
    Ok(())
}
fn hex(value: &str, length: usize, name: &str) -> Result<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(err(
            ErrorCode::InvalidValue,
            format!("{name} must be lowercase hexadecimal"),
        ));
    }
    Ok(())
}
fn sha(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn canonical(value: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(json_error)
}
fn json_error(error: serde_json::Error) -> ReviewError {
    let text = error.to_string();
    let code = if text.contains("duplicate key") {
        ErrorCode::DuplicateKey
    } else {
        ErrorCode::JsonSyntax
    };
    err(code, text)
}

struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v)))
            }
            fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element::<StrictValue>()? {
                    out.push(v.0);
                }
                Ok(StrictValue(Value::Array(out)))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Map::new();
                while let Some((k, v)) = a.next_entry::<String, StrictValue>()? {
                    if out.insert(k.clone(), v.0).is_some() {
                        return Err(de::Error::custom(format!("duplicate key {k}")));
                    }
                }
                Ok(StrictValue(Value::Object(out)))
            }
        }
        d.deserialize_any(V)
    }
}

/// Emits a self-contained HTML viewer with no external assets, network requests, or server.
pub fn render_html(bundle: &ReviewBundle) -> Result<String> {
    let payload = field(
        object(&bundle.document, ErrorCode::RootShape, "root")?,
        "payload",
    )?;
    let frames = array(field(
        object(payload, ErrorCode::RootShape, "payload")?,
        "frames",
    )?)?;
    let max_nodes = frames
        .iter()
        .map(|frame| {
            let state = field(object(frame, ErrorCode::RootShape, "frame")?, "state")?;
            let systems = array(field(
                object(state, ErrorCode::RootShape, "state")?,
                "systems",
            )?)?;
            systems
                .iter()
                .map(|s| {
                    let o = object(s, ErrorCode::RootShape, "system")?;
                    Ok(8 + array(field(o, "planets")?)?.len() * 2
                        + array(field(o, "units")?)?.len() * 2)
                })
                .try_fold(0usize, |a, b: Result<usize>| b.map(|v| a + v))
        })
        .try_fold(0usize, |a, b: Result<usize>| b.map(|v| a.max(v)))?;
    if max_nodes > 100_000 {
        return Err(err(
            ErrorCode::LimitExceeded,
            "render would exceed 100,000 SVG/DOM nodes",
        ));
    }
    let data = String::from_utf8(canonical(&bundle.document)?)
        .map_err(|_| err(ErrorCode::InvalidUtf8, "canonical JSON is not UTF-8"))?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    Ok(format!(
        r##"<!doctype html><meta charset="utf-8"><title>TI4 review</title><style>*{{box-sizing:border-box}}body{{margin:0;background:#101521;color:#e9edf5;font:14px system-ui,sans-serif}}header{{padding:16px 22px;border-bottom:1px solid #2a3448;display:flex;justify-content:space-between}}main{{display:grid;grid-template-columns:1fr 320px;min-height:calc(100vh - 62px)}}#board{{width:100%;height:100%;min-height:640px;background:#151d2b}}aside{{border-left:1px solid #2a3448;padding:14px;overflow:auto}}.card{{background:#1c2636;border:1px solid #324158;border-radius:8px;padding:10px;margin:8px 0}}button{{background:#273650;color:#eef;border:1px solid #405678;border-radius:5px;padding:7px;margin:3px;cursor:pointer}}button.active{{background:#7c4dff}}.hex{{fill:#263b5c;stroke:#8fa6ce;stroke-width:2}}.centre{{fill:#59452d}}.home{{fill:#2d5a51}}.label{{fill:white;text-anchor:middle;font-size:12px}}.unit{{fill:#f5c36a;font-size:11px}}#timeline{{max-height:38vh;overflow:auto}}small{{color:#a9b7ce}}</style><header><strong>TI4 game review</strong><span id="meta"></span></header><main><svg id="board" viewBox="-520 -380 1040 760" role="img" aria-label="TI4 board"></svg><aside><div id="players"></div><h3>Timeline</h3><div id="timeline"></div><div class="card" id="facts"></div></aside></main><script id="bundle" type="application/json">{data}</script><script>const B=JSON.parse(document.querySelector('#bundle').textContent),F=B.payload.frames,T=B.payload.timeline,S=document.querySelector('#board');let n=0;const color=['#ef8354','#4ea5d9','#a6c36f','#d66ba0','#f6bd60','#8d99ae','#70c1b3','#ff9f1c'];document.querySelector('#meta').textContent=`${{B.manifest.audience.kind}} · ${{B.manifest.terminal.kind}} · ${{F.length}} frames`;function esc(x){{return String(x).replace(/[&<>]/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;'}}[c]))}}function hex(x,y){{let p=[];for(let i=0;i<6;i++){{let a=Math.PI/3*i+Math.PI/6;p.push(`${{x+62*Math.cos(a)}},${{y+62*Math.sin(a)}}`)}}return p.join(' ')}}function draw(){{let s=F[n].state;S.innerHTML='';for(let x of s.systems){{let px=104*x.q+52*x.r,py=90*x.r,k=x.kind==='centre'?' centre':x.kind==='home'?' home':'';S.insertAdjacentHTML('beforeend',`<g><polygon class="hex${{k}}" points="${{hex(px,py)}}"/><text class="label" x="${{px}}" y="${{py-10}}">${{esc(x.tile.label)}}</text><text class="label" x="${{px}}" y="${{py+8}}">${{x.planets.map(p=>esc(p.label)).join(' · ')}}</text>${{x.units.map((u,i)=>`<text class="unit" x="${{px-45}}" y="${{py+28+i*14}}">${{esc(u.owner)}} ${{u.count}}× ${{esc(u.kind)}}${{u.damaged?' ⚠':''}}</text>`).join('')}}</g>`);}}document.querySelector('#players').innerHTML='<h3>Players</h3>'+s.players.map((p,i)=>`<div class="card" style="border-left:4px solid ${{color[i]}}"><b>${{esc(p.name)}}</b> <small>${{esc(p.faction)}}</small><br>VP ${{p.score}} · R ${{p.resources}} · I ${{p.influence}} · TG ${{p.trade_goods}}</div>`).join('');document.querySelector('#timeline').innerHTML=T.map((t,i)=>`<button class="${{i+1===n?'active':''}}" onclick="go(${{i+1}})">${{i+1}}. ${{t.kind}}</button>`).join('');let t=n?T[n-1]:null;document.querySelector('#facts').innerHTML=`<b>Round ${{s.round}} · ${{s.phase}}</b><br>${{t?t.facts.map(f=>`${{esc(f.label)}}: ${{esc(f.value)}}`).join('<br>'):'Initial state'}}`}}function go(i){{n=i;draw()}}draw();</script>"##
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn example_validates() {
        let bytes = canonical_example().expect("example");
        assert!(validate_bytes(&bytes).is_ok());
    }
    #[test]
    fn duplicate_key_is_rejected() {
        let bytes = br#"{"manifest":{},"manifest":{},"payload":{}}"#;
        assert_eq!(
            validate_bytes(bytes).expect_err("duplicate").code,
            ErrorCode::DuplicateKey
        );
    }
    #[test]
    fn rendered_page_has_svg() {
        let b = validate_bytes(&canonical_example().expect("example")).expect("read");
        assert!(render_html(&b).expect("html").contains("<svg"));
    }
}
