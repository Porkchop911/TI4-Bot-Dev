//! The secrets no planner can honestly steer, with the reason for each. They are not planned
//! (a made-up plan would only add noise to the weighing); the default logic plays them as before.
//!
//! # Objectives (exactly; checked by `lists_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | why it is unsteerable |
//! |---|---|---|---|---|
//! | `bam` | Become a Martyr | secret | 1 | "Lose control of a planet in a home system": an anti-goal; the only route is to lose a home planet to a rival, which costs more than the point |
//! | `pe` | Prove Endurance | secret | 1 | "Be the last player to pass": scored 41% of the time unsteered; outlasting the table means spending command tokens on actions worth nothing, which costs more than the point |

/// Alias and reason of every unsteerable secret, as documented at the top of the file.
pub const UNSTEERABLE: &[(&str, &str)] = &[
    ("bam", "anti-goal: scored only by losing a home planet"),
    (
        "pe",
        "outlasting the table wastes command tokens; already scored 41% unsteered",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backward::coverage::{covered_by, rows};

    #[test]
    fn lists_exactly_the_documented_objectives() {
        let mut listed: Vec<&str> = UNSTEERABLE.iter().map(|(a, _)| *a).collect();
        listed.sort_unstable();
        assert_eq!(listed, ["bam", "pe"]);
        // And no planner covers them.
        for row in rows() {
            if listed.contains(&row.alias.as_str()) {
                assert_eq!(covered_by(&row), None, "{} is planned", row.alias);
            }
        }
    }

    #[test]
    fn every_other_objective_has_a_planner() {
        let uncovered: Vec<String> = rows()
            .into_iter()
            .filter(|r| covered_by(r).is_none())
            .map(|r| r.alias)
            .filter(|a| !UNSTEERABLE.iter().any(|(u, _)| u == a))
            .collect();
        assert!(uncovered.is_empty(), "uncovered: {uncovered:?}");
    }
}
