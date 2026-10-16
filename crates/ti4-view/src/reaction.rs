//! A reaction window as a client shows it: what opened it, and each thing that the seat may use
//! now as it is printed.
//!
//! Every window is one question of the timing resolver (`ti4_engine::timing`), and each of its
//! options has an id that says what it stands for: `leader:<owner>:<id>:<EVENT>:<when|after>`,
//! and the same for `ability`, `technology`, `promissory`, `unit` and `breakthrough`; the slot
//! of the action cards is `reaction:…`, with the printed names in its payload. So one resolver
//! finds the printed card of any option, and no window needs code of its own.
//!
//! The same resolver names the *key* of an option: what a seat mutes when it mutes a reaction
//! ([`crate::reaction_modes`]). An ability that hangs on several events has one key.

use serde::{Deserialize, Serialize};
use ti4_content::{ContentStore, Record};
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::decision_context::DecisionTrigger;
use ti4_model::content_types::ContentType;

/// The prefix of an ability id, and the content that it names.
const KINDS: [(&str, ContentType); 7] = [
    ("leader", ContentType::Leaders),
    ("ability", ContentType::Abilities),
    ("technology", ContentType::Technologies),
    ("promissory", ContentType::PromissoryNotes),
    ("note", ContentType::PromissoryNotes),
    ("unit", ContentType::Units),
    ("breakthrough", ContentType::Breakthroughs),
];

/// The kind of an action card offer, and the prefix of its key in no case: a card is muted by
/// its printed name, so every copy is covered at once.
pub const ACTION_CARD: &str = "action_card";

/// One thing that a reaction window offers, as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionOffer {
    /// The id of the option that uses it. A slot of several action cards has one offer for each
    /// printed name, all with the id of the slot.
    pub option: String,
    /// What a seat mutes: `leader:nomadagentmercer`, or the printed name of an action card.
    pub key: String,
    /// `leader`, `ability`, `technology`, `promissory`, `unit`, `breakthrough` or `action_card`.
    pub kind: String,
    /// Of a leader: `agent`, `commander` or `hero`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    pub name: String,
    /// When it is used, as printed. Empty for a card whose text says it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub window: String,
    /// The printed text. Empty when the content has no card for the option.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// The reaction window that the viewer is asked in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionFacts {
    /// What opened the window. Public: all of it has happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<DecisionTrigger>,
    /// The window is in the action phase, so "only on my turns" says something about it.
    pub own_turn_applies: bool,
    pub offers: Vec<ReactionOffer>,
}

fn field(record: &Record, key: &str) -> String {
    record.text(key).map(ToOwned::to_owned).unwrap_or_default()
}

/// The record that a key segment of an id names: the segment itself, or the segment without its
/// last words (`arborec_infantry2_return` is of `arborec_infantry2`).
fn record_of<'a>(
    content: &'a ContentStore,
    kind: ContentType,
    segment: &str,
) -> Option<&'a Record> {
    let mut name = segment;
    loop {
        if let Some(record) = content.get(kind, name) {
            return Some(record);
        }
        name = &name[..name.rfind('_')?];
    }
}

/// The printed card behind the id of an ability, with no option: its key, kind, role, name,
/// window and text. `None` for an id that is no ability of a card (a rule, a card slot).
fn printed(content: &ContentStore, id: &str) -> Option<ReactionOffer> {
    let parts: Vec<&str> = id.split(':').collect();
    let (prefix, kind) = KINDS.iter().find(|(prefix, _)| parts[0] == *prefix)?;
    if parts.len() < 4 {
        return None;
    }
    // The last two segments are the window. The key is the last segment before them that names
    // a record: a copied leader names its owners first.
    let keys = &parts[1..parts.len() - 2];
    let word = if *prefix == "note" {
        "promissory"
    } else {
        prefix
    };
    let Some(record) = keys
        .iter()
        .rev()
        .find_map(|segment| record_of(content, *kind, segment))
    else {
        let segment = keys.last().copied().unwrap_or(id);
        return Some(ReactionOffer {
            option: String::new(),
            key: format!("{word}:{segment}"),
            kind: word.to_owned(),
            role: String::new(),
            name: String::new(),
            window: String::new(),
            text: String::new(),
        });
    };
    let record_id = record.id().unwrap_or_default();
    let (role, window, text) = match kind {
        ContentType::Leaders => (
            field(record, "type"),
            field(record, "abilityWindow"),
            field(record, "abilityText"),
        ),
        ContentType::Abilities => (
            String::new(),
            field(record, "window"),
            [
                field(record, "windowEffect"),
                field(record, "permanentEffect"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        ),
        ContentType::Units => (String::new(), String::new(), field(record, "ability")),
        _ => (String::new(), String::new(), field(record, "text")),
    };
    Some(ReactionOffer {
        option: String::new(),
        key: format!("{word}:{record_id}"),
        kind: word.to_owned(),
        role,
        name: field(record, "name"),
        window,
        text,
    })
}

/// The printed names of the action cards that an option stands for.
///
/// A window slot that holds several cards names them all in `card_names`; one holding a single
/// printed name carries it in `card_name`; the inner "which card" step has one name per option.
fn card_names(option: &ChoiceOption) -> Vec<String> {
    let offers_cards = (option.kind == "ability" && option.id.starts_with("reaction:"))
        || option.kind == ti4_engine::reactions::ACTION_CARD_KIND;
    if !offers_cards {
        return Vec::new();
    }
    let listed: Vec<String> = option
        .payload
        .get("card_names")
        .and_then(serde_json::Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    option
        .payload
        .get("card_name")
        .and_then(serde_json::Value::as_str)
        .map(|name| vec![name.to_owned()])
        .unwrap_or_default()
}

fn card_offer(content: &ContentStore, option: &ChoiceOption, name: &str) -> ReactionOffer {
    let record = content
        .records(ContentType::ActionCards)
        .iter()
        .find(|record| record.text("name") == Some(name));
    ReactionOffer {
        option: option.id.clone(),
        key: name.to_owned(),
        kind: ACTION_CARD.to_owned(),
        role: String::new(),
        name: name.to_owned(),
        window: record
            .map(|record| field(record, "window"))
            .unwrap_or_default(),
        text: record
            .map(|record| field(record, "text"))
            .unwrap_or_default(),
    }
}

/// What an option of a reaction window offers, as printed. Empty for an option that offers
/// nothing that a seat can mute: the decline, a rule, a slot whose cards are not named.
#[must_use]
pub fn offers_of(content: &ContentStore, option: &ChoiceOption) -> Vec<ReactionOffer> {
    let cards = card_names(option);
    if !cards.is_empty() {
        return cards
            .iter()
            .map(|name| card_offer(content, option, name))
            .collect();
    }
    if option.kind != "ability" {
        return Vec::new();
    }
    printed(content, &option.id)
        .map(|mut offer| {
            offer.option.clone_from(&option.id);
            if offer.name.is_empty() {
                offer.name.clone_from(&option.label);
            }
            vec![offer]
        })
        .unwrap_or_default()
}

/// Whether `choice` is a reaction window of the timing resolver.
#[must_use]
pub fn is_window(choice: &Choice) -> bool {
    choice
        .context
        .as_ref()
        .is_some_and(|context| context.subtype.starts_with("reaction_"))
}

/// The facts of the reaction window `choice`. `None` for any other question.
#[must_use]
pub fn project_reaction_facts(content: &ContentStore, choice: &Choice) -> Option<ReactionFacts> {
    if !is_window(choice) {
        return None;
    }
    Some(ReactionFacts {
        trigger: choice
            .context
            .as_ref()
            .and_then(|context| context.trigger.clone()),
        own_turn_applies: choice
            .details
            .contains_key(ti4_engine::timing::ACTIVE_DETAIL),
        offers: choice
            .options
            .iter()
            .flat_map(|option| offers_of(content, option))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: &str) -> ChoiceOption {
        ChoiceOption::labelled(id, "ability", id)
    }

    fn one(id: &str) -> ReactionOffer {
        let offers = offers_of(ContentStore::embedded(), &option(id));
        assert_eq!(offers.len(), 1, "{id}");
        offers.into_iter().next().unwrap()
    }

    #[test]
    fn a_leader_is_its_printed_card_and_one_key_over_its_events() {
        let mercer = one("leader:nomad:nomadagentmercer:TACTICAL_ACTION_ENDED:after");
        assert_eq!(mercer.key, "leader:nomadagentmercer");
        assert_eq!(mercer.kind, "leader");
        assert_eq!(mercer.role, "agent");
        assert_eq!(mercer.name, "Field Marshal Mercer");
        assert!(mercer.window.starts_with("At the end of a player's turn"));
        assert!(mercer.text.contains("exhaust this card"));
        assert_eq!(
            mercer.option,
            "leader:nomad:nomadagentmercer:TACTICAL_ACTION_ENDED:after"
        );
        // A copied leader names its owners first, and is the same card.
        let copied =
            one("leader:argent:a:yssarilagent:argentagent:GROUND_FORCES_BEING_PRODUCED:when");
        assert_eq!(copied.key, "leader:argentagent");
        // Two events of one ability are one key.
        assert_eq!(
            one("ability:mentak:pillage:TRADE_GOODS_GAINED:after").key,
            one("ability:mentak:pillage:TRANSACTION_RESOLVED:after").key
        );
    }

    #[test]
    fn every_kind_of_id_finds_its_text() {
        for id in [
            "ability:arborec:mitosis:STATUS_PHASE_BEGAN:after",
            "technology:generic:it:ACTION_CARD_PLAYED:when",
            "promissory:hacan:stymie:SHIP_MOVED:after",
            "unit:ghost:ghost_mech:SYSTEM_ACTIVATED:after",
            "breakthrough:argent:argentbt:ACTION_COMPLETED:after",
        ] {
            let offer = one(id);
            assert!(!offer.name.is_empty(), "{id}");
            assert!(!offer.text.is_empty(), "{id}");
        }
        // The words after the id of a record say which part of its rule the ability is.
        assert_eq!(
            one("unit:arborec:arborec_infantry2_return:TURN_BEGAN:after").key,
            "unit:arborec_infantry2"
        );
    }

    #[test]
    fn an_id_with_no_card_keeps_its_label_and_has_a_key() {
        let offer = one("ability:x:no_such_ability:TURN_BEGAN:after");
        assert_eq!(offer.key, "ability:no_such_ability");
        assert_eq!(offer.name, "ability:x:no_such_ability:TURN_BEGAN:after");
        assert!(offer.text.is_empty());
    }

    #[test]
    fn a_slot_of_action_cards_is_one_offer_for_each_name() {
        let mut slot = option("reaction:b:ACTION_CARD_PLAYED:when");
        slot.payload.insert(
            "card_names".to_owned(),
            serde_json::json!(["Sabotage", "No Such Card"]),
        );
        let offers = offers_of(ContentStore::embedded(), &slot);
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].key, "Sabotage");
        assert_eq!(offers[0].kind, ACTION_CARD);
        assert!(!offers[0].text.is_empty());
        assert!(!offers[0].window.is_empty());
        assert!(offers[1].text.is_empty());
        assert!(offers.iter().all(|offer| offer.option == slot.id));
    }

    #[test]
    fn the_decline_and_a_rule_offer_nothing() {
        let content = ContentStore::embedded();
        assert!(offers_of(content, &ChoiceOption::decline()).is_empty());
        assert!(offers_of(content, &option("instinct_training")).is_empty());
    }
}
