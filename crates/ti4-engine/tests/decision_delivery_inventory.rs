//! Source-level gate for OBS-002a's decision-delivery inventory.
//!
//! `Choice` does not yet carry typed source/subtype metadata, so this scanner tokenises production
//! Rust source, associates constructors and delivery calls with their enclosing function, and
//! checks them against a reviewed registry. Comments, strings and whitespace cannot create or hide
//! a site. A real Rust parser will replace this gate when typed `DecisionContext` lands.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Site {
    module: String,
    function: String,
    operation: Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Operation {
    Choice,
    AskViewless,
    AskObserved,
    ChooseDirect,
    ChooseObservedDirect,
}

fn production_source(path: &Path) -> String {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    source
        .split_once("\n#[cfg(test)]")
        .map_or(source.as_str(), |(production, _)| production)
        .to_owned()
}

/// Enough of Rust's lexical grammar for structural call-site discovery.
///
/// It intentionally discards comments and literal contents. Identifiers, braces, dots, parentheses
/// and `::` remain, which are the only tokens this audit consumes.
fn tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            at += 2;
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1usize;
            while at < bytes.len() && depth > 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
            assert_eq!(depth, 0, "unterminated block comment");
            continue;
        }
        if bytes[at] == b'r' {
            let mut quote = at + 1;
            while quote < bytes.len() && bytes[quote] == b'#' {
                quote += 1;
            }
            if quote < bytes.len() && bytes[quote] == b'"' {
                let hashes = quote - at - 1;
                at = quote + 1;
                loop {
                    assert!(at < bytes.len(), "unterminated raw string");
                    if bytes[at] == b'"'
                        && bytes.get(at + 1..at + 1 + hashes) == Some(&vec![b'#'; hashes])
                    {
                        at += 1 + hashes;
                        break;
                    }
                    at += 1;
                }
                continue;
            }
        }
        if bytes[at] == b'"' {
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == b'"' {
                    at += 1;
                    break;
                } else {
                    at += 1;
                }
            }
            continue;
        }
        if bytes[at] == b'\'' {
            // One-byte and escaped character literals; otherwise this is a lifetime apostrophe.
            if bytes.get(at + 2) == Some(&b'\'') {
                at += 3;
                continue;
            }
            if bytes.get(at + 1) == Some(&b'\\') {
                let mut end = at + 2;
                while end < bytes.len() && bytes[end] != b'\'' {
                    end += 1;
                }
                assert!(end < bytes.len(), "unterminated escaped char literal");
                at = end + 1;
                continue;
            }
        }
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            out.push(source[start..at].to_owned());
            continue;
        }
        if bytes[at..].starts_with(b"::") {
            out.push("::".to_owned());
            at += 2;
            continue;
        }
        out.push(char::from(bytes[at]).to_string());
        at += 1;
    }
    out
}

fn source_files() -> Vec<PathBuf> {
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = fs::read_dir(source_dir)
        .expect("read engine source directory")
        .map(|entry| entry.expect("source entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
        .collect();
    files.sort();
    files
}

fn scan() -> BTreeMap<Site, usize> {
    let mut sites = BTreeMap::new();
    for path in source_files() {
        let module = path
            .file_name()
            .expect("source file name")
            .to_string_lossy()
            .into_owned();
        let tokens = tokens(&production_source(&path));
        let mut depth = 0usize;
        let mut awaiting_name = false;
        let mut pending_function: Option<String> = None;
        let mut functions: Vec<(String, usize)> = Vec::new();
        for index in 0..tokens.len() {
            let token = &tokens[index];
            if token == "fn" {
                awaiting_name = true;
            } else if awaiting_name
                && token
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            {
                pending_function = Some(token.clone());
                awaiting_name = false;
            }

            if token == "{" {
                depth += 1;
                if let Some(function) = pending_function.take() {
                    functions.push((function, depth));
                }
            } else if token == "}" {
                if functions
                    .last()
                    .is_some_and(|(_, entered)| *entered == depth)
                {
                    functions.pop();
                }
                depth = depth.saturating_sub(1);
            } else if token == ";" {
                pending_function = None;
                awaiting_name = false;
            }

            let operation = if sequence(&tokens, index, &["Choice", "::", "new", "("]) {
                Some(Operation::Choice)
            } else if sequence(&tokens, index, &[".", "ask", "("]) {
                Some(Operation::AskViewless)
            } else if sequence(&tokens, index, &[".", "ask_seeing", "("]) {
                Some(Operation::AskObserved)
            } else if sequence(&tokens, index, &[".", "choose", "("]) {
                Some(Operation::ChooseDirect)
            } else if sequence(&tokens, index, &[".", "choose_seeing", "("]) {
                Some(Operation::ChooseObservedDirect)
            } else {
                None
            };
            if let Some(operation) = operation {
                let function = functions
                    .last()
                    .map_or_else(|| "<module>".to_owned(), |(function, _)| function.clone());
                *sites
                    .entry(Site {
                        module: module.clone(),
                        function,
                        operation,
                    })
                    .or_default() += 1;
            }
        }
    }
    sites
}

fn sequence(tokens: &[String], at: usize, expected: &[&str]) -> bool {
    tokens.get(at..at + expected.len()).is_some_and(|actual| {
        actual
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
    })
}

#[derive(Debug, Clone, Copy)]
enum Delivery {
    ObservedHere,
    ViewlessHere,
    ObservedVia(&'static str),
    /// Retained while `timing::pick` is still viewless. No producer reaches it indirectly today,
    /// but the registry must be able to say so if one is found, and removing the variant would
    /// force the next reviewer to reintroduce it before they could record what they had found.
    #[expect(dead_code, reason = "the vocabulary outlives the current inventory")]
    ViewlessVia(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct Producer {
    module: &'static str,
    function: &'static str,
    count: usize,
    delivery: Delivery,
}

const PRODUCERS: &[Producer] = &[
    Producer {
        module: "action_cards.rs",
        function: "choose_crashlanding_ground",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "choose_crashlanding_planet",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "confusing",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "enforce_hand_limit",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "exchange_program",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "ghost_squad",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "in_the_silence_of_space",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "pick",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "predicted_outcome",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "public_disgrace",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "reparations",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "skilled_retreat",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "agenda_effects.rs",
        function: "ask_the_speaker",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "agenda_effects.rs",
        function: "choose_structure",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "agenda_effects.rs",
        function: "resolve_with",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "choose_casualty",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "choose_reroll_dice",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "heart_ixth",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "offer_sustain",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "pending_choice",
        count: 4,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath"),
    },
    Producer {
        module: "draft.rs",
        function: "strategy_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        module: "exploration.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "faction_abilities.rs",
        function: "perform_component",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "faction_abilities.rs",
        function: "space_combat_round_started",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "faction_abilities.rs",
        function: "strategy_resolved",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "fleet.rs",
        function: "remove_one",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "game.rs",
        function: "action_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        module: "game.rs",
        function: "committee_formation",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "game.rs",
        function: "imperial_arbiter",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "game.rs",
        function: "minister_of_war",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "invasion.rs",
        function: "absorb_ground",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "invasion.rs",
        function: "bombardment_target_question",
        count: 1,
        delivery: Delivery::ObservedVia("invasion.rs::apply_bombard_plan"),
    },
    Producer {
        module: "invasion.rs",
        function: "commit_ground_forces",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "invasion.rs",
        function: "committing_choice",
        count: 1,
        delivery: Delivery::ObservedVia("invasion.rs::drive"),
    },
    Producer {
        module: "invasion.rs",
        function: "dunlain_reaper",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "invasion.rs",
        function: "pending_choice",
        count: 3,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath"),
    },
    Producer {
        module: "laws.rs",
        function: "offer_discard",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "objectives.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_scoring"),
    },
    Producer {
        module: "production.rs",
        function: "integrated_economy",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "production.rs",
        function: "pay_with_observation_credit",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "production.rs",
        function: "pending_choice",
        count: 3,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath"),
    },
    Producer {
        module: "production.rs",
        function: "produce_one",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "production.rs",
        function: "sling_relay",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "reactions.rs",
        function: "slot",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "codex",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "crown_of_emphidia_explore",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "grant_chosen_technology",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "neuraloop",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "offer_dominus_orb",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "stellar_converter",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "relics.rs",
        function: "titan_prototype",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "secrets.rs",
        function: "enforce_hand_limit",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "strategy.rs",
        function: "secondary_choice",
        count: 3,
        delivery: Delivery::ObservedVia("game.rs::step_secondary"),
    },
    Producer {
        module: "strategy.rs",
        function: "strategic_action_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "diplomacy_primary",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "doctor_sucaban",
        count: 2,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "gain_tokens",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "imperial_primary",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "influence_purchase_choice",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "offer_research",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "paid_research",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "place_structure",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "politics_primary",
        count: 2,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "primary",
        count: 2,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "ready_planets",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "redistribute_tokens",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "specialist_compounds",
        count: 2,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "trade_primary",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "warfare_primary",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "tactical.rs",
        function: "activation_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        module: "tactical.rs",
        function: "movement_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_tactical"),
    },
    Producer {
        module: "technology.rs",
        function: "end_turn",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "technology.rs",
        function: "start_turn",
        count: 3,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "thunders_edge.rs",
        function: "pay",
        count: 2,
        delivery: Delivery::ObservedVia("thunders_edge.rs::ask_seeing"),
    },
    Producer {
        module: "timing.rs",
        function: "pick",
        count: 1,
        delivery: Delivery::ViewlessHere,
    },
    Producer {
        module: "timing.rs",
        function: "pick_with_context",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "tokens.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_token_gain"),
    },
    Producer {
        module: "transactions.rs",
        function: "pending_choice",
        count: 2,
        delivery: Delivery::ObservedVia("game.rs::step_trade"),
    },
    Producer {
        module: "transit.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_tactical"),
    },
    Producer {
        module: "vote.rs",
        function: "pending_choice",
        count: 3,
        delivery: Delivery::ObservedVia("game.rs::step_vote"),
    },
];

const OBSERVED_ASKS: &[(&str, &str, usize)] = &[
    ("action_cards.rs", "choose_crashlanding_ground", 1),
    ("action_cards.rs", "choose_crashlanding_planet", 1),
    ("action_cards.rs", "confusing", 1),
    ("action_cards.rs", "enforce_hand_limit", 1),
    ("action_cards.rs", "exchange_program", 1),
    ("action_cards.rs", "ghost_squad", 1),
    ("action_cards.rs", "in_the_silence_of_space", 1),
    ("action_cards.rs", "pick", 1),
    ("action_cards.rs", "predicted_outcome", 1),
    ("action_cards.rs", "public_disgrace", 1),
    ("action_cards.rs", "reparations", 2),
    ("action_cards.rs", "skilled_retreat", 1),
    ("agenda_effects.rs", "ask_the_speaker", 1),
    ("agenda_effects.rs", "choose_structure", 1),
    ("agenda_effects.rs", "resolve_with", 1),
    ("choice.rs", "ask_seeing", 1),
    ("choice.rs", "drive", 1),
    ("combat.rs", "choose_casualty", 1),
    ("combat.rs", "choose_reroll_dice", 1),
    ("combat.rs", "heart_ixth", 1),
    ("combat.rs", "offer_sustain", 1),
    ("exploration.rs", "ask", 1),
    ("faction_abilities.rs", "perform_component", 2),
    ("faction_abilities.rs", "space_combat_round_started", 1),
    ("faction_abilities.rs", "strategy_resolved", 1),
    ("fleet.rs", "remove_one", 1),
    ("game.rs", "step", 1),
    ("game.rs", "step_aftermath", 1),
    ("game.rs", "step_event_scoring", 1),
    ("game.rs", "step_scoring", 1),
    ("game.rs", "step_secondary", 1),
    ("game.rs", "step_tactical", 1),
    ("game.rs", "step_token_gain", 1),
    ("game.rs", "step_trade", 1),
    ("game.rs", "step_vote", 1),
    ("game.rs", "committee_formation", 1),
    ("game.rs", "imperial_arbiter", 2),
    ("game.rs", "minister_of_war", 1),
    ("invasion.rs", "absorb_ground", 1),
    ("invasion.rs", "commit_ground_forces", 1),
    ("invasion.rs", "drive", 1),
    ("production.rs", "integrated_economy", 1),
    ("production.rs", "pay_with_observation_credit", 1),
    ("production.rs", "produce_one", 2),
    ("production.rs", "resolve", 1),
    ("production.rs", "sling_relay", 2),
    ("reactions.rs", "slot", 1),
    ("relics.rs", "codex", 1),
    ("relics.rs", "crown_of_emphidia_explore", 1),
    ("relics.rs", "grant_chosen_technology", 1),
    ("relics.rs", "offer_dominus_orb", 1),
    ("relics.rs", "stellar_converter", 1),
    ("relics.rs", "titan_prototype", 1),
    ("invasion.rs", "apply_bombard_plan", 1),
    ("invasion.rs", "dunlain_reaper", 1),
    ("laws.rs", "offer_discard", 1),
    ("relics.rs", "neuraloop", 1),
    ("secrets.rs", "enforce_hand_limit", 1),
    ("strategy_cards.rs", "ask", 1),
    ("technology.rs", "end_turn", 2),
    ("technology.rs", "start_turn", 3),
    ("thunders_edge.rs", "ask_seeing", 1),
    ("timing.rs", "ask_seeing", 1),
    ("timing.rs", "pick_with_context", 1),
];

const VIEWLESS_ASKS: &[(&str, &str, usize)] = &[("timing.rs", "pick", 1)];

fn expected_sites() -> BTreeMap<Site, usize> {
    let mut expected = BTreeMap::new();
    for producer in PRODUCERS {
        expected.insert(
            Site {
                module: producer.module.to_owned(),
                function: producer.function.to_owned(),
                operation: Operation::Choice,
            },
            producer.count,
        );
    }
    for &(module, function, count) in OBSERVED_ASKS {
        expected.insert(
            Site {
                module: module.to_owned(),
                function: function.to_owned(),
                operation: Operation::AskObserved,
            },
            count,
        );
    }
    for &(module, function, count) in VIEWLESS_ASKS {
        expected.insert(
            Site {
                module: module.to_owned(),
                function: function.to_owned(),
                operation: Operation::AskViewless,
            },
            count,
        );
    }
    for (function, operation, count) in [
        ("ask", Operation::ChooseDirect, 1),
        ("ask_private", Operation::ChooseObservedDirect, 1),
        ("ask_seeing", Operation::ChooseObservedDirect, 1),
        ("choose", Operation::ChooseDirect, 2),
        ("choose_seeing", Operation::ChooseDirect, 1),
    ] {
        expected.insert(
            Site {
                module: "choice.rs".to_owned(),
                function: function.to_owned(),
                operation,
            },
            count,
        );
    }
    expected
}

fn delivery_site(target: &str, operation: Operation) -> Site {
    let (module, function) = target
        .split_once("::")
        .expect("module::function delivery target");
    Site {
        module: module.to_owned(),
        function: function.to_owned(),
        operation,
    }
}

#[test]
fn every_producer_and_delivery_site_matches_the_reviewed_registry() {
    assert_eq!(scan(), expected_sites());
}

#[test]
fn every_indirect_producer_reaches_its_classified_delivery_api() {
    let actual = scan();
    for producer in PRODUCERS {
        let target = match producer.delivery {
            Delivery::ObservedHere => Site {
                module: producer.module.to_owned(),
                function: producer.function.to_owned(),
                operation: Operation::AskObserved,
            },
            Delivery::ViewlessHere => Site {
                module: producer.module.to_owned(),
                function: producer.function.to_owned(),
                operation: Operation::AskViewless,
            },
            Delivery::ObservedVia(target) => delivery_site(target, Operation::AskObserved),
            Delivery::ViewlessVia(target) => delivery_site(target, Operation::AskViewless),
        };
        assert!(
            actual.contains_key(&target),
            "unclassified delivery for {producer:?}: expected {target:?}"
        );
    }
}

#[test]
fn the_remaining_viewless_asks_stay_explicit_migration_work() {
    // Both halves matter, and only the second one measures the engine.
    //
    // Summing the registry is a ratchet: on its own it cannot fail, because it asserts a constant
    // against a literal. It still does useful work, because a new viewless ask first trips the
    // scan equality in `every_producer_and_delivery_site_matches_the_reviewed_registry`, the
    // registry must then be edited to restore it, and that edit trips this. But a reader could
    // easily mistake it for a check on the engine, so the scanned total is asserted too.
    let count: usize = VIEWLESS_ASKS.iter().map(|(_, _, count)| count).sum();
    assert_eq!(count, 1, "the reviewed registry still names one");
    let scanned: usize = scan()
        .iter()
        .filter(|(site, _)| site.operation == Operation::AskViewless)
        .map(|(_, count)| count)
        .sum();
    assert_eq!(
        scanned, 1,
        "production still contains one viewless ask; the registry and the source agree"
    );
    assert!(PRODUCERS.iter().all(|producer| !matches!(
        producer.delivery,
        Delivery::ViewlessHere | Delivery::ViewlessVia(_)
    ) || producer.count > 0));
}

#[test]
fn no_engine_module_calls_a_decider_around_table() {
    let actual = scan();
    let offenders: Vec<&Site> = actual
        .keys()
        .filter(|site| {
            matches!(
                site.operation,
                Operation::ChooseDirect | Operation::ChooseObservedDirect
            ) && site.module != "choice.rs"
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "direct decider calls outside Table: {offenders:?}"
    );
}
