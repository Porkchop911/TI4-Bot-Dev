//! Structural agenda-phase bookkeeping.

use ti4_model::id::{PlanetId, PlayerId};
use ti4_model::state::{GameState, Phase};

/// LRR 8.2 and 8.3 reveal two agenda cards in each agenda phase.
pub const AGENDAS_PER_PHASE: usize = 2;

/// The deliberately incomplete structural disposition of a revealed agenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaResolution {
    /// Voting, tie-breaking, laws, and card effects require M04-012's choice driver.
    Deferred,
}

/// One revealed agenda and the voting order it will use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealedAgenda {
    /// The agenda alias drawn from the top of the deck.
    pub alias: String,
    /// Voting order: 8.2ii starts to the speaker's *left* and goes clockwise, so the speaker is
    /// last. This is a report field; the order that actually drives a vote is `VoteWindow::new`,
    /// which has always been right. The two disagreed, and this one was labelled 8.5 while
    /// listing the speaker first.
    pub voting_order: Vec<PlayerId>,
    /// The known structural resolution boundary.
    pub resolution: AgendaResolution,
}

/// The observable structural changes from one agenda phase.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgendaPhaseReport {
    /// Agendas drawn from the deck, in reveal order.
    pub agendas: Vec<RevealedAgenda>,
    /// Exhausted planets readied after both agenda slots at LRR 8.4.
    pub readied_planets: Vec<PlanetId>,
}

/// An agenda phase was requested when it cannot legally occur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgendaPhaseError {
    #[error("cannot resolve agenda bookkeeping while in {0:?} phase")]
    WrongPhase(Phase),
    #[error("the agenda phase cannot begin before custodians are removed")]
    CustodiansNotRemoved,
}

/// Resolve the deterministic, choice-free structure of LRR 8.2 to 8.4.
///
/// Each available agenda is removed from the deck and exposes speaker-clockwise voting order.
/// It is recorded [`AgendaResolution::Deferred`] instead of inventing a vote, tie-break, law,
/// directive effect, or timing window. The phase still readies planets once its two agenda slots
/// have been processed, including when the deck is empty.
///
/// # Errors
/// [`AgendaPhaseError::WrongPhase`] unless `state` is in [`Phase::Agenda`], or
/// [`AgendaPhaseError::CustodiansNotRemoved`] before the agenda-phase entry condition holds.
pub fn resolve_agenda_phase(state: &mut GameState) -> Result<AgendaPhaseReport, AgendaPhaseError> {
    if state.phase != Phase::Agenda {
        return Err(AgendaPhaseError::WrongPhase(state.phase));
    }
    if !state.custodians_removed {
        return Err(AgendaPhaseError::CustodiansNotRemoved);
    }

    let mut voting_order = state.clockwise_from(&state.speaker);
    if !voting_order.is_empty() {
        voting_order.rotate_left(1); // 8.2ii: start to the speaker's left, so the speaker is last
    }
    let mut report = AgendaPhaseReport::default();
    for _ in 0..AGENDAS_PER_PHASE {
        let Some(alias) = state.agenda_deck.first().cloned() else {
            break;
        };
        state.agenda_deck.remove(0);
        report.agendas.push(RevealedAgenda {
            alias,
            voting_order: voting_order.clone(),
            resolution: AgendaResolution::Deferred,
        });
    }

    // 8.4 is after both slots, not after each individual agenda.
    // Checks and Balances (Against): "Each player readies only 3 of their planets at the end of
    // this agenda phase." 8.4 readies everything otherwise.
    if let Some(limit) = crate::laws::agenda_ready_limit(state) {
        let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
        let mut readied = Vec::new();
        for player in seats {
            let mut theirs: Vec<PlanetId> = state
                .controlled_planets(&player)
                .into_iter()
                .map(|(_, planet)| planet.clone())
                .filter(|planet| state.exhausted_planets.contains(planet))
                .collect();
            theirs.truncate(limit);
            for planet in theirs {
                state.exhausted_planets.remove(&planet);
                readied.push(planet);
            }
        }
        report.readied_planets = readied;
        return Ok(report);
    }
    report.readied_planets = state.exhausted_planets.iter().cloned().collect();
    state.ready_all_planets();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{PlanetId, PlayerId};
    use ti4_model::state::Phase;

    use super::*;
    use crate::setup::start_game;

    #[test]
    fn agenda_reveals_two_cards_uses_speaker_order_and_then_readies_planets() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let mut state = start_game(
            ContentStore::embedded(),
            &players,
            POK,
            Some(PlayerId::new("b")),
        )
        .unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.exhaust_planet(PlanetId::new("jord"));

        let report = resolve_agenda_phase(&mut state).unwrap();

        assert_eq!(report.agendas.len(), 2);
        assert!(report.agendas.iter().all(|agenda| {
            // Speaker b votes last (8.2ii): the order starts to their left.
            agenda.voting_order == vec![PlayerId::new("c"), PlayerId::new("a"), PlayerId::new("b")]
        }));
        assert!(
            report
                .agendas
                .iter()
                .all(|agenda| agenda.resolution == AgendaResolution::Deferred)
        );
        assert_eq!(report.readied_planets, vec![PlanetId::new("jord")]);
        assert!(state.exhausted_planets.is_empty());
    }

    #[test]
    fn an_empty_agenda_deck_still_finishes_by_readying_planets() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.agenda_deck.clear();
        state.exhaust_planet(PlanetId::new("jord"));

        let report = resolve_agenda_phase(&mut state).unwrap();

        assert!(report.agendas.is_empty());
        assert_eq!(report.readied_planets, vec![PlanetId::new("jord")]);
        assert!(state.exhausted_planets.is_empty());
    }

    #[test]
    fn an_illegal_agenda_entry_is_atomic() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let before = state.clone();
        assert_eq!(
            resolve_agenda_phase(&mut state),
            Err(AgendaPhaseError::WrongPhase(Phase::Strategy))
        );
        assert!(state.identical(&before));

        state.phase = Phase::Agenda;
        let before = state.clone();
        assert_eq!(
            resolve_agenda_phase(&mut state),
            Err(AgendaPhaseError::CustodiansNotRemoved)
        );
        assert!(state.identical(&before));
    }
}
