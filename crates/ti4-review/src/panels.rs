//! The two big reading panels, shared by both applications.
//!
//! R01's left sheet (who is at the table, what they hold, what the table itself is doing) and its
//! right sheet (this engine step, the policy's decisions, the events, the selected system) are the
//! reason the reviewer is worth opening. They were written inline in `gui.rs`, which meant the
//! replayer could only ever carry an abbreviation of them - and an abbreviation is exactly what you
//! cannot judge a play from. The drawing code moved here unchanged so that both windows draw the
//! same sheet from the same source, and so a difference between the two is a call site, not a
//! rewrite.
//!
//! These functions draw *contents*, not panels: the frame, the width, the collapsing header and the
//! light theme belong to the application, because that is where the two windows legitimately differ.
//!
//! Nothing here reads a `LiveReview` or mutates anything; a frame and its session are all it takes.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32};
use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;
use ti4_model::id::SystemId;

use crate::view::{
    DecisionPath, action_summary_in, content_label, decision_rows, event_rows, item_section,
    planets_for_tile, player_color, seat_label, section, section_with_id, selected_system,
    stat_badge, step_view, unit_base,
};
use crate::{ReviewFrame, ReviewSession};

/// How a panel spells a system.
///
/// The engine names a system by the number printed on its tile - `activate 43` - which is exact and
/// unreadable: judging that move means knowing what is in 43. The replayer therefore spells systems
/// with the planets inside them as well as the number. R01's wording is left alone, because its
/// labels are what its golden tests and its readers already know.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemNaming {
    /// The tile number alone, as the engine says it.
    TileOnly,
    /// The tile number and what is inside it.
    TileAndPlanets,
}

impl SystemNaming {
    /// Rewrite every system this game has onto the text, when that is what was asked for.
    #[must_use]
    pub fn apply(self, session: &ReviewSession, text: &str) -> String {
        match self {
            // Seats are named in both modes: a seat and its faction always appear together.
            Self::TileOnly => crate::view::annotate_seats(session, text),
            Self::TileAndPlanets => crate::view::annotate(session, text),
        }
    }
}

/// Relationships, deals, signals and finished deals for a game played with structured diplomacy.
pub fn diplomacy_sheet(ui: &mut egui::Ui, frame: &ReviewFrame) {
    let state = &frame.state;
    let diplomacy = &state.diplomacy;
    if !diplomacy.enabled {
        ui.weak("Structured diplomacy is off for this game.");
        return;
    }
    ui.label("How each row seat regards each column seat.");
    ui.small(crate::diplomacy::RELATIONSHIP_LEGEND);
    egui::Grid::new(format!("diplomacy-relationships-{}", frame.index))
        .striped(true)
        .show(ui, |ui| {
            ui.strong("regards →");
            for subject in &state.seating_order {
                seat_label(
                    ui,
                    subject,
                    crate::view::seat_name(frame, subject, ContentStore::embedded()),
                );
            }
            ui.end_row();
            for observer in &state.seating_order {
                seat_label(
                    ui,
                    observer,
                    crate::view::seat_name(frame, observer, ContentStore::embedded()),
                );
                for subject in &state.seating_order {
                    if observer == subject {
                        ui.weak("—");
                        continue;
                    }
                    let relationship = diplomacy.relationship(observer, subject);
                    let mut text = format!(
                        "{} · {}",
                        crate::diplomacy::stance(relationship),
                        crate::diplomacy::relationship_cell(relationship)
                    );
                    if ti4_engine::diplomacy::recent_attack(state, observer, subject) {
                        text.push_str(" ⚔");
                    }
                    if ti4_engine::diplomacy::recent_breach(state, observer, subject) {
                        text.push_str(" ✗");
                    }
                    ui.label(text);
                }
                ui.end_row();
            }
        });

    ui.strong(format!("Active deals · {}", diplomacy.active_deals.len()));
    if diplomacy.active_deals.is_empty() {
        ui.weak("None");
    }
    for deal in diplomacy.active_deals.values() {
        ui.group(|ui| {
            for (index, line) in crate::diplomacy::deal_lines(state, deal)
                .into_iter()
                .enumerate()
            {
                if index == 0 {
                    ui.strong(line);
                } else {
                    ui.label(line);
                }
            }
        });
    }

    ui.strong(format!(
        "Recent signals · {}",
        diplomacy.recent_signals.len()
    ));
    if diplomacy.recent_signals.is_empty() {
        ui.weak("None");
    }
    for signal in &diplomacy.recent_signals {
        ui.label(crate::diplomacy::signal_text(state, signal));
    }

    egui::CollapsingHeader::new(format!("Finished deals · {}", diplomacy.history.len()))
        .id_salt(format!("diplomacy-history-{}", frame.index))
        .show(ui, |ui| {
            if diplomacy.history.is_empty() {
                ui.weak("None yet");
            }
            for summary in diplomacy.history.iter().rev() {
                ui.group(|ui| {
                    for (index, line) in crate::diplomacy::summary_lines(state, summary)
                        .into_iter()
                        .enumerate()
                    {
                        if index == 0 {
                            ui.strong(line);
                        } else {
                            ui.label(line);
                        }
                    }
                });
            }
        });
    ui.small(format!(
        "Journal events so far: {}",
        diplomacy.journal.len()
    ));
}

/// What a sheet is allowed to read: the header facts of a game, and the frames to look backwards
/// through.
///
/// R01 has one `ReviewSession` holding both, and passes [`Sheets::whole`]. The replayer's store keeps a
/// frame-stripped shell per branch with the frames in a list beside it - an R01 recording is hundreds of
/// megabytes, and a branch tree must not multiply that - so it passes the shell and the branch's frames
/// separately.
///
/// Splitting the two is not a convenience. Taking a sheet's history out of `session.frames` while the
/// frames live elsewhere is what crashed the replayer as soon as it drew a live table: a range like
/// `session.frames[..=frame.index]` on an empty shell panics with "range end index 0 out of range for
/// slice of length 0", and the previous-frame row panics with "the len is 0 but the index is 39". A
/// sheet handed a history that does not contain its frame loses the row that depends on it, and paints
/// everything else.
#[derive(Debug)]
pub struct Sheets<'a> {
    /// Manifest, board, content: the facts a game was played under.
    pub header: &'a ReviewSession,
    /// The frames to scan backwards through, oldest first. For R01 these are the header's own.
    pub frames: &'a [ReviewFrame],
}

impl<'a> Sheets<'a> {
    /// A session that carries its frames, which is how R01 works and how the HTML export works.
    #[must_use]
    pub fn whole(session: &'a ReviewSession) -> Self {
        Self {
            header: session,
            frames: &session.frames,
        }
    }

    /// The frame played immediately before this one, if the history handed to this sheet has one.
    ///
    /// Found by `index` rather than by position, because a branch's list is its own: a fork starts at
    /// zero, a rebuild appends, and neither promises that the nth element is frame n.
    #[must_use]
    pub fn previous(&self, frame: &ReviewFrame) -> Option<&'a ReviewFrame> {
        self.frames
            .iter()
            .rev()
            .find(|candidate| candidate.index < frame.index)
    }
}

/// R01's left sheet: the policy behind the table, the table itself, and one sheet per player.
///
/// Everything an omniscient reader is entitled to at this frame, in R01's own order and wording.
pub fn players_sheet(ui: &mut egui::Ui, source: &Sheets<'_>, frame: &ReviewFrame) {
    let session = source.header;
    ui.heading("Omniscient players");
    let version = frame.state.rules_version;
    if version == ti4_model::state::RULES_VERSION {
        ui.small(format!("Rules version {version}"));
    } else {
        ui.colored_label(
            Color32::from_rgb(200, 60, 20),
            format!(
                "Rules version {version}, but this engine plays {}: this game replays under older rules.",
                ti4_model::state::RULES_VERSION
            ),
        );
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        let content = ContentStore::embedded();
        section(ui, "Current policy profiles", |ui| {
            let policy = &session.manifest.policy;
            ui.group(|ui| {
                ui.strong(if policy.format.is_empty() {
                    "Legacy review · profile details unavailable".to_owned()
                } else {
                    format!(
                        "{}{}",
                        policy.format,
                        policy
                            .schema
                            .map_or_else(String::new, |schema| format!(" · schema {schema}"))
                    )
                });
                ui.small(format!(
                    "{} · {} · temperature {:.2}",
                    session.manifest.checkpoint_path,
                    if policy.format == "MLP inference bundle" {
                        "shared actor with faction rows"
                    } else if policy.format == crate::SEPARATE_MODELS_FORMAT {
                        "one model per faction"
                    } else {
                        session.manifest.profile_table.label()
                    },
                    session.manifest.temperature
                ));
                ui.small(format!(
                    "Seats 0–5: {}",
                    session.manifest.factions.join(" → ")
                ));
                if let Some(name) = &policy.name {
                    ui.label(name);
                }
                if let Some(source) = &policy.source {
                    ui.label(format!("Source: {source}"));
                }
                if let Some(commit) = &policy.git_commit {
                    ui.small(format!("Engine/training commit: {commit}"));
                }
                if let Some(update) = policy.update {
                    ui.small(format!("Training update: {update}"));
                }
                if let Some(dimensions) = &policy.dimensions {
                    ui.small(dimensions);
                }
                let mut runtime = Vec::new();
                if let Some(abi) = policy.projection_abi {
                    runtime.push(format!("projection ABI {abi}"));
                }
                if let Some(version) = policy.oov_registry_version {
                    runtime.push(format!("OOV registry v{version}"));
                }
                if let Some(mode) = &policy.critic_mode {
                    runtime.push(format!("critic {mode}"));
                }
                if let Some(temperature) = policy.trained_temperature {
                    runtime.push(format!("trained temperature {temperature:.2}"));
                }
                if !runtime.is_empty() {
                    ui.small(runtime.join(" · "));
                }
                ui.separator();
                ui.small(format!(
                    "Initial speaker: {} · map arrangement: {}",
                    session
                        .manifest
                        .initial_speaker
                        .as_deref()
                        .unwrap_or("legacy/unrecorded"),
                    session
                        .manifest
                        .map_arrangement_index
                        .map_or_else(|| "legacy/unrecorded".to_owned(), |value| value.to_string())
                ));
                if let Some(commit) = &session.manifest.engine_commit {
                    ui.small(format!(
                        "Review engine: {commit}{}",
                        if session.manifest.engine_dirty {
                            " (dirty build)"
                        } else {
                            ""
                        }
                    ));
                }
                if let Some(digest) = &session.manifest.content_sha256 {
                    ui.small(format!("Content: {digest}"));
                }
                if let Some(scope) = &session.manifest.source_scope {
                    ui.small(format!("Scope: {scope}"));
                }
            });
            item_section(
                ui,
                "◫",
                "Decision heads",
                policy.heads.clone(),
                Color32::LIGHT_BLUE,
            );
            item_section(
                ui,
                "♙",
                "Available faction rows",
                policy.factions.clone(),
                Color32::LIGHT_BLUE,
            );
            if !policy.profiles.is_empty() {
                item_section(
                    ui,
                    "ƒ",
                    "Loaded profiles",
                    policy.profiles.clone(),
                    Color32::LIGHT_BLUE,
                );
            }
        });
        section(ui, "Open public objectives", |ui| {
            if frame.state.revealed_objectives.is_empty() {
                ui.label("None revealed yet.");
            } else {
                for objective in &frame.state.revealed_objectives {
                    let record = content.get(ContentType::PublicObjectives, objective.as_str());
                    let name = record
                        .as_ref()
                        .and_then(|record| record.text("name"))
                        .unwrap_or(objective.as_str());
                    let points = record
                        .as_ref()
                        .and_then(|record| record.int("points"))
                        .unwrap_or(0);
                    let scored_by: Vec<String> = frame
                        .state
                        .scored_objectives
                        .iter()
                        .filter(|(_, scored)| scored.contains(objective))
                        .map(|(player, _)| player.to_string())
                        .collect();
                    ui.group(|ui| {
                        ui.strong(format!("{name} · {points} VP"));
                        ui.small(format!(
                            "{} · scored by {}",
                            objective,
                            if scored_by.is_empty() {
                                "nobody".to_owned()
                            } else {
                                scored_by.join(", ")
                            }
                        ));
                        if let Some(text) = record.as_ref().and_then(|record| record.text("text")) {
                            ui.label(text);
                        }
                    });
                }
            }
        });
        section(ui, "Table state", |ui| {
            ui.horizontal_wrapped(|ui| {
                stat_badge(ui, "♛", "Speaker", &frame.state.speaker);
                stat_badge(
                    ui,
                    "◎",
                    "Custodians",
                    if frame.state.custodians_removed {
                        "removed"
                    } else {
                        "present"
                    },
                );
                stat_badge(
                    ui,
                    "▣",
                    "Action discard",
                    frame.state.discarded_action_cards.len(),
                );
            });
            let initiative_order: Vec<String> = frame
                .state
                .initiative_order()
                .into_iter()
                .map(|player_id| {
                    let Some(player) = frame.state.player(&player_id) else {
                        return player_id.to_string();
                    };
                    let cards = player
                        .strategy_cards
                        .iter()
                        .map(|card| {
                            let initiative =
                                frame.state.card_initiative.get(card).copied().unwrap_or(99);
                            format!("{card} ({initiative})")
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{}{}",
                        crate::view::seat_name(frame, &player.id, content),
                        if cards.is_empty() {
                            String::new()
                        } else {
                            format!(" · {cards}")
                        }
                    )
                })
                .collect();
            item_section(
                ui,
                "➜",
                "Initiative turn order",
                initiative_order,
                Color32::LIGHT_BLUE,
            );
            if let Some(previous) = source.previous(frame)
                && previous.state.speaker != frame.state.speaker
            {
                ui.strong(format!(
                    "Speaker changed: {} → {} · events: {}",
                    crate::view::seat_name(frame, &previous.state.speaker, content),
                    crate::view::seat_name(frame, &frame.state.speaker, content),
                    if frame.new_events.is_empty() {
                        "unrecorded cause".to_owned()
                    } else {
                        frame.new_events.join(", ")
                    }
                ));
            }
            item_section(
                ui,
                "◆",
                "Unclaimed strategy cards",
                frame
                    .state
                    .unclaimed_strategy_cards
                    .iter()
                    .map(|card| {
                        let goods = frame
                            .state
                            .strategy_card_goods
                            .get(card)
                            .copied()
                            .unwrap_or_default();
                        if goods > 0 {
                            format!(
                                "{} · {goods} TG",
                                content_label(content, ContentType::StrategyCards, card)
                            )
                        } else {
                            content_label(content, ContentType::StrategyCards, card)
                        }
                    })
                    .collect(),
                Color32::LIGHT_BLUE,
            );
            item_section(
                ui,
                "⚖",
                "Laws in play",
                frame
                    .state
                    .laws
                    .iter()
                    .map(|(law, outcome)| {
                        format!(
                            "{} · {outcome}",
                            content_label(content, ContentType::Agendas, law)
                        )
                    })
                    .collect(),
                Color32::LIGHT_BLUE,
            );
            item_section(
                ui,
                "↯",
                "Discarded action cards",
                frame
                    .state
                    .discarded_action_cards
                    .iter()
                    .map(|card| content_label(content, ContentType::ActionCards, card))
                    .collect(),
                Color32::LIGHT_BLUE,
            );
        });
        section(ui, "Diplomacy", |ui| diplomacy_sheet(ui, frame));
        let explored = crate::view::exploration_lines(session, frame, content);
        section_with_id(
            ui,
            &format!("Exploration this round · {}", explored.len()),
            "exploration",
            |ui| {
                if explored.is_empty() {
                    ui.weak("Nothing explored this round yet.");
                }
                for line in &explored {
                    ui.label(line);
                }
            },
        );
        section(ui, "Player sheets", |ui| {
            for player in &frame.state.players {
                let color = player_color(&player.id);
                egui::CollapsingHeader::new(format!(
                    "● {} · {} VP",
                    crate::view::seat_name(frame, &player.id, content),
                    player.victory_points
                ))
                .default_open(true)
                .show(ui, |ui| {
                    seat_label(
                        ui,
                        &player.id,
                        if player.passed { "PASSED" } else { "ACTIVE" },
                    );
                    ui.horizontal_wrapped(|ui| {
                        stat_badge(ui, "★", "VP", player.victory_points);
                        stat_badge(ui, "◆", "TG", player.trade_goods);
                        stat_badge(ui, "◇", "Com", player.commodities);
                    });
                    let pools =
                        ti4_engine::tokens::CommandPools::of(&frame.state, content, &player.id);
                    ui.horizontal_wrapped(|ui| {
                        stat_badge(ui, "▲", "Tactic", pools.tactic);
                        if pools.fleet_supply_bonus > 0 {
                            // Armada: the supply limit per system is the pool plus the bonus.
                            stat_badge(
                                ui,
                                "⬟",
                                "Fleet",
                                format!(
                                    "{} · supply limit {}",
                                    pools.fleet,
                                    ti4_engine::fleet::limit(&frame.state, content, &player.id)
                                ),
                            );
                        } else {
                            stat_badge(ui, "⬟", "Fleet", pools.fleet);
                        }
                        stat_badge(ui, "●", "Strategy", pools.strategic);
                    });

                    let strategy = player
                        .strategy_cards
                        .iter()
                        .map(|card| {
                            let label = content_label(content, ContentType::StrategyCards, card);
                            if player.exhausted_strategy_cards.contains(card) {
                                format!("{label} · used")
                            } else {
                                label
                            }
                        })
                        .collect();
                    item_section(ui, "◆", "Strategy cards", strategy, color);

                    let mut controlled_planets = Vec::new();
                    for tile in &session.board {
                        let state = frame.state.board.get(&SystemId::new(&tile.system));
                        for planet in planets_for_tile(session, frame, tile) {
                            if state.and_then(|state| state.planet_control.get(planet.id.as_str()))
                                != Some(&player.id)
                            {
                                continue;
                            }
                            let exhausted =
                                frame.state.exhausted_planets.contains(planet.id.as_str());
                            let attachments: &[String] = frame
                                .state
                                .planet_attachments
                                .get(planet.id.as_str())
                                .map_or(&[][..], Vec::as_slice);
                            controlled_planets.push(format!(
                                "{} {}/{}{}{}",
                                planet.label,
                                planet.resources,
                                planet.influence,
                                if exhausted { " · exhausted" } else { "" },
                                if attachments.is_empty() {
                                    String::new()
                                } else {
                                    // Named, not counted - see `view::attachment_names` for why the
                                    // engine cannot yet say what an attachment does.
                                    format!(
                                        " · {} attachment(s): {}",
                                        attachments.len(),
                                        crate::view::attachment_labels(attachments, content)
                                            .join(", ")
                                    )
                                }
                            ));
                        }
                    }
                    item_section(ui, "●", "Planets", controlled_planets, color);
                    item_section(
                        ui,
                        "✨",
                        "Special effects",
                        crate::novelty::standing(frame, &player.id),
                        color,
                    );
                    ui.label(
                        egui::RichText::new(crate::view::negotiation_budget(frame, &player.id))
                            .color(crate::view::PANEL_TEXT),
                    );
                    let totals = crate::view::planet_totals(session, frame, &player.id, content);
                    for line in crate::view::planet_totals_lines(&totals) {
                        ui.label(egui::RichText::new(line).color(crate::view::PANEL_TEXT));
                    }

                    let mut unit_counts: BTreeMap<String, usize> = BTreeMap::new();
                    for state in frame.state.board.values() {
                        for unit in state
                            .units
                            .iter()
                            .chain(state.planet_units.values().flat_map(|units| units.iter()))
                        {
                            if unit.owner == player.id {
                                *unit_counts.entry(unit_base(content, unit)).or_default() += 1;
                            }
                        }
                    }
                    item_section(
                        ui,
                        "⬡",
                        "Units on board",
                        unit_counts
                            .into_iter()
                            .map(|(kind, count)| format!("{kind} ×{count}"))
                            .collect(),
                        color,
                    );

                    item_section(
                        ui,
                        "⚙",
                        "Technologies",
                        player
                            .technologies
                            .iter()
                            .map(|technology| {
                                let label =
                                    content_label(content, ContentType::Technologies, technology);
                                if player.exhausted_technologies.contains(technology) {
                                    format!("{label} · exhausted")
                                } else {
                                    label
                                }
                            })
                            .collect(),
                        color,
                    );
                    item_section(
                        ui,
                        "✓",
                        "Scored objectives",
                        frame
                            .state
                            .scored_objectives
                            .get(&player.id)
                            .into_iter()
                            .flatten()
                            .map(|objective| {
                                content_label(content, ContentType::PublicObjectives, objective)
                            })
                            .collect(),
                        color,
                    );
                    item_section(
                        ui,
                        "?",
                        "Secret objectives",
                        player
                            .secret_objectives
                            .iter()
                            .map(|objective| {
                                content_label(content, ContentType::SecretObjectives, objective)
                            })
                            .collect(),
                        color,
                    );
                    item_section(
                        ui,
                        "▣",
                        "Action cards",
                        player
                            .action_cards
                            .iter()
                            .map(|card| content_label(content, ContentType::ActionCards, card))
                            .collect(),
                        color,
                    );
                    item_section(
                        ui,
                        "✦",
                        "Relics and fragments",
                        player
                            .relics
                            .iter()
                            .map(|relic| {
                                let label = content_label(content, ContentType::Relics, relic);
                                if player.exhausted_relics.contains(relic) {
                                    format!("{label} · exhausted")
                                } else {
                                    label
                                }
                            })
                            .chain(player.relic_fragments.iter().map(|(trait_name, count)| {
                                format!("{trait_name} fragment ×{count}")
                            }))
                            .collect(),
                        color,
                    );
                    item_section(
                        ui,
                        "◈",
                        "Exploration cards in play",
                        player
                            .exploration_cards
                            .iter()
                            .map(|card| content_label(content, ContentType::Explores, card))
                            .collect(),
                        color,
                    );
                    let mut promissory: Vec<String> = frame
                        .state
                        .promissory_notes
                        .iter()
                        .filter(|(_, holder)| *holder == &player.id)
                        .map(|(note, _)| {
                            let label = content_label(content, ContentType::PromissoryNotes, note);
                            if frame.state.promissory_faceup.contains(note) {
                                format!("{label} · faceup")
                            } else {
                                label
                            }
                        })
                        .collect();
                    promissory.extend(
                        frame
                            .state
                            .support_holders
                            .iter()
                            .filter(|(_, holder)| *holder == &player.id)
                            .map(|(owner, _)| {
                                format!(
                                    "Support for the Throne: {} · faceup",
                                    crate::view::seat_name(frame, owner, content)
                                )
                            }),
                    );
                    // A seat's own Support is not in `promissory_notes`: the engine tracks it only
                    // by its absence from `support_holders`, so an unlent one is still in hand.
                    if !frame.state.support_holders.contains_key(&player.id) {
                        promissory.push("Support for the Throne".to_owned());
                    }
                    promissory.sort();
                    promissory.dedup();
                    item_section(ui, "✉", "Promissory notes", promissory, color);
                    item_section(
                        ui,
                        "♟",
                        "Leaders",
                        player
                            .leaders
                            .iter()
                            .map(|(leader, status)| {
                                format!(
                                    "{} · {status:?}",
                                    content_label(content, ContentType::Leaders, leader)
                                )
                            })
                            .collect(),
                        color,
                    );
                    item_section(ui, "⌁", "Plots", player.plots.clone(), color);
                    if let Some(breakthrough) = &player.breakthrough {
                        item_section(
                            ui,
                            "⚡",
                            "Breakthrough",
                            vec![content_label(
                                content,
                                ContentType::Breakthroughs,
                                breakthrough,
                            )],
                            color,
                        );
                    }
                    ui.collapsing("Complete player JSON", |ui| {
                        let text = serde_json::to_string_pretty(player)
                            .unwrap_or_else(|error| error.to_string());
                        ui.monospace(text);
                    });
                });
            }
        });
    });
}

/// R01's right sheet: this engine step, the action, every decision the policy settled, the new
/// events, and the selected system.
///
/// `selected` is the system the board has highlighted, or `None`. `naming` decides whether a
/// system is spelled by its tile number alone or by the planets inside it as well.
pub fn decision_sheet(
    ui: &mut egui::Ui,
    source: &Sheets<'_>,
    frame: &ReviewFrame,
    selected: Option<&str>,
    naming: SystemNaming,
) {
    let session = source.header;
    ui.heading("Step and policy detail");
    let step = step_view(frame);
    ui.label(format!(
        "Step {} · decision {} · action {}",
        step.engine_step, step.decision_count, step.action_count
    ));
    ui.label(format!(
        "Round {} · {} · active {}",
        step.round,
        step.phase,
        naming.apply(session, &step.active)
    ));
    ui.horizontal_wrapped(|ui| {
        stat_badge(
            ui,
            "⬡",
            "Active system",
            naming.apply(session, &step.active_system),
        );
        stat_badge(ui, "⌛", "Pending", &step.pending);
        stat_badge(ui, "⚔", "Combat round", step.combat_round);
    });
    // The agenda under vote, whose card the vote itself never names.
    if let Some(alias) = crate::view::current_agenda(source.frames, frame) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            for (index, line) in crate::view::agenda_card(ContentStore::embedded(), &alias)
                .iter()
                .enumerate()
            {
                if index == 0 {
                    ui.strong(line);
                } else {
                    ui.label(line);
                }
            }
        });
    }
    if !step.agenda_lines.is_empty() {
        ui.collapsing("Current timing / agenda state", |ui| {
            for line in &step.agenda_lines {
                ui.label(line);
            }
        });
    }
    if let Some(error) = &step.error {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        let action = action_summary_in(source.frames, frame);
        section_with_id(ui, action.title, "action-summary", |ui| {
            if let Some(headline) = &action.headline {
                ui.strong(naming.apply(session, headline));
                if let Some(span) = &action.span {
                    ui.small(span);
                }
                for detail in &action.details {
                    ui.label(format!("• {}", naming.apply(session, detail)));
                }
            } else {
                ui.label("No action-phase turn has completed yet.");
            }
        });
        let rolls = crate::view::rolls_for_action(source.frames, frame);
        section_with_id(ui, &format!("Dice · {}", rolls.len()), "dice", |ui| {
            if rolls.is_empty() {
                ui.weak("No dice rolled in this action so far.");
            }
            for roll in &rolls {
                ui.label(crate::view::roll_line(
                    frame,
                    roll,
                    ContentStore::embedded(),
                ));
            }
        });
        section(ui, "Decisions", |ui| {
            let rows = decision_rows(frame);
            if rows.is_empty() {
                ui.label("This engine step resolved no policy choice.");
            }
            for (decision_index, row) in rows.iter().enumerate() {
                ui.separator();
                ui.strong(format!(
                    "Decision {} · {}",
                    row.sequence,
                    crate::view::seat_name(
                        frame,
                        &ti4_model::id::PlayerId::new(&row.player),
                        ContentStore::embedded()
                    )
                ));
                // Who actually chose: the model, or a fleet plan carrying out an
                // earlier decision. Without this a planned move reads as a decision
                // the model made.
                if let Some(note) = row.path.annotation() {
                    let colour = if row.path == DecisionPath::FleetPlan {
                        Color32::LIGHT_GREEN
                    } else {
                        Color32::LIGHT_YELLOW
                    };
                    ui.colored_label(colour, note);
                }
                ui.label(naming.apply(session, &row.prompt));
                for line in crate::view::details_lines(session, &row.prompt, &row.details) {
                    ui.small(line);
                }
                if let Some(effect) = &row.pending_effect {
                    if let Some(said) = crate::view::say_effect(session, effect) {
                        ui.small(format!("Pending effect: {said}"));
                    }
                }
                if let Some(context) = &row.context {
                    ui.collapsing("Typed decision context", |ui| {
                        ui.monospace(context);
                    });
                } else {
                    ui.small("Typed decision context unavailable (legacy or viewless choice)");
                }
                ui.label(&row.summary);
                if let Some(rank) = &row.rank {
                    ui.small(format!(
                        "Chosen rank {}/{} · p {} · best p {}{}",
                        rank.position,
                        rank.ranked,
                        rank.probability,
                        rank.best,
                        if rank.below_greedy {
                            " · sampled below the greedy choice"
                        } else {
                            ""
                        }
                    ));
                }
                for option in &row.options {
                    egui::CollapsingHeader::new(naming.apply(session, &option.title))
                        .id_salt(format!(
                            "option-{}-{decision_index}-{}",
                            frame.index, option.id
                        ))
                        .default_open(option.selected)
                        .show(ui, |ui| {
                            ui.label(format!("id={} kind={}", option.id, option.kind));
                            for line in crate::view::option_lines(
                                session,
                                &option.id,
                                &option.label,
                                &row.details,
                                &row.previews,
                            ) {
                                ui.label(line);
                            }
                            for line in &option.detail_lines {
                                ui.strong(line);
                            }
                            if let Some(payload) = &option.payload {
                                ui.collapsing("Structured payload", |ui| {
                                    ui.monospace(payload);
                                });
                            }
                            if let Some(preview) = &option.preview {
                                ui.collapsing("Consequence preview", |ui| {
                                    ui.monospace(preview);
                                });
                            } else {
                                ui.small("Consequence preview unavailable");
                            }
                            egui::Grid::new(format!(
                                "features-{}-{decision_index}-{}",
                                frame.index, option.id
                            ))
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("feature");
                                ui.strong("value");
                                ui.strong("weight");
                                ui.strong("contribution");
                                ui.end_row();
                                for feature in &option.features {
                                    ui.label(&feature.name);
                                    ui.label(&feature.value);
                                    ui.label(&feature.weight);
                                    ui.label(&feature.contribution);
                                    ui.end_row();
                                }
                            });
                        });
                }
            }
        });
        section(ui, "New engine events", |ui| {
            let events = event_rows(frame);
            if events.is_empty() && frame.new_events.is_empty() {
                ui.label("—");
            } else {
                for event in &events {
                    egui::CollapsingHeader::new(naming.apply(session, &event.title)).show(
                        ui,
                        |ui| {
                            ui.monospace(&event.payload);
                        },
                    );
                }
                if !frame.new_events.is_empty() {
                    ui.collapsing("Legacy event-name trace", |ui| {
                        for event in &frame.new_events {
                            ui.monospace(event);
                        }
                    });
                }
            }
        });
        if let Some(tile) = selected {
            ui.separator();
            let card = selected_system(session, frame, tile);
            ui.strong(&card.title);
            for line in &card.lines {
                ui.label(line);
            }
            if card.planets.is_empty() {
                if card.has_metadata {
                    ui.label("Planets: none");
                }
            } else {
                ui.strong("Planets");
                for planet in &card.planets {
                    ui.label(planet);
                }
            }
            if let Some(dynamic) = &card.dynamic {
                ui.strong("Dynamic board state");
                ui.monospace(dynamic);
            } else {
                ui.label("Dynamic board state: empty");
            }
        }
    });
}
