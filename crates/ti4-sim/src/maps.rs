//! Compatible readers for Python simulation map pools.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;
use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, GalaxyError, all_systems};
use ti4_model::content_types::SourceSet;
use ti4_model::hex::Hex;

const MAP_POOL_SCHEMA: &str = "ti4-map-pool-v1";
const MAX_DECOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARRANGEMENTS: usize = 65_536;
const MAX_COORDS: usize = 256;

#[derive(Debug, Deserialize)]
struct Payload {
    schema: String,
    effort: usize,
    coords: Vec<[i32; 2]>,
    slots: Vec<[i32; 2]>,
    arrangements: Vec<Vec<String>>,
}

/// A validated Python `ti4-map-pool-v1` artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPool {
    coords: Vec<Hex>,
    slots: Vec<Hex>,
    arrangements: Vec<Vec<String>>,
    effort: usize,
}

/// Why a map-pool artifact could not be used.
#[derive(Debug, thiserror::Error)]
pub enum MapPoolError {
    #[error("read map pool: {0}")]
    Io(#[from] io::Error),
    #[error("parse map pool: {0}")]
    Json(#[from] serde_json::Error),
    #[error("map pool expands beyond the {MAX_DECOMPRESSED_BYTES}-byte limit")]
    TooLarge,
    #[error("map pool schema {0:?} is unsupported")]
    Schema(String),
    #[error("map pool must contain at least one coordinate and arrangement")]
    Empty,
    #[error("map pool exceeds its structural limit")]
    StructuralLimit,
    #[error("map pool repeats coordinate ({0}, {1})")]
    DuplicateCoordinate(i32, i32),
    #[error("home slot ({0}, {1}) is not one of the pool coordinates")]
    UnknownSlot(i32, i32),
    #[error("map arrangement {index} has {found} tiles; expected {expected}")]
    ArrangementWidth {
        index: usize,
        found: usize,
        expected: usize,
    },
    #[error("map arrangement {index} repeats system {system:?}")]
    DuplicateSystem { index: usize, system: String },
    #[error("map arrangement {index} references unknown system {system:?}")]
    UnknownSystem { index: usize, system: String },
    #[error("map pool has {slots} home slots but {homes} faction homes were supplied")]
    HomeCount { slots: usize, homes: usize },
    #[error(transparent)]
    Galaxy(#[from] GalaxyError),
}

impl MapPool {
    /// Load JSON or JSON.GZ, selected by the `.gz` extension.
    ///
    /// # Errors
    /// Refuses oversized, malformed, or structurally inconsistent artifacts.
    pub fn load(path: &Path) -> Result<Self, MapPoolError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        if path.extension().is_some_and(|extension| extension == "gz") {
            Self::from_gzip_reader(reader)
        } else {
            Self::from_reader(reader)
        }
    }

    /// Parse an already-read pool buffer, dispatching on `path`'s extension exactly like
    /// [`Self::load`]. This is the unified boundary of MLP plan §10: role verification and
    /// parsing consume the same immutable bytes, so a file that changes after approval cannot
    /// be consumed.
    ///
    /// # Errors
    /// Refuses oversized, malformed, or structurally inconsistent artifacts.
    pub fn load_verified(path: &Path, bytes: &[u8]) -> Result<Self, MapPoolError> {
        if path.extension().is_some_and(|extension| extension == "gz") {
            Self::from_gzip_reader(io::Cursor::new(bytes))
        } else {
            Self::from_reader(io::Cursor::new(bytes))
        }
    }

    /// Parse a gzip-compressed pool stream.
    ///
    /// # Errors
    /// Refuses invalid gzip, oversized, malformed, or structurally inconsistent artifacts.
    pub fn from_gzip_reader(reader: impl Read) -> Result<Self, MapPoolError> {
        Self::from_reader(GzDecoder::new(reader))
    }

    /// Parse a pool from an already decompressed stream.
    ///
    /// # Errors
    /// Refuses oversized, malformed, or structurally inconsistent artifacts.
    pub fn from_reader(reader: impl Read) -> Result<Self, MapPoolError> {
        let mut bytes = Vec::new();
        reader
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_DECOMPRESSED_BYTES {
            return Err(MapPoolError::TooLarge);
        }
        let payload: Payload = serde_json::from_slice(&bytes)?;
        Self::from_payload(payload)
    }

    fn from_payload(payload: Payload) -> Result<Self, MapPoolError> {
        if payload.schema != MAP_POOL_SCHEMA {
            return Err(MapPoolError::Schema(payload.schema));
        }
        if payload.coords.is_empty() || payload.arrangements.is_empty() {
            return Err(MapPoolError::Empty);
        }
        if payload.coords.len() > MAX_COORDS || payload.arrangements.len() > MAX_ARRANGEMENTS {
            return Err(MapPoolError::StructuralLimit);
        }
        let coords: Vec<Hex> = payload
            .coords
            .into_iter()
            .map(|[q, r]| Hex::new(q, r))
            .collect();
        let mut unique = BTreeSet::new();
        for coord in &coords {
            if !unique.insert(*coord) {
                return Err(MapPoolError::DuplicateCoordinate(coord.q, coord.r));
            }
        }
        let slots: Vec<Hex> = payload
            .slots
            .into_iter()
            .map(|[q, r]| Hex::new(q, r))
            .collect();
        for slot in &slots {
            if !unique.contains(slot) {
                return Err(MapPoolError::UnknownSlot(slot.q, slot.r));
            }
        }
        for (index, arrangement) in payload.arrangements.iter().enumerate() {
            if arrangement.len() != coords.len() {
                return Err(MapPoolError::ArrangementWidth {
                    index,
                    found: arrangement.len(),
                    expected: coords.len(),
                });
            }
            let mut systems = BTreeSet::new();
            for system in arrangement {
                if !systems.insert(system) {
                    return Err(MapPoolError::DuplicateSystem {
                        index,
                        system: system.clone(),
                    });
                }
            }
        }
        Ok(Self {
            coords,
            slots,
            arrangements: payload.arrangements,
            effort: payload.effort,
        })
    }

    /// Number of stored arrangements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.arrangements.len()
    }

    /// Whether the pool contains no arrangements. Valid pools are never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arrangements.is_empty()
    }

    /// Balancing effort recorded by the producer.
    #[must_use]
    pub const fn effort(&self) -> usize {
        self.effort
    }

    /// Number of physical home-system slots captured by each arrangement.
    #[must_use]
    pub fn home_slots(&self) -> usize {
        self.slots.len()
    }

    /// Validate every stored system id against the selected content scope.
    ///
    /// # Errors
    /// Returns the first arrangement and system absent from the content catalogue.
    pub fn validate_systems(
        &self,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Result<(), MapPoolError> {
        let catalogue = all_systems(content, sources);
        for (index, arrangement) in self.arrangements.iter().enumerate() {
            for system in arrangement {
                if !catalogue.contains_key(system.as_str()) {
                    return Err(MapPoolError::UnknownSystem {
                        index,
                        system: system.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Select exactly as Python does: `tile_seed % len(pool)`.
    #[must_use]
    pub fn draw(&self, tile_seed: u64) -> &[String] {
        let count = u64::try_from(self.arrangements.len()).unwrap_or(u64::MAX);
        let index = usize::try_from(tile_seed % count).unwrap_or(0);
        &self.arrangements[index]
    }

    /// Build one galaxy, replacing the pool's captured home tiles by physical-slot order.
    ///
    /// # Errors
    /// Refuses a home-count mismatch or any unknown/duplicate system in the resulting galaxy.
    pub fn galaxy(
        &self,
        content: &ContentStore,
        sources: SourceSet,
        tile_seed: u64,
        home_systems: &[&str],
    ) -> Result<Galaxy, MapPoolError> {
        if home_systems.len() != self.slots.len() {
            return Err(MapPoolError::HomeCount {
                slots: self.slots.len(),
                homes: home_systems.len(),
            });
        }
        let arrangement = self.draw(tile_seed);
        let placed: Vec<(String, Hex)> = self
            .coords
            .iter()
            .zip(arrangement)
            .map(|(coord, system)| {
                let system = self
                    .slots
                    .iter()
                    .position(|slot| slot == coord)
                    .map_or_else(|| system.clone(), |index| home_systems[index].to_owned());
                (system, *coord)
            })
            .collect();
        let borrowed: Vec<(&str, Hex)> = placed
            .iter()
            .map(|(system, coord)| (system.as_str(), *coord))
            .collect();
        Ok(Galaxy::placed(content, &borrowed, sources)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use ti4_model::content_types::FULL;

    fn payload(arrangements: &str) -> String {
        format!(
            r#"{{"schema":"ti4-map-pool-v1","effort":2000,"coords":[[0,0],[0,-3],[-3,3],[3,0]],"slots":[[0,-3],[-3,3],[3,0]],"arrangements":{arrangements}}}"#
        )
    }

    #[test]
    fn deterministic_draw_and_home_replacement_match_python_contract() {
        let json = payload(r#"[["18","10","12","16"],["18","01","02","03"]]"#);
        let pool = MapPool::from_reader(json.as_bytes()).unwrap();
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.effort(), 2000);
        assert_eq!(pool.draw(0)[1], "10");
        assert_eq!(pool.draw(3)[1], "01", "3 % 2 selects arrangement one");

        let galaxy = pool
            .galaxy(ContentStore::embedded(), FULL, 3, &["10", "12", "16"])
            .unwrap();
        assert_eq!(galaxy.coord_of("10"), Some(Hex::new(0, -3)));
        assert_eq!(galaxy.coord_of("12"), Some(Hex::new(-3, 3)));
        assert_eq!(galaxy.coord_of("16"), Some(Hex::new(3, 0)));
    }

    #[test]
    fn reads_the_json_gzip_artifact_shape() {
        let json = payload(r#"[["18","10","12","16"]]"#);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(json.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let pool = MapPool::from_gzip_reader(compressed.as_slice()).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.draw(100)[0], "18");
    }

    #[test]
    fn rejects_wrong_schema_width_duplicates_and_unknown_slots() {
        let wrong_schema = payload(r#"[["18","10","12","16"]]"#).replace(MAP_POOL_SCHEMA, "future");
        assert!(matches!(
            MapPool::from_reader(wrong_schema.as_bytes()),
            Err(MapPoolError::Schema(_))
        ));

        let width = payload(r#"[["18","10","12"]]"#);
        assert!(matches!(
            MapPool::from_reader(width.as_bytes()),
            Err(MapPoolError::ArrangementWidth { .. })
        ));

        let duplicate = payload(r#"[["18","10","10","16"]]"#);
        assert!(matches!(
            MapPool::from_reader(duplicate.as_bytes()),
            Err(MapPoolError::DuplicateSystem { .. })
        ));

        let unknown_slot = r#"{"schema":"ti4-map-pool-v1","effort":1,"coords":[[0,0],[0,-3],[-3,3],[3,0]],"slots":[[0,-3],[-3,3],[2,0]],"arrangements":[["18","10","12","16"]]}"#;
        assert!(matches!(
            MapPool::from_reader(unknown_slot.as_bytes()),
            Err(MapPoolError::UnknownSlot(2, 0))
        ));

        let unknown_system = payload(r#"[["18","10","12","nonesuch"]]"#);
        let pool = MapPool::from_reader(unknown_system.as_bytes()).unwrap();
        assert!(matches!(
            pool.validate_systems(ContentStore::embedded(), FULL),
            Err(MapPoolError::UnknownSystem {
                index: 0,
                system,
            }) if system == "nonesuch"
        ));
    }
}
