//! The canonical critic state extractor (M09-027), per MLP plan §4.1.
//!
//! # Why a separate extractor exists at all
//!
//! The policy extractor is option-conditioned throughout: `state-kind:` carries `option.kind` and
//! `state-option:` carries `option.id`. §4.1 is explicit that revision 4 called those state-only and
//! was wrong. A value function computed from them is a value function of *the options on offer*,
//! which is not what `V(s)` means and which makes the critic's target move whenever the legal set
//! does.
//!
//! So the critic gets its own namespace and its own inventory:
//!
//! ```text
//! x_critic(s, f) = critic-state:* facts(s, f)
//!                ++ selected critic objective and ability facts(s, f)
//!                ++ emb[f]
//! ```
//!
//! # What it must not contain, which is the whole design
//!
//! No prompt. No option id, kind, payload or target. No legal-option count or aggregate. **No fact
//! derived by iterating the legal set at all** — not even its size, because that is a property of
//! what the engine happened to offer rather than of the position. No authored valuations, no
//! scoreable-count helpers, no future outcomes.
//!
//! That list is what makes the two invariance properties §4.2 requires *structural* rather than
//! coincidental: this function never receives a `Choice`, so it cannot depend on one. Permutation
//! and legal-set invariance follow from the signature, and the tests confirm the signature is not
//! being subverted.
//!
//! # Hidden information
//!
//! The entry points take a [`SeatObservation`] and **nothing else that names a seat**. The acting
//! player and the held-secret progress are both read off that capability, which the engine mints
//! only for the seat it is about to ask ([`ti4_engine::choice::ask_private`] performs the same
//! binding for offline drivers). There is no public constructor, so a policy-side caller cannot
//! bind one to a seat of its choosing.
//!
//! F-M09-027-1: the first version took a public `Observed`, an arbitrary `PlayerId`, and a
//! caller-supplied `held_secrets` slice. That recreated exactly the hole `SeatObservation` was
//! introduced to close — any caller could request any seat's position and staple any seat's
//! unredacted secret progress to it. The documentation claimed the records were engine-bound; the
//! signature did not say so, and only the signature counts.
//!
//! # Ablation gating
//!
//! §4.1 requires that objective aliases/progress and decomposed abilities reach the critic **only**
//! when their matching feature set is enabled, so a `factual` ablation cannot acquire them
//! indirectly through critic gradients. That is [`CriticFeatures`], and it is a parameter rather
//! than a constant because the three §6.5 ablation runs differ precisely in it.

use std::collections::BTreeMap;

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::PlayerId;
use ti4_model::state::Phase;

use crate::features::FeatureVector;
use crate::intern::register;

/// The namespace every critic fact lives in.
///
/// Disjoint from every policy family by construction, so a critic column can never alias a policy
/// one — §4.1's "never alias policy columns".
pub const CRITIC_FAMILY: &str = "critic-state";

/// Which optional fact groups the critic may see.
///
/// The three ablation runs differ in exactly this, so it travels as data rather than being compiled
/// in: a `factual` run must not gain objective or ability signal through the value path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CriticFeatures {
    /// Objective requirement and progress facts, matching the M09-021 policy set.
    pub objectives: bool,
    /// Faction decomposition facts, matching the M09-022 policy set.
    pub abilities: bool,
}

impl CriticFeatures {
    /// The base inventory only — no objectives, no abilities.
    #[must_use]
    pub const fn factual() -> Self {
        Self {
            objectives: false,
            abilities: false,
        }
    }

    /// Everything the full model sees.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            objectives: true,
            abilities: true,
        }
    }
}

const fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Strategy => "strategy",
        Phase::Action => "action",
        Phase::Status => "status",
        Phase::Agenda => "agenda",
    }
}

/// The acting seat's own standing: tokens, economy, score, holdings and identity.
///
/// Unprefixed names; [`critic_facts`] applies the namespace.
#[expect(
    clippy::cast_precision_loss,
    reason = "public counts are small integers"
)]
fn seat_standing(seat: Option<&ti4_engine::choice::PublicSeat>) -> Vec<(String, f64)> {
    let mut facts: Vec<(String, f64)> = Vec::new();
    let mut push = |name: &str, value: f64| facts.push((name.to_owned(), value));
    push(
        "tactic_tokens",
        f64::from(seat.map_or(0, |s| s.tactic_tokens)),
    );
    push(
        "fleet_tokens",
        f64::from(seat.map_or(0, |s| s.fleet_tokens)),
    );
    push(
        "strategic_tokens",
        f64::from(seat.map_or(0, |s| s.strategic_tokens)),
    );
    push("trade_goods", f64::from(seat.map_or(0, |s| s.trade_goods)));
    push("commodities", f64::from(seat.map_or(0, |s| s.commodities)));
    push(
        "victory_points",
        f64::from(seat.map_or(0, |s| s.victory_points)),
    );
    push(
        "technologies",
        seat.map_or(0, |s| s.technologies.len()) as f64,
    );
    push(
        "action_cards_held",
        seat.map_or(0, |s| s.action_cards_held) as f64,
    );
    push(
        "secrets_held",
        seat.map_or(0, |s| s.secret_objectives_held) as f64,
    );
    push(
        "passed",
        f64::from(u8::from(seat.is_some_and(|s| s.passed))),
    );
    // Faction identity, which §4.1's base inventory names. The embedding carries it too; this is
    // the readable form, and it is a corpus identity rather than a board one.
    if let Some(seat) = seat {
        push(&format!("faction:{}", seat.faction.as_str()), 1.0);
    }
    facts
}

/// The rest of the table, in aggregate and by count only.
///
/// Unprefixed names; [`critic_facts`] applies the namespace.
#[expect(
    clippy::cast_precision_loss,
    reason = "public counts are small integers"
)]
fn table_aggregate(
    seen: &Observed<'_>,
    player: &PlayerId,
    own_victory_points: i32,
) -> Vec<(String, f64)> {
    let mut facts: Vec<(String, f64)> = Vec::new();
    let mut push = |name: &str, value: f64| facts.push((name.to_owned(), value));
    //
    // Opponents contribute distributions, never identities: `victory_points:3 = 2` says two
    // opponents are on three points, and names neither. A per-seat fact would be a board identity
    // that means nothing next game, the same reason the policy path refuses one.
    let mut opponents = 0usize;
    let mut vp_spread: BTreeMap<i32, usize> = BTreeMap::new();
    let mut secret_spread: BTreeMap<usize, usize> = BTreeMap::new();
    let mut leader_vp = i32::MIN;
    for other in seen.players() {
        let Some(public) = seen.seat(other) else {
            continue;
        };
        leader_vp = leader_vp.max(public.victory_points);
        if other == player {
            continue;
        }
        opponents += 1;
        *vp_spread.entry(public.victory_points).or_default() += 1;
        *secret_spread
            .entry(public.secret_objectives_held)
            .or_default() += 1;
    }
    push("opponents", opponents as f64);
    for (points, seats) in &vp_spread {
        push(&format!("opponent_victory_points:{points}"), *seats as f64);
    }
    for (held, seats) in &secret_spread {
        push(&format!("opponent_secrets_held:{held}"), *seats as f64);
    }
    if leader_vp > i32::MIN {
        push("leader_victory_points", f64::from(leader_vp));
        let own = own_victory_points;
        push("victory_points_behind_leader", f64::from(leader_vp - own));
    }
    facts
}

/// The critic's view of a position, as named facts.
///
/// Takes **no `Choice`**, and takes its seat as a capability rather than as data. Both are
/// load-bearing: a function that cannot see the legal set cannot depend on its order or its
/// contents, and a function whose only seat argument is an engine-minted [`SeatObservation`] cannot
/// be pointed at somebody else's cards.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "public counts are small integers"
)]
pub fn critic_facts(view: &SeatObservation<'_>, enabled: CriticFeatures) -> Vec<(String, f64)> {
    let seen: &Observed<'_> = view.observed();
    let player: &PlayerId = view.bound_seat();
    let held_secrets = view.held_secret_progress();
    let held_secrets = held_secrets.as_slice();
    let mut facts: Vec<(String, f64)> = Vec::new();
    let mut push = |name: &str, value: f64| facts.push((format!("{CRITIC_FAMILY}:{name}"), value));

    // -- the round, and where in it --------------------------------------------------------
    push("round", f64::from(seen.round()));
    push(&format!("phase:{}", phase_name(seen.phase())), 1.0);

    let seat = seen.seat(player);
    for (name, value) in seat_standing(seat.as_ref()) {
        push(&name, value);
    }

    // -- what the acting seat holds on the board -------------------------------------------
    push(
        "controlled_planets",
        seen.controlled_planets(player).len() as f64,
    );
    push(
        "systems_with_units",
        seen.systems_with_units_of(player).len() as f64,
    );
    push(
        "systems_with_token",
        seen.systems_with_token(player).len() as f64,
    );
    push("units_held", seen.units_held(player) as f64);
    push("board_systems", seen.board().len() as f64);

    for (name, value) in
        table_aggregate(seen, player, seat.as_ref().map_or(0, |s| s.victory_points))
    {
        push(&name, value);
    }

    // -- public reveal counts, deliberately without aliases ---------------------------------
    //
    // §4.1: "public score totals and reveal counts without objective aliases". The count is a fact
    // about the position; the aliases are objective signal and are gated below.
    push(
        "revealed_objectives",
        seen.revealed_objectives().len() as f64,
    );
    push("scored_objectives", seen.scored_by(player).len() as f64);

    // -- gated groups ------------------------------------------------------------------------
    if enabled.objectives {
        facts.extend(objective_facts(seen, player, held_secrets));
    }
    if enabled.abilities {
        facts.extend(ability_facts(seen, player));
    }

    facts.sort_by(|left, right| left.0.cmp(&right.0));
    facts.dedup_by(|later, earlier| {
        if later.0 == earlier.0 {
            earlier.1 += later.1;
            true
        } else {
            false
        }
    });
    facts
}

/// Objective requirement and progress, in the critic namespace.
///
/// The same aggregation the policy path uses, renamed — family maxima and need markers, never a
/// per-option or per-choice quantity.
fn objective_facts(
    seen: &Observed<'_>,
    player: &PlayerId,
    held_secrets: &[ti4_engine::objectives::CardProgress],
) -> Vec<(String, f64)> {
    let publics = seen.revealed_objective_progress(player);
    let mut family_max: BTreeMap<String, f64> = BTreeMap::new();
    let mut met = 0usize;
    for card in publics.iter().chain(held_secrets) {
        let ratio = if card.threshold > 0.0 {
            (card.have / card.threshold).min(1.0)
        } else {
            0.0
        };
        family_max
            .entry(card.family_token.clone())
            .and_modify(|best| *best = best.max(ratio))
            .or_insert(ratio);
        if card.satisfied {
            met += 1;
        }
    }
    let mut facts: Vec<(String, f64)> = family_max
        .into_iter()
        .filter(|(_, best)| *best > 0.0)
        .map(|(family, best)| (format!("{CRITIC_FAMILY}:objective_progress:{family}"), best))
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    facts.push((format!("{CRITIC_FAMILY}:objectives_met"), met as f64));
    facts
}

/// Faction decomposition, in the critic namespace.
fn ability_facts(seen: &Observed<'_>, player: &PlayerId) -> Vec<(String, f64)> {
    let Some(seat) = seen.seat(player) else {
        return Vec::new();
    };
    let Some(faction) = seen
        .content()
        .get(
            ti4_model::content_types::ContentType::Factions,
            seat.faction.as_str(),
        )
        .filter(|record| record.in_sources(seen.sources()))
        .map(ti4_content::factions::Faction::new)
    else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    for ability in faction.abilities() {
        facts.push((format!("{CRITIC_FAMILY}:ability:{ability}"), 1.0));
    }
    for tech in faction.faction_tech() {
        facts.push((format!("{CRITIC_FAMILY}:faction_tech:{tech}"), 1.0));
    }
    facts
}

/// The critic vector, keyed and ordered like any other.
#[must_use]
pub fn critic_vector(view: &SeatObservation<'_>, enabled: CriticFeatures) -> CriticVector {
    CriticVector(FeatureVector::from_pairs(
        critic_facts(view, enabled)
            .into_iter()
            .map(|(name, value)| (register(&name), value)),
    ))
}

/// A feature vector that came out of [`critic_vector`], and can have come from nowhere else.
///
/// F-M09-027-2: the value head used to accept the same public sparse type the policy uses, so a
/// caller could hand it an option's vector — legal-set-derived columns and all — and the claim that
/// the value function "has no way to see the legal set" was simply false at the inference API. The
/// wrapper is the fix: it holds a private field and has no public constructor, so the only route to
/// a critic input starts at a [`SeatObservation`] and passes through the option-free extractor
/// above.
#[derive(Debug, Clone)]
pub struct CriticVector(FeatureVector);

impl CriticVector {
    /// The underlying facts, for naming and column lookup.
    #[must_use]
    pub const fn facts(&self) -> &FeatureVector {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Choice, ChoiceOption};
    use ti4_model::content_types::POK;
    use ti4_model::id::FactionId;
    use ti4_model::state::GameState;

    fn position() -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = ti4_engine::fixtures::game(&["a", "b", "c"]);
        state.round = 3;
        {
            let seat = state.player_mut(&player).unwrap();
            seat.faction = FactionId::new("sol");
            seat.tactic_tokens = 2;
            seat.trade_goods = 5;
            seat.victory_points = 1;
        }
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .victory_points = 3;
        state
            .player_mut(&PlayerId::new("c"))
            .unwrap()
            .victory_points = 3;
        (state, player)
    }

    /// Obtain the facts the way production does: through the engine's own seat binding.
    ///
    /// `SeatObservation::bind` is `pub(crate)` to `ti4-engine`, so these tests cannot mint a
    /// capability — which is the property F-M09-027-1 asked for. The only public route is
    /// `ask_private`, which binds the view to the choice's owner and hands it to the decider. The
    /// probe therefore reads its facts *inside* `choose_seeing`, exactly where a real bot does.
    fn facts_via_engine(
        state: &GameState,
        player: &PlayerId,
        enabled: CriticFeatures,
    ) -> Vec<(String, f64)> {
        struct Probe {
            enabled: CriticFeatures,
            out: Vec<(String, f64)>,
        }
        impl ti4_engine::choice::Decider for Probe {
            fn choose(
                &mut self,
                choice: &Choice,
            ) -> Result<ChoiceOption, ti4_engine::choice::IllegalChoice> {
                Ok(choice.options[0].clone())
            }
            fn choose_seeing(
                &mut self,
                choice: &Choice,
                seen: &SeatObservation<'_>,
            ) -> Result<ChoiceOption, ti4_engine::choice::IllegalChoice> {
                self.out = critic_facts(seen, self.enabled);
                Ok(choice.options[0].clone())
            }
        }

        let content = ContentStore::embedded();
        let choice = Choice::new(
            player.clone(),
            "probe",
            vec![ChoiceOption::new("noop", "noop")],
        );
        let mut probe = Probe {
            enabled,
            out: Vec::new(),
        };
        ti4_engine::choice::ask_private(&choice, state, content, POK, None, &mut probe)
            .expect("the probe answers");
        probe.out
    }

    fn names(state: &GameState, player: &PlayerId, enabled: CriticFeatures) -> Vec<String> {
        facts_via_engine(state, player, enabled)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    #[test]
    fn every_critic_name_is_in_its_own_namespace() {
        // §4.1: the critic namespace never aliases policy columns. Checked over the full inventory
        // rather than by inspection of the builder.
        let (state, player) = position();
        let facts = names(&state, &player, CriticFeatures::full());
        assert!(!facts.is_empty());
        for name in &facts {
            assert!(
                name.starts_with(&format!("{CRITIC_FAMILY}:")),
                "{name} is outside the critic namespace"
            );
        }
    }

    #[test]
    fn the_inventory_excludes_everything_section_four_one_forbids() {
        // The exclusions are the design. A name carrying a prompt, an option, a target or a
        // legal-set aggregate would make `V` a function of what was offered rather than of the
        // position.
        let (state, player) = position();
        for name in names(&state, &player, CriticFeatures::full()) {
            let bare = name
                .strip_prefix(&format!("{CRITIC_FAMILY}:"))
                .expect("namespaced");
            for forbidden in [
                "prompt",
                "option",
                "kind",
                "payload",
                "target",
                "legal",
                "scoreable",
                "choice",
            ] {
                assert!(
                    !bare.contains(forbidden),
                    "{name} looks like {forbidden}-derived input"
                );
            }
        }
    }

    #[test]
    fn opponents_contribute_counts_and_never_identities() {
        let (state, player) = position();
        let facts = facts_via_engine(&state, &player, CriticFeatures::full());

        // Two opponents, both on three points, so the spread names neither of them.
        let spread = facts
            .iter()
            .find(|(name, _)| name == "critic-state:opponent_victory_points:3")
            .expect("the spread is present");
        assert!((spread.1 - 2.0).abs() < f64::EPSILON, "{spread:?}");
        // No seat id appears as a name segment. Checked by segment rather than substring: an
        // earlier version used `contains(":b")` and flagged `critic-state:board_systems`.
        for (name, _) in &facts {
            for segment in name.split(':') {
                for seat in ["a", "b", "c"] {
                    assert_ne!(segment, seat, "{name} names seat {seat}");
                }
            }
        }
        // And the position is summarised relative to the leader without naming them.
        assert!(
            facts
                .iter()
                .any(|(name, _)| name == "critic-state:victory_points_behind_leader")
        );
    }

    #[test]
    fn the_gated_groups_are_absent_unless_enabled() {
        // §4.1: a `factual` ablation must not gain objective or ability signal through the critic.
        let (state, player) = position();

        let factual = names(&state, &player, CriticFeatures::factual());
        assert!(!factual.is_empty());
        assert!(
            !factual
                .iter()
                .any(|n| n.contains("objective_progress") || n.contains("ability:")),
            "the factual inventory carries gated signal"
        );

        let full = names(&state, &player, CriticFeatures::full());
        // Non-vacuity: enabling really does add something, so the absence above is meaningful.
        assert!(
            full.iter().any(|n| n.contains("critic-state:ability:")),
            "enabling abilities added nothing, so the gate proves nothing"
        );
        assert!(full.len() > factual.len());
        for name in &factual {
            assert!(
                full.contains(name),
                "{name} vanished when a group was enabled"
            );
        }
    }

    #[test]
    fn the_vector_is_ordered_and_deduplicated() {
        let (state, player) = position();
        let facts = facts_via_engine(&state, &player, CriticFeatures::full());
        let mut sorted = facts.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(facts, sorted, "the facts are not in a canonical order");
        let mut seen_names = std::collections::BTreeSet::new();
        for (name, _) in &facts {
            assert!(seen_names.insert(name.clone()), "{name} appears twice");
        }
    }

    #[test]
    fn two_bound_seats_receive_only_their_own_secret_progress() {
        // F-M09-027-1. The critic used to take a caller-supplied `held_secrets` slice, so nothing
        // stopped a caller from handing seat `a`'s critic seat `b`'s cards. Now the records come
        // off the capability, and the capability is minted by the engine per seat.
        //
        // Ground truth comes from the offline free function, which is authority-gated by full-state
        // possession — a test holds the whole state by design, so it may compute what each seat
        // *should* see and compare.
        use std::collections::BTreeSet;

        let content = ContentStore::embedded();
        let mut state = ti4_engine::fixtures::game(&["a", "b", "c"]);
        state.round = 3;
        // `ans` scores off the holder's own technologies; `dp` scores off the laws in play. Both
        // are given real progress, because a fixture where neither card registers would satisfy
        // every containment assertion below with two empty sets.
        //
        // `dp` is the sharper of the two: laws are *global* state, so seat `a` can read the same
        // board fact — but the card is `b`'s, and a progress family is a statement about a card in
        // hand. If `laws` appears in `a`'s critic, `b`'s hand leaked.
        {
            let seat = state.player_mut(&PlayerId::new("a")).unwrap();
            seat.secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("ans")];
            seat.technologies = ["t1", "t2"]
                .into_iter()
                .map(ti4_model::id::TechnologyId::new)
                .collect();
        }
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("dp")];
        state.laws = ["l1", "l2", "l3"]
            .into_iter()
            .map(|law| (law.to_owned(), law.to_owned()))
            .collect();

        let families = |seat: &str| -> BTreeSet<String> {
            ti4_engine::choice::held_secret_progress(
                &state,
                content,
                POK,
                None,
                &PlayerId::new(seat),
            )
            .into_iter()
            .filter(|card| card.threshold > 0.0 && card.have > 0.0)
            .map(|card| card.family_token)
            .collect()
        };
        let progress_names = |seat: &str| -> BTreeSet<String> {
            facts_via_engine(&state, &PlayerId::new(seat), CriticFeatures::full())
                .into_iter()
                .filter_map(|(name, _)| {
                    name.strip_prefix(&format!("{CRITIC_FAMILY}:objective_progress:"))
                        .map(str::to_owned)
                })
                .collect()
        };

        let (truth_a, truth_b) = (families("a"), families("b"));
        let (seen_a, seen_b) = (progress_names("a"), progress_names("b"));

        // Non-vacuity: if neither seat's secret contributes a progress family, the containment
        // assertions below are satisfied by two empty sets and prove nothing.
        assert!(
            !truth_a.is_empty() || !truth_b.is_empty(),
            "neither fixture secret produces progress, so this test is vacuous"
        );

        for (seat, truth, seen) in [("a", &truth_a, &seen_a), ("b", &truth_b, &seen_b)] {
            assert!(
                truth.is_subset(seen),
                "seat {seat} lost its own secret progress: {truth:?} against {seen:?}"
            );
        }
        // The load-bearing direction: what only the *other* seat holds must never appear.
        for (seat, mine, theirs, seen) in [
            ("a", &truth_a, &truth_b, &seen_a),
            ("b", &truth_b, &truth_a, &seen_b),
        ] {
            for family in theirs.difference(mine) {
                assert!(
                    !seen.contains(family),
                    "seat {seat}'s critic saw an opponent-only secret family {family}"
                );
            }
        }
    }
}
