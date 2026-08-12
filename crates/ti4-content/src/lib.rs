//! The TI4 content corpus: loading, provenance, and referential validation.
//!
//! The corpus is 28 categories of records extracted from `AsyncTI4` and copied verbatim from
//! the Python oracle at commit `37061c5` (see `content/CHECKSUMS.sha256`). Records keep
//! their upstream field names; interpretation belongs to the rules code that consumes a
//! category.
//!
//! ```
//! use ti4_content::ContentStore;
//! use ti4_model::content_types::{ContentType, POK};
//!
//! let content = ContentStore::embedded();
//! let carrier = content.get(ContentType::Units, "carrier").unwrap();
//! assert_eq!(carrier.int("moveValue"), Some(1));
//! assert_eq!(content.factions(POK).count(), 27);
//! ```

pub mod error;
pub mod factions;
pub mod galaxy;
pub mod loader;
pub mod manifest;
pub mod provenance;
pub mod record;
pub mod units;
pub mod validator;

pub use error::{ContentError, ReferenceError};
pub use factions::{Deployment, Faction, FleetError, Placement};
pub use galaxy::{Galaxy, GalaxyError, Planet, System, all_planets, all_systems};
pub use loader::ContentStore;
pub use manifest::{CategoryCounts, Manifest, Totals, Upstream};
pub use provenance::{CorpusDigest, digest_of, embedded_digest};
pub use record::Record;
pub use units::{UnitType, catalogue as unit_catalogue, faction_unit, unit_type};
pub use validator::{ValidationReport, validate};
