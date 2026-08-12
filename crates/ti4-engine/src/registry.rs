//! The rule registry ledger (M06-017).
//!
//! Every registry in this engine follows the oracle's design: a card with no registered handler
//! is *unavailable*, never silently free. That is the right behaviour, but it makes coverage
//! invisible from the outside — a game where nobody can score looks the same as a game where
//! nobody has met a requirement.
//!
//! This counts what is covered, per registry, so the gap is a number rather than an impression.
//! It is also the honest answer to "how much of the rules are implemented", which is a question
//! this project has been burned by answering from memory.
//!
//! **The denominator is the corpus, not the oracle.** A card the oracle does not implement
//! either is not a porting gap, and reading these fractions as "work remaining on the migration"
//! overstates it badly. Measured against the oracle at the pinned commit on 2026-08-12, by
//! comparing its registered aliases with this engine's:
//!
//! | registry | oracle implements | this engine |
//! |---|---|---|
//! | public objectives | 32 | 40 |
//! | secret objectives | 27 | 27 |
//! | agenda effects | 34 | 34 |
//! | exploration cards | 33 | 36 |
//! | action cards | **1** | 0 |
//!
//! So "action cards 0/122" is one card behind the oracle, not 122. The rest of that deck is
//! unwritten in both engines and waits on the reaction system, not on porting effort. Re-measure
//! rather than trusting this table: it was true on the date above and nothing keeps it true.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};

/// Coverage of one registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    /// What the registry governs, e.g. "public objectives".
    pub registry: &'static str,
    /// Cards of this kind in the corpus, within the source scope.
    pub total: usize,
    /// Cards with a registered handler or predicate.
    pub implemented: usize,
}

impl Coverage {
    /// Cards the engine cannot act on.
    #[must_use]
    pub const fn missing(&self) -> usize {
        self.total.saturating_sub(self.implemented)
    }

    /// Implemented share, 0.0 to 1.0. An empty registry counts as covered — there is nothing
    /// missing from it, and reporting 0% would read as a gap that does not exist.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts are card totals, far below f64's exact-integer range"
        )]
        {
            self.implemented as f64 / self.total as f64
        }
    }
}

fn count(content: &ContentStore, kind: ContentType, sources: SourceSet) -> usize {
    content.from_sources(kind, sources).count()
}

/// Coverage across every registry, in a stable order.
#[must_use]
pub fn ledger(content: &ContentStore, sources: SourceSet) -> Vec<Coverage> {
    vec![
        Coverage {
            registry: "public objectives",
            total: count(content, ContentType::PublicObjectives, sources),
            // Bought objectives (61.10) are covered by their price, not a predicate, so they
            // count here too — otherwise the ledger would under-report by eight.
            implemented: crate::objectives::registered_aliases().len()
                + crate::objectives::bought_aliases().len(),
        },
        Coverage {
            registry: "secret objectives",
            total: count(content, ContentType::SecretObjectives, sources),
            implemented: crate::secrets::registered_aliases().len(),
        },
        Coverage {
            registry: "action cards",
            total: count(content, ContentType::ActionCards, sources),
            // *Playable*, not *implemented*: a reaction card can be played into its window and
            // still have no effect, which is announced rather than passed off as resolved. No
            // action card in this engine has an effect yet.
            implemented: 0,
        },
        Coverage {
            registry: "reaction windows",
            // Every card whose printed window is not "Action".
            total: content
                .from_sources(ContentType::ActionCards, sources)
                .filter(|record| record.text("window").is_some_and(|w| w.trim() != "Action"))
                .count(),
            // Mapped to an event *and* that event is emitted somewhere. A window with no
            // emission is as unplayable as one with no mapping, and far easier to mistake for
            // finished — so it does not count here.
            implemented: crate::reactions::reachable(content, sources)
                .into_iter()
                .filter(|alias| {
                    crate::reactions::window_for(content, alias).is_some_and(|window| {
                        crate::reactions::EMITTED_EVENTS.contains(&window.event)
                    })
                })
                .count(),
        },
        Coverage {
            registry: "agenda effects",
            total: count(content, ContentType::Agendas, sources),
            implemented: crate::agenda_effects::registered_aliases().len(),
        },
        Coverage {
            registry: "exploration cards",
            total: count(content, ContentType::Explores, sources),
            // Fragments and attachments always resolve; instants need a handler each.
            implemented: count(content, ContentType::Explores, sources)
                - crate::exploration::unimplemented(content, sources).len(),
        },
        Coverage {
            registry: "relics",
            total: count(content, ContentType::Relics, sources),
            // Plus the Circlet, whose effect is standing rather than an action.
            implemented: crate::relics::registered_aliases().len() + 1,
        },
    ]
}

/// A one-line-per-registry summary, for an evidence file or a console.
#[must_use]
pub fn report(content: &ContentStore, sources: SourceSet) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for entry in ledger(content, sources) {
        let _ = writeln!(
            out,
            "{:<20} {:>4}/{:<4} implemented ({:.0}%)",
            entry.registry,
            entry.implemented,
            entry.total,
            entry.fraction() * 100.0,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;

    #[test]
    fn the_ledger_covers_every_registry_the_engine_has() {
        let entries = ledger(ContentStore::embedded(), POK);
        let names: Vec<&str> = entries.iter().map(|entry| entry.registry).collect();

        for expected in [
            "public objectives",
            "secret objectives",
            "action cards",
            "agenda effects",
            "exploration cards",
            "relics",
        ] {
            assert!(names.contains(&expected), "{expected} is not reported");
        }
    }

    #[test]
    fn every_registry_has_cards_to_cover() {
        // A registry reporting zero total would mean the corpus lookup is wrong, and the
        // coverage figure beside it would be meaningless rather than merely bad.
        for entry in ledger(ContentStore::embedded(), POK) {
            assert!(entry.total > 0, "{} has no cards at all", entry.registry);
        }
    }

    #[test]
    fn implemented_never_exceeds_total() {
        // A registry claiming more handlers than cards means an alias was registered that the
        // corpus does not have — which the per-registry guard tests also catch, from the other
        // side.
        for entry in ledger(ContentStore::embedded(), POK) {
            assert!(
                entry.implemented <= entry.total,
                "{}: {} of {}",
                entry.registry,
                entry.implemented,
                entry.total
            );
        }
    }

    #[test]
    fn objective_coverage_matches_the_registry_itself() {
        let entries = ledger(ContentStore::embedded(), POK);
        let objectives = entries
            .iter()
            .find(|entry| entry.registry == "public objectives")
            .unwrap();

        assert_eq!(
            objectives.implemented,
            crate::objectives::registered_aliases().len()
                + crate::objectives::bought_aliases().len(),
            "achieved and bought objectives both count as covered"
        );
        // The public deck is fully registered now, so a drop shows up here as a gap rather
        // than as an objective quietly becoming unscoreable in a game.
        assert_eq!(
            objectives.missing(),
            0,
            "every revealed public objective must have a requirement or a price"
        );
    }

    #[test]
    fn an_empty_registry_counts_as_covered() {
        // Nothing is missing from it, and 0% would read as a gap that does not exist.
        let empty = Coverage {
            registry: "nothing",
            total: 0,
            implemented: 0,
        };
        assert!((empty.fraction() - 1.0).abs() < f64::EPSILON);
        assert_eq!(empty.missing(), 0);
    }

    #[test]
    fn print_the_ledger() {
        // Not an assertion: the numbers themselves are the deliverable, and this is how they
        // reach an evidence file without anyone retyping them.
        print!("{}", report(ContentStore::embedded(), POK));
    }

    #[test]
    fn the_report_names_every_registry_once() {
        let text = report(ContentStore::embedded(), POK);
        assert_eq!(
            text.lines().count(),
            ledger(ContentStore::embedded(), POK).len()
        );
        assert!(text.contains("public objectives"));
    }
}
