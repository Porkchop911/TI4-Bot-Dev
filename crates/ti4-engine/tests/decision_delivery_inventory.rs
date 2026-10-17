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
    source[..test_modules_start(&source)].to_owned()
}

/// Where the test modules of a file start: the first `#[cfg(test)]` that is on a `mod`. A
/// `#[cfg(test)]` on another item (a helper that only tests call) does not end the production
/// source, or every decision site below it would leave the audit unseen.
fn test_modules_start(source: &str) -> usize {
    const MARKER: &str = "\n#[cfg(test)]\n";
    let mut from = 0;
    while let Some(found) = source[from..].find(MARKER) {
        let at = from + found;
        let item = source[at + MARKER.len()..]
            .lines()
            .find(|line| !line.starts_with("#[") && !line.starts_with("//"))
            .unwrap_or("");
        let item = item.strip_prefix("pub(crate) ").unwrap_or(item);
        if item.starts_with("mod ") {
            return at;
        }
        from = at + MARKER.len();
    }
    source.len()
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
    let mut files = Vec::new();
    collect_rust_files(&source_dir, &mut files);
    files.sort();
    files
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read engine source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
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
        module: "invasion.rs",
        function: "assign_selected_ground_hit_in_timing",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "yssaril.rs",
        function: "borrowed_stall_tactics",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "argent.rs",
        function: "ask_option_for",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "sardakk.rs",
        function: "skip_to_commit",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "muaat.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "nekro.rs",
        function: "take_from",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "nekro_units.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naaz.rs",
        function: "ask_checked",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naaz.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naaz.rs",
        function: "borrowed_agent",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naaz.rs",
        function: "offer_supercharge",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "saar.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "agent_use",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "bioplasmosis",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "mitosis",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "perform_component",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "stymie",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "arborec.rs",
        function: "warrior_returns",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "ghost.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naalu.rs",
        function: "foresight",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "naalu.rs",
        function: "hero",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Yssaril breakthrough Deepgloom Executable: when another player draws action cards, each
        // holder of the breakthrough is asked whether the drawing player may use Scheming. The
        // holder sees the table, the requester does not.
        module: "yssaril.rs",
        function: "action_card_draw_requested",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "yssaril.rs",
        function: "action_cards_drawn",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "yssaril.rs",
        function: "commander_look",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "yssaril.rs",
        function: "kyver_decisions",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "mentak.rs",
        function: "ask_among",
        count: 1,
        delivery: Delivery::ObservedVia("mentak.rs::ask_choice"),
    },
    Producer {
        // The same, for a question whose labels name seats or notes by id (`details.phrases`).
        module: "mentak.rs",
        function: "ask_phrased",
        count: 1,
        delivery: Delivery::ObservedVia("mentak.rs::ask_choice"),
    },
    Producer {
        // Cabal: Vortex picks which reinforcement unit to capture (BF-cabal.md CB-06).
        module: "cabal.rs",
        function: "vortex_perform",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Cabal: The Stillness of Stars picks which reinforcement unit to capture (CB-07).
        module: "cabal.rs",
        function: "stillness_of_stars",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Cabal: Al'Raith Ix Ianovar moves up to two ingress tokens into gravity rifts (CB-11).
        module: "cabal.rs",
        function: "alraith",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Keleres (BF-keleres.md, BF-keleres-units.md).
        module: "keleres.rs",
        function: "laws_order",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Keleres agent: offered once per payment window (BF-keleres.md).
        module: "keleres.rs",
        function: "offer_agent",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Keleres (BF-keleres.md, BF-keleres-units.md).
        module: "keleres_units.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Keleres (BF-keleres.md, BF-keleres-units.md).
        module: "production.rs",
        function: "agency_supply_network",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Last Bastion: which unit to galvanize (BF-bastion.md); every ask goes through this one.
        module: "bastion.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Mahact: Edict opponent, Scepter system (BF-mahact.md).
        module: "mahact.rs",
        function: "ask_among",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Mahact units: Crimson Legionnaire, Starlancer, Airo Shir Aur (BF-mahact-units.md).
        module: "mahact_units.rs",
        function: "legionnaire",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "mahact_units.rs",
        function: "legionnaire_returns",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "mahact_units.rs",
        function: "offer_starlancer",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "mahact_units.rs",
        function: "use_leader_timed",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Empyrean: Void Tether placement (BF-empyrean.md).
        module: "empyrean.rs",
        function: "tether_place",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Empyrean: Voidwatch, the mover gives a promissory note (BF-empyrean.md).
        module: "empyrean.rs",
        function: "voidwatch",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Empyrean units: Watcher's mech, Dynamo, notes (BF-empyrean-units.md).
        module: "empyrean_units.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Nomad: Future Sight, Thunder's Paradox, hero (BF-nomad.md).
        module: "nomad.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Nomad agents: Mercer's removals and planet, The Cavalry's ship (BF-nomad-agents.md).
        module: "nomad_agents.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Slumberstate Computing: fight or coexist when Coalescence forces a ground combat.
        module: "invasion.rs",
        function: "coexist_instead",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Deepwrought Research Team: fight or coexist for each committed planet (BF-deepwrought.md).
        module: "invasion.rs",
        function: "offer_research_team",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Deepwrought: every question asked inside a timing window or component action (BF-deepwrought.md).
        module: "deepwrought.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "deepwrought.rs",
        function: "ask_about",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // `ask` for a question whose texts name seats or notes by id (`details.phrases`).
        module: "deepwrought.rs",
        function: "ask_phrased",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Crimson Rebellion: every question asked inside a timing window or component action
        // (BF-crimson.md). Ahk Ravin's transport questions are the shared cargo hold's own.
        module: "crimson.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "crimson.rs",
        function: "ask_about",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Ral Nel: every question asked inside a timing window or component action (BF-ralnel.md).
        module: "ralnel.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Doctor Carrina, in the one window every research route opens: the holder's exhaust
        // offer, then the infantry placement (BF-deepwrought.md).
        module: "deepwrought_research.rs",
        function: "open",
        count: 1,
        delivery: Delivery::ObservedVia("deepwrought_research.rs::put"),
    },
    Producer {
        module: "deepwrought_research.rs",
        function: "settle",
        count: 1,
        delivery: Delivery::ObservedVia("deepwrought_research.rs::put"),
    },
    Producer {
        // Research Team on defense: the Deepwrought defender chooses to coexist (BF-deepwrought.md).
        module: "invasion.rs",
        function: "offer_research_team_defense",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Slumberstate Computing: the Titans pick a planet, its controller allows the sleeper.
        module: "titans_leaders.rs",
        function: "sleeper_allowance",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Titans: Terragenesis, Awaken and Ouranos questions share one helper (BF-titans.md).
        module: "titans.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Crimson commander: gain a commodity or convert one (BF-COMMANDERS-ALLIANCE-OCT5).
        module: "borrowed_commanders.rs",
        function: "crimson_ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Ral Nel commander: retreat destination and ships (BF-COMMANDERS-ALLIANCE-OCT5).
        module: "borrowed_commanders.rs",
        function: "ralnel_ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "argent.rs",
        function: "afb_excess",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "argent.rs",
        function: "extra_die_effect",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "argent.rs",
        function: "strike_wing_effect",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "winnu.rs",
        function: "htp",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "winnu.rs",
        function: "imperator",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "winnu.rs",
        function: "reclaimer",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Mathis Mathinus's follower question, built for `leader_strategy_follower_choices`
        // (asked by `game.rs::step_leader_followers`) and for `use_leader`, which asks it.
        module: "winnu.rs",
        function: "mathis_follower_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_leader_followers"),
    },
    Producer {
        module: "winnu.rs",
        function: "use_leader_strategy_primary",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "winnu.rs",
        function: "use_leader",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "sardakk.rs",
        function: "exotrireme",
        count: 2,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "sardakk.rs",
        function: "supremacy",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "yin.rs",
        function: "ask_one",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "place_units_choosing",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    // BF-00h-cards: hidden-hand choices for faction effects.
    Producer {
        module: "action_cards.rs",
        function: "choose_from_own_hand",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "take_from_revealed_hand",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
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
        function: "pick_detailed",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "action_cards.rs",
        function: "pick_rider_target",
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
        // Wrath of Kenara: how many near-miss dice to buy +1 on, or decline. The question sits in
        // the purchase half, inside the Keleres agent's goods window (`wrath_of_kenara` opens it).
        module: "combat.rs",
        function: "wrath_of_kenara_buy",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "choose_casualty_owing",
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
        // War Funding: offered to the holder after both sides have rolled a combat round.
        module: "combat.rs",
        function: "roll_round",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "combat.rs",
        function: "pending_choice",
        count: 4,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath_inner"),
    },
    Producer {
        module: "draft.rs",
        function: "strategy_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        // Entropic Scar rule 6: spend a strategy token at the start of the status phase to gain a
        // faction technology. Options are the unowned faction technologies, each carrying the
        // technology id and the token cost, plus a decline; asked once per available grant.
        module: "entropic_scars.rs",
        function: "resolve_status_start",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "exploration.rs",
        function: "ask_decorated",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "exploration.rs",
        function: "ask_device_technology",
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
        // Nullification Field: use (exhaust + a strategy token, end the turn) or decline.
        module: "faction_techs.rs",
        function: "offer_nullification_field",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Quantum Datahub Node: (partner, card given, card taken), or decline.
        module: "faction_techs.rs",
        function: "offer_quantum_datahub",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Scanlink Drone Network: which planet to explore, or none.
        module: "faction_techs.rs",
        function: "offer_scanlink",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Spatial Conduit Cylinders: use (exhaust, link the active system) or decline.
        module: "faction_techs.rs",
        function: "offer_spatial_conduit",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Production Biomes: which other player gains the 2 trade goods.
        module: "faction_abilities.rs",
        function: "production_biomes",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Munitions Reserves, asked inside the Keleres agent's goods window that
        // `space_combat_round_started` opens.
        module: "faction_abilities.rs",
        function: "munitions_offer",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Munitions Reserves after the roll (rules version 2), asked with the dice in view.
        module: "faction_abilities.rs",
        function: "munitions_roll_offer",
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
        // The Fracture, rules 9-12: choose the ingress system for each of the breakthrough's
        // technology-specialty colours. Options are the legal (system, planet) candidates for that
        // colour; no decline, because placement is not optional once the breakthrough is gained,
        // and an empty candidate set breaks out before a choice is built rather than offering none.
        module: "fracture.rs",
        function: "after_breakthrough_gained",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // The Fracture, rule 11: a game effect (Firmament Planesplitter) puts it into play and the
        // player who caused it chooses one ingress system per technology colour (BF-firmament.md).
        module: "fracture.rs",
        function: "enter_play_by_effect",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // The Firmament: every question asked inside a timing window, component action or leader
        // action (BF-firmament.md).
        module: "firmament.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // The Obsidian: every question asked inside a timing window, including the opponent's
        // choice for Vos Hollow (BF-obsidian.md).
        module: "obsidian.rs",
        function: "ask",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // The seat's action-phase turn: strategic, tactical, component, pass, contacts.
        module: "game.rs",
        function: "turn_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        // OP-08: the aftermath's pause before invasion or production: continue, or a contact.
        module: "game.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath_inner"),
    },
    Producer {
        // OP-08: end the turn, or do what does not take an action (and Fleet Logistics' second).
        module: "game.rs",
        function: "closing_options",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        // Structured diplomacy: each voter, in voting order, may open contacts before the vote.
        module: "game.rs",
        function: "agenda_talks_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_agenda_talks"),
    },
    Producer {
        // Political Favor and Political Secret: whether the holder uses the note at its window.
        module: "game.rs",
        function: "ask_to_use_note",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Z'eu (the Naalu's Prophecy of Kings agent): which non-home system to act in.
        module: "game.rs",
        function: "perform_zeu",
        count: 1,
        delivery: Delivery::ObservedHere,
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
        function: "absorb_ground_with_origin",
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
        delivery: Delivery::ObservedVia("game.rs::step_aftermath_inner"),
    },
    Producer {
        module: "laws.rs",
        function: "offer_discard",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // LEADER-FIX-001: the action-phase leaders ask their targets here — which planet to
        // ready, whether to remove the infantry, gain-or-replenish, which system to gather in,
        // and per-technology swap or keep.
        module: "leaders.rs",
        function: "dispatch_leader",
        count: 7,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "borrowed_round_agents.rs",
        function: "resolve_copy",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "leaders.rs",
        function: "ssruu_l1z1x_activation_abilities",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Harrugh Gefhara, the Hacan hero: offered when a production is about to be paid for.
        module: "leaders.rs",
        function: "offer_production_hero",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "legendary.rs",
        function: "end_turn",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "legendary.rs",
        function: "pass",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "legendary.rs",
        function: "place_on_own_planet",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "legendary.rs",
        function: "resolve",
        count: 5,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        module: "legendary.rs",
        function: "resolve_pass",
        count: 3,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // The scoring choice, and Plots Within Plots' second step: the plot card to place.
        module: "objectives.rs",
        function: "pending_choice",
        count: 2,
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
        function: "placement_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath_inner"),
    },
    Producer {
        module: "production.rs",
        function: "pending_choice",
        count: 2,
        delivery: Delivery::ObservedVia("game.rs::step_aftermath_inner"),
    },
    Producer {
        module: "production.rs",
        function: "produce_one_from",
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
        // Instinct Training: use (exhaust + a strategy token, cancel the card) or decline.
        module: "reactions.rs",
        function: "instinct_training",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // I48S, the L1Z1X agent, after a system is activated: exhaust or decline.
        module: "reactions.rs",
        function: "l1z1x_agent",
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
        function: "crown_of_emphidia_explore_with",
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
        // Starting technology (Winnu, Argent Flight, Keleres, Thunder's Edge picks): the front
        // pick of `GameState::starting_tech_picks`, asked by the driver before the first draft.
        module: "starting_tech.rs",
        function: "choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step"),
    },
    Producer {
        module: "strategy.rs",
        function: "secondary_question",
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
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        // Deepwrought commander: the researcher's reduction, then the holder's payment.
        module: "strategy_cards.rs",
        function: "deepwrought_commander",
        count: 2,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "trade_infantry_for_research",
        count: 1,
        delivery: Delivery::ObservedVia("strategy_cards.rs::ask"),
    },
    Producer {
        module: "strategy_cards.rs",
        function: "gain_tokens_offering",
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
        function: "offer_research_inner",
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
        function: "place_structure_step",
        count: 2, // the spot, then a PDS or a module alternative (Hecatoncheires)
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
        // Brother Omar and any other research waiver: when a faction waiver is the only way past
        // the prerequisites, the player chooses the waiver and then its exact cost. Both questions
        // are built in `resolve_research` and delivered through `strategy_cards::ask`.
        module: "strategy_cards.rs",
        function: "resolve_research",
        count: 2,
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
        function: "activation_options_with",
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
        module: "technology.rs",
        function: "production_used",
        count: 1,
        delivery: Delivery::ObservedHere,
    },
    Producer {
        // Which player places the Thunder's Edge expedition slice.
        module: "thunders_edge.rs",
        function: "choose_placer",
        count: 1,
        delivery: Delivery::ObservedVia("thunders_edge.rs::ask_seeing"),
    },
    Producer {
        // Which system the slice is placed in.
        module: "thunders_edge.rs",
        function: "choose_system",
        count: 1,
        delivery: Delivery::ObservedVia("thunders_edge.rs::ask_seeing"),
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
        // Two `Choice::new` sites share this function name. `TokenRedistribution::pending_choice`:
        // 81.5's gain and redistribution (one decision per seat), and Warfare's and Predictive
        // Intelligence's redistributions, all asked by `strategy_cards::redistribute_tokens`.
        // `TokenGain::pending_choice`: the gain token by token of a game from before
        // `RULES_STATUS_GAIN`, asked by `game.rs::step_token_gain` (wired below).
        count: 2,
        delivery: Delivery::ObservedVia("game.rs::step_token_gain"),
    },
    Producer {
        module: "transactions.rs",
        function: "pending_choice",
        count: 2,
        delivery: Delivery::ObservedVia("game.rs::step_trade"),
    },
    Producer {
        module: "window.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_diplomacy"),
    },
    Producer {
        // The deal builder: offer items, ask items, amounts, review (TRADE_REWORK_2026-09-22).
        module: "window.rs",
        function: "building_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_diplomacy"),
    },
    Producer {
        module: "transit.rs",
        function: "pending_choice",
        count: 1,
        delivery: Delivery::ObservedVia("game.rs::step_tactical"),
    },
    Producer {
        module: "vote.rs",
        function: "pending_choice_bare",
        // Outcome, planets, Gila's trade goods (hacancommander), tiebreak, Mahact's Recombine
        // and Tribute stages.
        count: 6,
        delivery: Delivery::ObservedVia("game.rs::step_vote"),
    },
];

const OBSERVED_ASKS: &[(&str, &str, usize)] = &[
    // BF faction modules (crates/ti4-engine/src/factions/).
    ("invasion.rs", "fight_committed_planet", 1),
    ("invasion.rs", "assign_selected_ground_hit_in_timing", 1),
    ("yssaril.rs", "borrowed_stall_tactics", 1),
    ("argent.rs", "ask_option_for", 1),
    ("sardakk.rs", "skip_to_commit", 1),
    ("muaat.rs", "ask", 1),
    ("nekro.rs", "take_from", 1),
    ("nekro_units.rs", "ask", 1),
    // Null Reference (Nekro): the ability production window's unit-type and placement questions.
    ("production.rs", "produce_unit_by_ability", 1),
    ("naaz.rs", "ask", 1),
    ("naaz.rs", "ask_checked", 1),
    ("naaz.rs", "borrowed_agent", 1),
    ("naaz.rs", "offer_supercharge", 1),
    ("saar.rs", "ask", 1),
    ("arborec.rs", "agent_use", 2),
    ("arborec.rs", "bioplasmosis", 1),
    ("arborec.rs", "mitosis", 1),
    ("arborec.rs", "perform_component", 2),
    ("arborec.rs", "stymie", 1),
    ("arborec.rs", "warrior_returns", 1),
    ("ghost.rs", "ask", 1),
    ("naalu.rs", "foresight", 1),
    ("naalu.rs", "hero", 1),
    ("yssaril.rs", "action_cards_drawn", 1),
    ("yssaril.rs", "action_card_draw_requested", 1),
    ("yssaril.rs", "commander_look", 1),
    ("yssaril.rs", "kyver_decisions", 1),
    // Deepgloom Executable runs the trade window itself, so its choices are asked here rather
    // than through `game.rs::step_trade`.
    ("yssaril.rs", "deepgloom_transaction", 1),
    ("mentak.rs", "ask_choice", 1),
    ("borrowed_commanders.rs", "ralnel_ask", 1),
    ("borrowed_commanders.rs", "crimson_ask", 1),
    ("titans.rs", "ask", 1),
    ("cabal.rs", "vortex_perform", 1),
    ("cabal.rs", "stillness_of_stars", 1),
    ("cabal.rs", "alraith", 1),
    ("nomad.rs", "ask", 1),
    ("keleres.rs", "laws_order", 1),
    ("keleres.rs", "offer_agent", 1),
    ("keleres_units.rs", "ask", 1),
    ("production.rs", "agency_supply_network", 2),
    ("empyrean.rs", "tether_place", 1),
    ("empyrean.rs", "voidwatch", 1),
    ("mahact.rs", "ask_among", 1),
    ("bastion.rs", "ask", 1),
    ("mahact_units.rs", "legionnaire", 1),
    ("mahact_units.rs", "legionnaire_returns", 1),
    ("mahact_units.rs", "offer_starlancer", 1),
    ("mahact_units.rs", "use_leader_timed", 1),
    ("empyrean_units.rs", "ask", 1),
    ("nomad.rs", "temporal_command_suite", 1),
    ("nomad_agents.rs", "ask", 1),
    ("invasion.rs", "coexist_instead", 1),
    ("invasion.rs", "offer_research_team", 1),
    ("invasion.rs", "offer_research_team_defense", 1),
    ("deepwrought_research.rs", "put", 1),
    ("deepwrought.rs", "ask", 1),
    ("deepwrought.rs", "ask_about", 1),
    ("deepwrought.rs", "ask_phrased", 1),
    ("crimson.rs", "ask", 1),
    ("crimson.rs", "ask_about", 1),
    ("crimson_cards.rs", "fill_hold", 1),
    // Ral Nel's timing windows and component actions (BF-ralnel): every question is put through
    // `ralnel::ask`; Survival Instinct's transport is the shared cargo hold (`passengers`).
    ("ralnel.rs", "ask", 1),
    ("ralnel.rs", "passengers", 1),
    ("titans_leaders.rs", "sleeper_allowance", 1),
    ("argent.rs", "afb_excess", 1),
    ("argent.rs", "extra_die_effect", 1),
    ("argent.rs", "strike_wing_effect", 1),
    ("winnu.rs", "htp", 1),
    ("winnu.rs", "imperator", 1),
    ("winnu.rs", "reclaimer", 1),
    ("winnu.rs", "use_leader", 3),
    ("winnu.rs", "use_leader_strategy_primary", 1),
    ("game.rs", "step_leader_followers", 1),
    ("sardakk.rs", "exotrireme", 2),
    ("sardakk.rs", "supremacy", 1),
    ("yin.rs", "ask_one", 1),
    ("action_cards.rs", "choose_from_own_hand", 1),
    ("action_cards.rs", "take_from_revealed_hand", 1),
    ("action_cards.rs", "choose_crashlanding_ground", 1),
    ("action_cards.rs", "choose_crashlanding_planet", 1),
    ("action_cards.rs", "confusing", 1),
    ("action_cards.rs", "enforce_hand_limit", 1),
    ("action_cards.rs", "exchange_program", 1),
    ("action_cards.rs", "ghost_squad", 1),
    ("action_cards.rs", "in_the_silence_of_space", 1),
    ("action_cards.rs", "pick_detailed", 1),
    // BF-00b: placement from reinforcements by a faction effect; asks its own choice.
    ("action_cards.rs", "place_units_choosing", 1),
    ("action_cards.rs", "pick_rider_target", 1),
    ("action_cards.rs", "predicted_outcome", 1),
    ("action_cards.rs", "public_disgrace", 1),
    ("action_cards.rs", "reparations", 2),
    ("action_cards.rs", "skilled_retreat", 1),
    ("agenda_effects.rs", "ask_the_speaker", 1),
    ("agenda_effects.rs", "choose_structure", 1),
    ("agenda_effects.rs", "resolve_with", 1),
    ("choice.rs", "ask_seeing", 1),
    ("choice.rs", "drive", 1),
    ("combat.rs", "choose_casualty_owing", 1),
    ("combat.rs", "choose_reroll_dice", 1),
    ("combat.rs", "heart_ixth", 1),
    ("combat.rs", "offer_sustain", 1),
    ("combat.rs", "roll_round", 1),
    ("combat.rs", "wrath_of_kenara_buy", 1),
    ("exploration.rs", "ask_decorated", 1),
    ("exploration.rs", "ask_device_technology", 1),
    ("faction_abilities.rs", "munitions_offer", 1),
    ("faction_abilities.rs", "munitions_roll_offer", 1),
    ("faction_abilities.rs", "perform_component", 2),
    ("faction_abilities.rs", "production_biomes", 1),
    ("faction_abilities.rs", "strategy_resolved", 1),
    ("faction_techs.rs", "offer_nullification_field", 1),
    ("faction_techs.rs", "offer_quantum_datahub", 1),
    ("faction_techs.rs", "offer_scanlink", 1),
    ("faction_techs.rs", "offer_spatial_conduit", 1),
    ("fleet.rs", "remove_one", 1),
    ("game.rs", "step", 1),
    ("game.rs", "step_aftermath_inner", 1),
    ("game.rs", "step_event_scoring", 1),
    ("game.rs", "step_scoring", 1),
    ("game.rs", "step_secondary", 1),
    ("game.rs", "step_tactical", 1),
    ("game.rs", "step_token_gain", 1),
    ("game.rs", "step_trade", 1),
    ("game.rs", "step_diplomacy", 1),
    ("game.rs", "step_agenda_talks", 1),
    ("game.rs", "step_vote", 1),
    ("game.rs", "ask_to_use_note", 1),
    ("game.rs", "perform_zeu", 1),
    ("game.rs", "committee_formation", 1),
    ("game.rs", "imperial_arbiter", 2),
    ("game.rs", "minister_of_war", 1),
    ("invasion.rs", "absorb_ground_with_origin", 1),
    ("invasion.rs", "commit_ground_forces", 1),
    ("invasion.rs", "drive", 1),
    ("production.rs", "integrated_economy", 1),
    ("production.rs", "pay_with_observation_credit", 1),
    ("production.rs", "produce_one_from", 2),
    // BF-00b: ability-driven production delivers `ProductionWindow::pending_choice` itself.
    ("production.rs", "produce_by_ability_capped", 1),
    ("production.rs", "resolve_timed", 1),
    ("production.rs", "sling_relay", 2),
    ("reactions.rs", "instinct_training", 1),
    ("reactions.rs", "l1z1x_agent", 1),
    ("reactions.rs", "slot", 1),
    ("relics.rs", "codex", 1),
    ("relics.rs", "crown_of_emphidia_explore_with", 1),
    ("relics.rs", "grant_chosen_technology", 1),
    ("relics.rs", "offer_dominus_orb", 1),
    ("relics.rs", "stellar_converter", 1),
    ("relics.rs", "titan_prototype", 1),
    ("entropic_scars.rs", "resolve_status_start", 1),
    ("fracture.rs", "after_breakthrough_gained", 1),
    ("fracture.rs", "enter_play_by_effect", 1),
    ("firmament.rs", "ask", 1),
    ("obsidian.rs", "ask", 1),
    ("invasion.rs", "apply_bombard_plan", 1),
    ("invasion.rs", "dunlain_reaper", 1),
    ("laws.rs", "offer_discard", 1),
    ("leaders.rs", "dispatch_leader", 7),
    ("leaders.rs", "offer_production_hero", 1),
    ("borrowed_round_agents.rs", "resolve_copy", 1),
    ("leaders.rs", "ssruu_l1z1x_activation_abilities", 1),
    ("legendary.rs", "end_turn", 1),
    ("legendary.rs", "pass", 1),
    ("legendary.rs", "place_on_own_planet", 1),
    ("legendary.rs", "resolve", 5),
    ("legendary.rs", "resolve_pass", 3),
    ("relics.rs", "neuraloop", 1),
    ("secrets.rs", "enforce_hand_limit", 1),
    ("strategy_cards.rs", "ask", 1),
    ("technology.rs", "end_turn", 2),
    ("technology.rs", "production_used", 1),
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
        ("choose", Operation::ChooseDirect, 3),
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
    let actual = scan();
    let expected = expected_sites();
    let differences: Vec<_> = actual
        .keys()
        .chain(expected.keys())
        .filter(|site| actual.get(*site) != expected.get(*site))
        .map(|site| (site, actual.get(site), expected.get(site)))
        .collect();
    assert!(
        differences.is_empty(),
        "unreviewed decision sites: {differences:?}"
    );
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
