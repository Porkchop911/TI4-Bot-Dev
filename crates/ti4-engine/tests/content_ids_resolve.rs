//! Every content id hardcoded in engine production code must exist in the corpus.
//!
//! `relics.rs` guarded the Stellar Converter with `alias == "mecatol_rex"`. No planet has that id
//! -- Mecatol Rex is `mr` in the base game and `mrte` in Thunder's Edge -- so the guard was dead
//! from the day it was written and the relic could destroy Mecatol. Nothing failed, because a
//! comparison against a string that matches nothing is not an error, it is just always false.
//!
//! That is a class, not an incident: a literal id is unchecked at compile time and silent at
//! runtime. This test is the check. It scans production code for lines that handle content ids and
//! asserts every id-shaped literal on them resolves.
//!
//! # Scope, and why it is narrow
//!
//! Only lines that already mention an id accessor or constructor are scanned. The engine is full
//! of lowercase string literals that are not content ids -- option kinds, payload keys, decision
//! names -- and scanning every literal would drown the signal in vocabulary that is correct by
//! construction. Proximity keeps the false-positive rate at zero while still catching the
//! comparison form the Stellar Converter used.
//!
//! Test modules are skipped: a fixture may legitimately name something the corpus does not carry.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;

/// Lines that touch a content id in a way a wrong literal would silently break.
const ID_CONTEXTS: [&str; 6] = [
    "PlanetId::new(",
    "SystemId::new(",
    ".id()",
    "alias",
    "planets_in(",
    "home_system(",
];

/// Literals on those lines that are engine vocabulary rather than corpus ids.
///
/// Each is a name the engine defines for itself, so the corpus cannot be expected to carry it.
/// Anything not here and not in the corpus is either a typo or a dead branch.
const ENGINE_VOCABULARY: [&str; 17] = [
    "decline",
    "activate",
    "system",
    "planet",
    "build",
    "produce",
    "placement",
    "component",
    "expedition",
    "discard",
    "colour",
    "annex",
    "structure",
    "unknown",
    // Payload keys and option kinds, not corpus identities.
    "card",
    "return",
    "alias",
];

/// Record accessors whose argument is a JSON field name, not a content id.
///
/// `record.text("alias")` asks for the field called `alias`; the corpus is not expected to carry
/// a record *named* "alias". Without this the check drowns in field names.
const FIELD_ACCESSORS: [&str; 9] = [
    "text(", "strings(", "int(", "float(", "flag(", "raw(", "number(", "with(", "payload_string(",
];

fn used_as_field_name(line: &str, literal: &str) -> bool {
    let needle = format!("\"{literal}\"");
    line.match_indices(&needle).any(|(at, _)| {
        let before = &line[..at];
        FIELD_ACCESSORS
            .iter()
            .any(|accessor| before.ends_with(accessor) || before.ends_with(&format!("{accessor} ")))
    })
}

fn is_id_shaped(token: &str) -> bool {
    token.len() >= 3
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && token.chars().any(|c| c.is_ascii_alphabetic())
}

/// Literals on one line, in source order.
fn literals(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let mut j = i + 1;
            let mut value = String::new();
            while j < bytes.len() && bytes[j] != '"' {
                if bytes[j] == '\\' {
                    j += 1;
                }
                if j < bytes.len() {
                    value.push(bytes[j]);
                }
                j += 1;
            }
            found.push(value);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    found
}

fn source_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            source_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

#[test]
fn every_hardcoded_content_id_resolves_in_the_corpus() {
    let content = ContentStore::embedded();
    // Every identity the corpus carries, across every category: a hardcoded id may name a relic,
    // a technology or a breakthrough as legitimately as a planet, and checking only two
    // categories would report the other twenty-six as dead.
    let mut known: BTreeSet<String> = BTreeSet::new();
    for category in <ti4_model::content_types::ContentType as strum::IntoEnumIterator>::iter() {
        for record in content.records(category) {
            if let Some(id) = record.id() {
                known.insert(id.to_owned());
            }
            if let Some(alias) = record.text("alias") {
                known.insert(alias.to_owned());
            }
            for alias in record.strings("aliases") {
                known.insert(alias.to_owned());
            }
        }
    }
    for planet in ti4_content::galaxy::all_planets(content, FULL).into_keys() {
        known.insert(planet.to_owned());
    }
    for system in ti4_content::galaxy::all_systems(content, FULL).into_keys() {
        known.insert(system.to_owned());
    }
    let vocabulary: BTreeSet<&str> = ENGINE_VOCABULARY.into_iter().collect();

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&root, &mut files);
    assert!(!files.is_empty(), "no engine sources found under {root:?}");

    let mut dead: Vec<String> = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        // Tests sit at the end of a file in this codebase; a fixture may name something the
        // corpus does not carry, and that is its business.
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        for (number, line) in production.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if !ID_CONTEXTS.iter().any(|needle| line.contains(needle)) {
                continue;
            }
            for literal in literals(line) {
                if !is_id_shaped(&literal)
                    || vocabulary.contains(literal.as_str())
                    || known.contains(&literal)
                    || used_as_field_name(line, &literal)
                {
                    continue;
                }
                dead.push(format!("{name}:{}  {literal:?}", number + 1));
            }
        }
    }

    assert!(
        dead.is_empty(),
        "content ids that match nothing in the corpus -- a comparison against one is always \
         false and a construction of one names a planet or system that does not exist:\n  {}",
        dead.join("\n  ")
    );
}
