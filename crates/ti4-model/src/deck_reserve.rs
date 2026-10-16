//! Card identity across a redone timeline (turn redo, H7).
//!
//! Decks are shuffled once at setup and drawn from the front. When a turn is redone and draws a
//! different number of cards than the original did, every later positional draw would hand a
//! different card to the seats whose recorded decisions follow. A [`DeckReserve`] on the state
//! changes *which* card a draw takes, only while a redo replay is running:
//!
//! * a draw made in decision interval `d` (the interval after decision `d` was answered) that
//!   the original timeline also made takes **the same card the original drew**, from wherever
//!   that card sits now (identity draw);
//! * every other draw (the redone turn's own, or one the original never made) skips the cards
//!   still reserved for later recorded draws and takes the next one further down the deck.
//!
//! A reserved card that is no longer in the deck is recorded as a [`ReserveFailure`]; the replay
//! turns it into a conflict. A normal game has no reserve (`None` on the state) and every draw
//! helper reduces to `Vec::remove(0)`.
//!
//! The reserve lives in the state by value, so a rolled-back transition (`*state = before`)
//! restores the pending reservations and the draw log together with the deck. Only the cursor
//! (which decision interval a draw belongs to) is shared and outside the state: the replay
//! driver moves it as it answers decisions.

use std::fmt::Display;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

/// Cursor value while no recorded decision has been answered yet or the replay is over.
pub const IDLE: usize = usize::MAX;

/// Cursor value once the recorded decisions are used up and answers are live.
pub const LIVE: usize = usize::MAX - 1;

/// `until` of a segment whose redone turn is still being played live: it governs the live
/// draws too (they skip every reserved card).
pub const OPEN: usize = usize::MAX;

/// One card taken from a deck, or reserved to be taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedDraw {
    /// Index of the decision whose interval the draw belongs to.
    pub tag: usize,
    /// Deck name: `action_card`, `secret`, `objective`, `relic`, `agenda`, `exploration:<kind>`.
    pub deck: String,
    pub card: String,
    /// The seat that received the card, when the draw site knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
}

/// The reservations of one redo: active while the cursor is in `from..until`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckSegment {
    /// Index of the redone turn's first decision.
    pub from: usize,
    /// First index no longer governed (end of the kept tail).
    pub until: usize,
    /// The original timeline's later draws, in order.
    pub reserved: Vec<ReservedDraw>,
}

/// All reservations a history carries, oldest redo first.
pub type DeckPlan = Vec<DeckSegment>;

/// A reserved card that could not be handed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveFailure {
    pub tag: usize,
    pub deck: String,
    pub card: String,
    pub recipient: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeckReserve {
    cursor: Arc<AtomicUsize>,
    pending: DeckPlan,
    draws: Vec<ReservedDraw>,
    failure: Option<ReserveFailure>,
}

impl DeckReserve {
    /// A reserve following `cursor`. An empty plan only records draws.
    #[must_use]
    pub fn new(cursor: Arc<AtomicUsize>, plan: DeckPlan) -> Self {
        Self {
            cursor,
            pending: plan,
            draws: Vec::new(),
            failure: None,
        }
    }

    /// A copy with its own cursor at the same position (for a fork or snapshot): the pending
    /// reservations and the draw log are copied by value, only the shared cursor is split.
    #[must_use]
    pub fn detached(&self) -> Self {
        Self {
            cursor: Arc::new(AtomicUsize::new(self.cursor.load(Ordering::Relaxed))),
            ..self.clone()
        }
    }

    /// Follow `cursor` instead of the current one, which is set to the current position first
    /// (a fork resumed under a new replay driver).
    pub fn rebind(&mut self, cursor: &Arc<AtomicUsize>) {
        cursor.store(self.cursor.load(Ordering::Relaxed), Ordering::Relaxed);
        self.cursor = Arc::clone(cursor);
    }

    /// Whether both reserves move the same cursor (true for a plain clone, false for a
    /// [`DeckReserve::detached`] copy).
    #[must_use]
    pub fn shares_cursor_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cursor, &other.cursor)
    }

    /// Where the cursor stands now (a decision index, [`IDLE`] or [`LIVE`]).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor.load(Ordering::Relaxed)
    }

    /// Every draw made so far (tagged with the cursor), oldest first.
    #[must_use]
    pub fn draws(&self) -> &[ReservedDraw] {
        &self.draws
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&ReserveFailure> {
        self.failure.as_ref()
    }

    fn take_index<T: Display>(
        &mut self,
        deck_name: &str,
        deck: &[T],
        pred: &dyn Fn(&T) -> bool,
        recipient: Option<&str>,
        from_back: bool,
    ) -> Option<usize> {
        let find = |ok: &dyn Fn(&T) -> bool| {
            if from_back {
                deck.iter().rposition(ok)
            } else {
                deck.iter().position(ok)
            }
        };
        let cursor = self.cursor.load(Ordering::Relaxed);
        let segment = if cursor == IDLE {
            None
        } else {
            self.pending.iter_mut().find(|s| {
                if cursor == LIVE {
                    s.until == OPEN
                } else {
                    s.from <= cursor && cursor < s.until
                }
            })
        };
        let Some(segment) = segment else {
            return find(pred);
        };
        if cursor != LIVE
            && let Some(slot) = segment
                .reserved
                .iter()
                .position(|r| r.tag == cursor && r.deck == deck_name)
        {
            let entry = segment.reserved.remove(slot);
            if let Some(at) = deck.iter().position(|c| c.to_string() == entry.card) {
                return Some(at);
            }
            if self.failure.is_none() {
                self.failure = Some(ReserveFailure {
                    tag: cursor,
                    deck: deck_name.to_owned(),
                    card: entry.card,
                    recipient: entry.recipient.or_else(|| recipient.map(str::to_owned)),
                });
            }
        }
        // Reserved for a later recorded draw of this deck (entries of earlier intervals that
        // were not drawn this time are released).
        let held: Vec<&str> = segment
            .reserved
            .iter()
            .filter(|r| r.deck == deck_name && (cursor == LIVE || r.tag > cursor))
            .map(|r| r.card.as_str())
            .collect();
        find(&|c| pred(c) && !held.contains(&c.to_string().as_str()))
    }

    /// Take the first card of `deck` that satisfies `pred` under the reserve's rules, logging
    /// the draw.
    pub fn take_where<T: Display>(
        &mut self,
        deck_name: &str,
        deck: &mut Vec<T>,
        recipient: Option<&str>,
        pred: &dyn Fn(&T) -> bool,
        from_back: bool,
    ) -> Option<T> {
        let index = self.take_index(deck_name, deck, pred, recipient, from_back)?;
        let card = deck.remove(index);
        let cursor = self.cursor.load(Ordering::Relaxed);
        if cursor != IDLE {
            self.draws.push(ReservedDraw {
                tag: cursor,
                deck: deck_name.to_owned(),
                card: card.to_string(),
                recipient: recipient.map(str::to_owned),
            });
        }
        Some(card)
    }
}

/// Take the first card of `deck` satisfying `pred`. Without a reserve this is exactly
/// `deck.iter().position(pred).map(|i| deck.remove(i))`.
pub fn take_where<T: Display>(
    reserve: &mut Option<DeckReserve>,
    deck_name: &str,
    deck: &mut Vec<T>,
    recipient: Option<&str>,
    pred: &dyn Fn(&T) -> bool,
) -> Option<T> {
    match reserve {
        None => {
            let at = deck.iter().position(pred)?;
            Some(deck.remove(at))
        }
        Some(r) => r.take_where(deck_name, deck, recipient, pred, false),
    }
}

/// Take the bottom card of `deck` (the last one).
pub fn take_bottom<T: Display>(
    reserve: &mut Option<DeckReserve>,
    deck_name: &str,
    deck: &mut Vec<T>,
    recipient: Option<&str>,
) -> Option<T> {
    match reserve {
        None => deck.pop(),
        Some(r) => r.take_where(deck_name, deck, recipient, &|_| true, true),
    }
}

/// Take the top card of `deck`.
pub fn take_top<T: Display>(
    reserve: &mut Option<DeckReserve>,
    deck_name: &str,
    deck: &mut Vec<T>,
    recipient: Option<&str>,
) -> Option<T> {
    take_where(reserve, deck_name, deck, recipient, &|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reserve(plan: DeckPlan, cursor: usize) -> (Option<DeckReserve>, Arc<AtomicUsize>) {
        let c = Arc::new(AtomicUsize::new(cursor));
        (Some(DeckReserve::new(Arc::clone(&c), plan)), c)
    }

    fn draw(tag: usize, card: &str) -> ReservedDraw {
        ReservedDraw {
            tag,
            deck: "d".into(),
            card: card.into(),
            recipient: None,
        }
    }

    fn seg(reserved: Vec<ReservedDraw>) -> DeckPlan {
        vec![DeckSegment {
            from: 0,
            until: 9,
            reserved,
        }]
    }

    fn deck() -> Vec<String> {
        ["a", "b", "c", "d", "e"].map(str::to_owned).to_vec()
    }

    #[test]
    fn without_a_reserve_the_top_card_comes_off() {
        let mut none = None;
        let mut d = deck();
        assert_eq!(take_top(&mut none, "d", &mut d, None).as_deref(), Some("a"));
        assert_eq!(d.len(), 4);
        let mut empty: Vec<String> = Vec::new();
        assert_eq!(take_top(&mut none, "d", &mut empty, None), None);
    }

    #[test]
    fn an_idle_cursor_draws_from_the_top_even_with_a_plan() {
        let (mut r, _c) = reserve(seg(vec![draw(5, "a")]), IDLE);
        let mut d = deck();
        assert_eq!(take_top(&mut r, "d", &mut d, None).as_deref(), Some("a"));
        assert!(r.unwrap().draws().is_empty());
    }

    #[test]
    fn new_draws_skip_reserved_cards_and_the_tail_gets_its_own() {
        let (mut r, c) = reserve(seg(vec![draw(5, "a"), draw(6, "b")]), 2);
        let mut d = deck();
        // The redone turn (cursor 2..5) draws past the reserved a and b.
        assert_eq!(
            take_top(&mut r, "d", &mut d, Some("p1")).as_deref(),
            Some("c")
        );
        assert_eq!(
            take_top(&mut r, "d", &mut d, Some("p1")).as_deref(),
            Some("d")
        );
        c.store(5, Ordering::Relaxed);
        assert_eq!(
            take_top(&mut r, "d", &mut d, Some("p2")).as_deref(),
            Some("a")
        );
        c.store(6, Ordering::Relaxed);
        assert_eq!(
            take_top(&mut r, "d", &mut d, Some("p3")).as_deref(),
            Some("b")
        );
        assert_eq!(d, vec!["e".to_owned()]);
        assert!(r.as_ref().unwrap().failure().is_none());
        // Past the segment the deck is drawn normally again.
        c.store(9, Ordering::Relaxed);
        assert_eq!(take_top(&mut r, "d", &mut d, None).as_deref(), Some("e"));
    }

    #[test]
    fn a_reserved_card_that_left_the_deck_is_a_failure() {
        let (mut r, _c) = reserve(seg(vec![draw(3, "zz")]), 3);
        let mut d = deck();
        let got = take_top(&mut r, "d", &mut d, Some("p2"));
        assert_eq!(
            got.as_deref(),
            Some("a"),
            "falls back so the state stays valid"
        );
        let f = r.unwrap().failure().cloned().unwrap();
        assert_eq!(
            (f.tag, f.card.as_str(), f.recipient.as_deref()),
            (3, "zz", Some("p2"))
        );
    }

    #[test]
    fn a_draw_the_tail_did_not_make_releases_its_card() {
        let (mut r, _c) = reserve(seg(vec![draw(2, "a")]), 4);
        let mut d = deck();
        assert_eq!(take_top(&mut r, "d", &mut d, None).as_deref(), Some("a"));
    }

    #[test]
    fn a_predicate_picks_the_first_matching_unreserved_card() {
        let (mut r, _c) = reserve(seg(vec![draw(4, "b")]), 1);
        let mut d = deck();
        let got = take_where(&mut r, "d", &mut d, None, &|c: &String| c.as_str() != "a");
        assert_eq!(got.as_deref(), Some("c"));
    }
}
