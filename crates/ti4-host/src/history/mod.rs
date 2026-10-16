//! Rewrites of the past of a game, played in memory: the dry run of a changed decision
//! ([`splice`]) and the redo of a seat's last turn with the auto-play of the round that followed
//! it ([`turn_redo`]). Nothing here touches a live session or storage; each host applies the
//! result to its own history.

pub mod splice;
pub mod turn_redo;
