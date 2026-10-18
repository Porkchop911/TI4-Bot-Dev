//! The TypeScript types of the wire, generated from the Rust types with `ts-rs` (feature `ts`).
//!
//! Both pages read them from `web-shared/src/wire/generated/`: one file for each type, and
//! `index.ts` with every type and [`PROTOCOL_VERSION`]. The files are checked in. The test
//! `wire_types::generated_wire_types_are_current` fails when they differ from what this module
//! writes, and `cargo wire-web` writes them again.
//!
//! The roots are what a page sends and reads: `ClientMessage` and `ServerMessage` of
//! `ti4_host::protocol`, the catalogue, the setup of a table (its options and preview as the
//! server and the engine give them, its draw, and the game request of a start), the odds of a
//! space and of a ground combat, the standing of a fleet, the save of a game, and the look-ahead
//! details of the strategy-card questions, of a transaction and of the texts that name ids
//! (`Choice.details` is untyped JSON, so each struct that the engine writes there with
//! `Choice::hinted` is a root). Every type that they name is written with them.

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use ti4_host::protocol::PROTOCOL_VERSION;
use ts_rs::{Config, TS, TypeVisitor};

/// The directory of the generated files, from the root of the repository.
pub const GENERATED_DIR: &str = "web-shared/src/wire/generated";

/// The file of the index: every type, and the protocol version.
const INDEX: &str = "index.ts";

/// How the types are written: `u64` is a JSON number, and an import names its file with `.ts`,
/// as both pages import.
fn config() -> Config {
    Config::new()
        .with_large_int("number")
        .with_import_extension(Some("ts"))
}

/// Every generated file, by its name in [`GENERATED_DIR`], with its text.
///
/// # Panics
/// When two types of the closure have the same TypeScript name (rename one with
/// `#[ts(rename = "...")]`), or ts-rs cannot write one.
#[must_use]
pub fn files() -> BTreeMap<String, String> {
    let mut collect = Collect {
        cfg: config(),
        seen: HashSet::new(),
        files: BTreeMap::new(),
        names: BTreeMap::new(),
    };
    collect.visit::<ti4_host::protocol::ClientMessage>();
    collect.visit::<ti4_host::protocol::ServerMessage>();
    collect.visit::<ti4_host::view::components::Catalog>();
    collect.visit::<ti4_host::setup::SetupOptions>();
    collect.visit::<crate::TableOptions>();
    collect.visit::<crate::OptionsRequest>();
    collect.visit::<ti4_host::maps::MapPreview>();
    collect.visit::<crate::TablePreview>();
    collect.visit::<crate::TableDraw>();
    collect.visit::<crate::host_setup::GameRequest>();
    collect.visit::<crate::SpaceOddsRequest>();
    collect.visit::<ti4_calc::odds::BattleResponse>();
    collect.visit::<ti4_calc::odds::GroundOddsRequest>();
    collect.visit::<ti4_calc::odds::GroundOddsResponse>();
    collect.visit::<ti4_engine::fleet::Standing>();
    collect.visit::<ti4_engine::fleet::FleetSupplyHints>();
    collect.visit::<ti4_engine::production::CapacityHints>();
    // ts-rs inlines a flattened struct, so the shared part is a root of its own.
    collect.visit::<ti4_engine::strategy_cards::PaymentHints>();
    collect.visit::<ti4_engine::strategy_cards::ResearchPaymentHints>();
    collect.visit::<ti4_engine::strategy_cards::PurchaseHints>();
    collect.visit::<ti4_engine::strategy_cards::StructureHints>();
    collect.visit::<ti4_engine::tokens::RedistributionHints>();
    collect.visit::<ti4_engine::strategy_cards::ReadyHints>();
    collect.visit::<ti4_engine::strategy_cards::PoliticsHints>();
    collect.visit::<ti4_engine::strategy_cards::AgendaPlacementHints>();
    collect.visit::<ti4_engine::strategy_cards::TradeReplenishHints>();
    collect.visit::<ti4_engine::strategy_cards::CommodityStanding>();
    collect.visit::<ti4_engine::strategy_cards::ImperialHints>();
    collect.visit::<ti4_engine::transactions::TransactionHints>();
    collect.visit::<ti4_engine::phrases::PhraseHints>();
    collect.visit::<crate::host_session::HostSave>();
    let mut index = format!("{NOTE}\n");
    for name in collect.names.keys() {
        let _ = writeln!(index, "export type {{ {name} }} from \"./{name}.ts\";");
    }
    let _ = writeln!(
        index,
        "\n/** `ti4_host::protocol::PROTOCOL_VERSION`: the version of the messages of this build. */\nexport const PROTOCOL_VERSION = {PROTOCOL_VERSION};"
    );
    collect.files.insert(INDEX.to_owned(), index);
    collect.files
}

/// The first line of every file.
const NOTE: &str =
    "// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.\n";

/// Writes [`files`] into `dir`, and removes every other `.ts` file there.
///
/// # Errors
/// A failed read, write or removal.
pub fn write(dir: &Path) -> std::io::Result<()> {
    let files = files();
    std::fs::create_dir_all(dir)?;
    for name in ts_files(dir)? {
        if !files.contains_key(&name) {
            std::fs::remove_file(dir.join(name))?;
        }
    }
    for (name, text) in &files {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
}

/// The files of `dir` that differ from [`files`]: changed, missing or not generated.
///
/// # Errors
/// A failed read of the directory.
pub fn stale(dir: &Path) -> std::io::Result<Vec<String>> {
    let files = files();
    let mut stale = Vec::new();
    let present = if dir.is_dir() {
        ts_files(dir)?
    } else {
        Vec::new()
    };
    for name in &present {
        if !files.contains_key(name) {
            stale.push(format!("{name} (not generated)"));
        }
    }
    for (name, text) in &files {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(old) if old == *text => {}
            Ok(_) => stale.push(format!("{name} (changed)")),
            Err(_) => stale.push(format!("{name} (missing)")),
        }
    }
    Ok(stale)
}

fn ts_files(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".ts") {
            names.push(name);
        }
    }
    Ok(names)
}

struct Collect {
    cfg: Config,
    seen: HashSet<TypeId>,
    files: BTreeMap<String, String>,
    /// The TypeScript name of each type, with the Rust type that has it.
    names: BTreeMap<String, &'static str>,
}

impl TypeVisitor for Collect {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        let Some(path) = T::output_path() else {
            return;
        };
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        let name = T::ident(&self.cfg);
        let rust = std::any::type_name::<T>();
        if let Some(other) = self.names.insert(name.clone(), rust) {
            panic!("{rust} and {other} are both `{name}` in TypeScript: rename one");
        }
        let text = T::export_to_string(&self.cfg)
            .unwrap_or_else(|error| panic!("{rust} is not written: {error}"));
        let text = optional_not_null(&text.replacen(
            "// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.\n",
            NOTE,
            1,
        ));
        self.files.insert(path.to_string_lossy().into_owned(), text);
        T::visit_dependencies(self);
    }
}

/// `text` with `name?: T` for each `name?: T | null` that ts-rs writes. ts-rs writes an `Option`
/// field that serde leaves out when it is `None` (`default` and `skip_serializing_if`) as
/// `name?: T | null`, but such a field is never `null` in a message: it is there or not.
fn optional_not_null(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            i = text[i..].find("*/").map_or(bytes.len(), |end| i + end + 2);
        } else if bytes[i] == b'"' {
            i = text[i + 1..]
                .find('"')
                .map_or(bytes.len(), |end| i + end + 2);
        } else if bytes[i..].starts_with(b"?: ") {
            let start = i + 3;
            let end = type_end(text, start);
            out.push_str(&text[copied..start]);
            let field = optional_not_null(&text[start..end]);
            out.push_str(field.strip_suffix(" | null").unwrap_or(&field));
            copied = end;
            i = end;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// Where the type of a field that starts at `start` ends: at the `,` or the `}` after it, outside
/// of brackets, comments and strings.
fn type_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes[i..].starts_with(b"/*") => {
                i = text[i..].find("*/").map_or(bytes.len(), |end| i + end + 2);
                continue;
            }
            b'"' => {
                i = text[i + 1..]
                    .find('"')
                    .map_or(bytes.len(), |end| i + end + 2);
                continue;
            }
            b'{' | b'[' | b'(' | b'<' => depth += 1,
            b'}' | b']' | b')' | b'>' if depth > 0 => depth -= 1,
            b',' | b'}' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::optional_not_null;

    #[test]
    fn an_optional_field_is_not_null() {
        assert_eq!(
            optional_not_null(
                "export type A = { a?: B | null, b: C | null, c?: { d?: Array<E | null> | null, }, \"e?: f | null\": G, };"
            ),
            "export type A = { a?: B, b: C | null, c?: { d?: Array<E | null>, }, \"e?: f | null\": G, };"
        );
        assert_eq!(
            optional_not_null("type A = { /** x?: y | null, */ a?: B | null, };"),
            "type A = { /** x?: y | null, */ a?: B, };"
        );
    }
}
