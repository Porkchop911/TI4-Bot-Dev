# M02 — Content and model

## Goal

Represent all state used by the current branch and load the full language-neutral content corpus.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M02-001 | Identifier newtypes | M01 | content aliases across `engine/` | Typed player/faction/system/planet/unit/card/ability/objective IDs; parse/display/serde tests. |
| M02-002 | Common schema envelope | 001 | manifests and training schemas | Version, provenance, content hash, RNG version, and compatibility metadata; unknown versions fail clearly. |
| M02-003 | Unit model | 001 | `engine/units.py` | Unit identity, ownership, type, damage, upgrade facts; golden round trips. |
| M02-004 | System state | 001,003 | `engine/state.py` | Space units, planet units/control, command tokens; mutation helpers preserve conservation properties. |
| M02-005 | Player core state | 001 | `engine/state.py` | Economy, tokens, cards, tech, objectives, leaders, relics, faction fields; every Python field mapped. |
| M02-006 | Effect-scope state | 005 | scoped flags in `state.py` | Activation/combat/production/round sequence fields represented without boolean leakage. |
| M02-007 | Game state | 004–006 | `engine/state.py` | Phase, order, decks, laws, systems, public/hidden state, finish metadata; canonical serialization. |
| M02-008 | Hidden views | 005,007 | `engine/views.py`, `tests/test_views.py` | Per-player redaction by type/API; property test proves another hand/secret cannot be observed. |
| M02-009 | Content raw schemas | 001,002 | `engine/content/*.json` | Deserialize all 26 categories without loss; unknown fields policy documented. |
| M02-010 | Content indexes | 009 | `engine/content.py` | Alias/category/source lookup with deterministic iteration and duplicate rejection. |
| M02-011 | Referential validation | 009,010 | extraction manifest | Cross-reference factions, units, tech, planets, systems, decks, leaders, abilities; expected gaps allowlisted. |
| M02-012 | Content provenance/hash | 002,009 | `manifest.json` | Canonical content digest and upstream provenance; repeat loads agree. |
| M02-013 | State canonicalizer | 007,008 | M00 projection spec | Stable state/view JSON independent of internal map layout. |
| M02-014 | Model properties | 003–008 | state/unit tests | Generated add/remove/control/token operations preserve invariants or return typed errors. |
| M02-015 | Content differential suite | 009–013 | content tests | Counts, aliases, source filters, representative records, and hashes match the oracle. |
| M02-016 | Frontier model review | 001–015 | — | Review hidden information, identifier boundaries, schemas, deterministic ordering, and missing fields. |

## Exit gate

All existing content loads and validates, every branch-used state field has a Rust representation,
hidden views are enforced, and canonical projections match M00 fixtures.

