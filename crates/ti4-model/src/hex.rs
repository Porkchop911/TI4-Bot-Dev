//! Hex geometry.
//!
//! Coordinates are axial `(q, r)`, with the third cube coordinate `s` implied by the other
//! two. This module knows nothing about systems or content — it is the grid the galaxy sits
//! on, and it is pure arithmetic so that adjacency can be derived rather than stored.

use serde::{Deserialize, Serialize};

/// Axial directions, clockwise from east.
pub const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// An axial hex coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl Hex {
    /// The origin.
    pub const ORIGIN: Self = Self { q: 0, r: 0 };

    #[must_use]
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// The third cube coordinate, implied by the other two.
    #[must_use]
    pub const fn s(self) -> i32 {
        -self.q - self.r
    }

    /// The six hexes one step away, clockwise from east.
    #[must_use]
    pub fn neighbours(self) -> [Self; 6] {
        DIRECTIONS.map(|(dq, dr)| Self::new(self.q + dq, self.r + dr))
    }

    /// Cube distance, without materialising the third coordinate.
    ///
    /// `s` is derived as `-q - r`, so `self.s - other.s` is exactly
    /// `-((self.q - other.q) + (self.r - other.r))` and its absolute value is the same.
    /// The oracle records this as a measured optimisation: distance is called 139,000 times
    /// in a round-4 six-player game.
    #[must_use]
    pub const fn distance(self, other: Self) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
    }

    /// The hexes exactly `radius` steps from the centre, clockwise.
    ///
    /// Starts at the south-west corner and walks each of the six sides, which fixes the
    /// order. That order is load-bearing: [`Self::spiral`] places tiles by it.
    #[must_use]
    pub fn ring(radius: i32) -> Vec<Self> {
        if radius <= 0 {
            return vec![Self::ORIGIN];
        }
        let mut out = Vec::with_capacity(radius.unsigned_abs() as usize * 6);
        let mut current = Self::new(-radius, radius);
        for (dq, dr) in DIRECTIONS {
            for _ in 0..radius {
                out.push(current);
                current = Self::new(current.q + dq, current.r + dr);
            }
        }
        out
    }

    /// Centre outwards, ring by ring.
    #[must_use]
    pub fn spiral(rings: i32) -> Vec<Self> {
        (0..=rings.max(0)).flat_map(Self::ring).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn a_hex_has_six_neighbours_all_one_step_away() {
        let centre = Hex::ORIGIN;
        let neighbours = centre.neighbours();
        assert_eq!(neighbours.len(), 6);
        for n in neighbours {
            assert_eq!(centre.distance(n), 1);
        }
        assert_eq!(neighbours.iter().collect::<BTreeSet<_>>().len(), 6);
    }

    #[test]
    fn cube_coordinates_are_consistent() {
        for hex in Hex::spiral(3) {
            assert_eq!(hex.q + hex.r + hex.s(), 0);
        }
    }

    #[test]
    fn ring_sizes_are_six_times_the_radius() {
        assert_eq!(Hex::ring(0).len(), 1);
        assert_eq!(Hex::ring(1).len(), 6);
        assert_eq!(Hex::ring(2).len(), 12);
        assert_eq!(Hex::ring(3).len(), 18);
    }

    #[test]
    fn ring_members_are_all_at_the_stated_distance() {
        for radius in 0..5 {
            for hex in Hex::ring(radius) {
                assert_eq!(Hex::ORIGIN.distance(hex), radius);
            }
        }
    }

    #[test]
    fn spiral_covers_every_ring_without_duplicates() {
        let spiral = Hex::spiral(3);
        assert_eq!(spiral.len(), 1 + 6 + 12 + 18);
        assert_eq!(spiral.iter().collect::<BTreeSet<_>>().len(), spiral.len());
        assert_eq!(spiral[0], Hex::ORIGIN, "the centre is placed first");
    }

    #[test]
    fn distance_is_symmetric_and_zero_to_itself() {
        let spiral = Hex::spiral(2);
        for a in &spiral {
            assert_eq!(a.distance(*a), 0);
            for b in &spiral {
                assert_eq!(a.distance(*b), b.distance(*a));
            }
        }
    }

    #[test]
    fn distance_obeys_the_triangle_inequality() {
        let spiral = Hex::spiral(2);
        for a in &spiral {
            for b in &spiral {
                for c in &spiral {
                    assert!(a.distance(*c) <= a.distance(*b) + b.distance(*c));
                }
            }
        }
    }

    #[test]
    fn a_negative_radius_is_the_centre_alone() {
        assert_eq!(Hex::ring(-1), vec![Hex::ORIGIN]);
        assert_eq!(Hex::spiral(-1), vec![Hex::ORIGIN]);
    }
}
