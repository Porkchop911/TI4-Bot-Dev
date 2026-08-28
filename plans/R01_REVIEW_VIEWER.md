# R01 — Offline game review viewer

## Status and scope

This is an operator-authorized, **independent optional tooling track**. It does not change the
M00–M13 execution order, does not gate M10 training, and has no TTS dependency. The viewer reads
one validated artifact and cannot make an engine decision or mutate a game.

The source design was Pi session `2026-08-19T11-26-54-269Z_01a019c6-313c-7c2e-9f35-9cde967d20f3`.
This document supersedes that conversational sketch as the durable plan.

## Product decision

Deliver a Windows-friendly, offline HTML/SVG game viewer. It opens an immutable review artifact
for exactly one audience (`public`, one named seat, or an explicitly authorized referee), shows a
board and timeline, and reports incomplete/corrupt captures honestly. It has no server, network
access, live-TTS integration, or training runtime dependency.

The only integration point is an optional capture/export adapter. Training and simulation continue
to work unchanged when capture is disabled; selected training games may use the adapter later.

## Implementation status (2026-08-28)

At the operator's direction, the independently usable viewer path was implemented in one pass
without the migration project's package/review cadence. `ti4-review` is a new standalone Rust
executable with no engine, simulator, training, bridge, browser, or server dependency. It validates
one canonical `*.ti4review.json` bundle and renders a self-contained local HTML/SVG page with a
hex board, unit/planet labels, player panels, timeline controls, and per-frame facts.

```text
ti4-review example sample.ti4review.json
ti4-review validate sample.ti4review.json
ti4-review render sample.ti4review.json sample.html
```

The optional engine capture adapter remains intentionally separate: the current M03 replay module
is still a stub, and this viewer must not pretend raw engine records are a safe projection. Any
future adapter feeds this completed viewer contract; it does not alter training or TTS behaviour.

## Architecture

```text
audited or scripted game --optional capture--> ReviewBundle --offline--> ti4-review HTML/SVG
```

`ReviewBundle` is the sole cross-boundary contract. It contains a versioned manifest, deterministic
decision/event ordering, normalized render frames, a terminal status, and checksums. The viewer
does not deserialize `GameState`, call the engine, or derive rules from events.

Separate artifacts are generated per audience. A public HTML file must never contain private data
in JSON, comments, DOM attributes, or unused assets. A privileged view is a separately exported
artifact, not a client-side toggle.

## R01-001 — ReviewBundle v1 contract

### Package specification

- **Milestone/dependencies:** R01 independent tooling. R01-001 and R01-002 do not depend on
  training or replay playback; R01-003 cannot begin until the M03 replay boundary it uses is
  actually accepted. It is deliberately independent of M09/M10 training packages.
- **Objective:** Define the strict, bounded, role-specific review artifact used by both exporter
  and viewer.
- **Normative sources:** this plan; `plans/M03_CHOICE_TIMING_REPLAY.md` rows 005, 008, and 013;
  `plans/SCOPED_PERMISSIONS.md`; accepted Rust hidden-information boundaries.
- **Compatibility:** new schema `review-bundle` v1; no Python compatibility claim.
- **Permission:** P1. Writable paths: `plans/R01_REVIEW_VIEWER.md`, `plans/evidence/R01-001.md`,
  `plans/INDEX.md`, `plans/EXECUTION_STATE.md`. No network, process, port, or generated artifact.
- **Inputs/outputs:** a typed, audience-projected presentation DTO in; a deterministic
  `ReviewBundle` v1 specification out. Raw `GameState`, `DecisionRecord`, and `Event` are not
  bundle inputs.
- **Non-goals:** capture code, engine changes, training integration, a viewer implementation,
  browser/server dependencies, and TTS.

### Container, canonical form, and integrity scope

A v1 bundle is exactly one regular `*.ti4review.json` file, encoded as UTF-8. It is neither an
archive nor a directory: compression, paths, external URLs, embedded HTML/SVG, and extra members
are not part of v1. The file is at most 67,108,864 bytes (64 MiB), has JSON nesting depth at most
16, and is encoded in the JSON Canonicalization Scheme (RFC 8785). Integers are the only numbers;
non-finite values and a fractional or exponent numeric spelling are invalid. A validator parses
with duplicate-property detection, rejects unknown properties at every level, re-canonicalizes the
complete value, and rejects an input whose bytes are not precisely the canonical UTF-8 bytes.

The root has exactly these properties:

```json
{"manifest": Manifest, "payload": Payload}
```

`Manifest` has exactly these properties:

| Property | Exact v1 type / rule |
|---|---|
| `schema` | the literal `"ti4-review-bundle"` |
| `schema_version` | integer `1` |
| `generator_version` | ASCII SemVer, 1-64 bytes |
| `engine_revision` | 40 lowercase hexadecimal characters |
| `content_sha256`, `map_sha256`, `payload_sha256` | 64 lowercase hexadecimal SHA-256 strings |
| `source_kind` | `"audited"` or `"scripted"` |
| `audience` | `{"kind":"public"}`, `{"kind":"seat","seat":Id}`, or `{"kind":"referee"}` |
| `frame_count`, `timeline_count` | unsigned integers; `frame_count = timeline_count + 1`, and `timeline_count <= 4,095` |
| `terminal` | one of the closed terminal variants below |

The canonical representation of `manifest` is at most 65,536 bytes. An individual canonical
`Frame` is at most 524,288 bytes and an individual timeline entry at most 16,384 bytes; these caps
apply before hashing or rendering as well as to the total-file and payload caps.

`payload_sha256` is SHA-256 of the canonical UTF-8 representation of the `payload` value only.
It detects accidental or malicious payload alteration **only when the manifest itself is trusted**;
v1 provides no signature, key-distribution, origin-authentication, or provenance guarantee. There
is deliberately no seed, entropy stream, raw private-state digest, raw event digest, or raw
decision digest in any audience's bundle. `content_sha256` and `map_sha256` name non-secret
immutable inputs only.

An unsupported `schema_version` fails closed. A schema revision that changes a field, limit, or
meaning requires a new version; v1 readers neither ignore new properties nor attempt a forward
conversion.

### Audience boundary

The exporter first constructs `ReviewProjection { audience, frames, timeline }`; only this typed
projection may be passed to `export_review_bundle`. The export boundary does not accept
`GameState`, `DecisionRecord`, `Event`, an RNG/seed, or an arbitrary JSON value. Raw engine record
IDs and trace-local event IDs never cross it. Every frame, timeline fact, display identifier, and
hash is derived from the projection for the one named audience.

| Bundle area | Public | Seat | Referee |
|---|---|---|---|
| Manifest identity, content/map fingerprints, terminal status | Public-only values; no entropy or private fingerprints | Same public values plus the recipient's opaque seat ID | Same, explicitly labeled referee; still no entropy/raw log IDs |
| Board, counters, cards, objectives, units | Only publicly observable projected values | Public projection plus this seat's own projected private values | Full referee presentation projection |
| Timeline, decision/event summaries, narrative facts | Only facts and labels observable to all players; no hidden offers or selections | Public facts plus facts observable to this seat | Full referee presentation facts |
| `state_sha256` and `payload_sha256` | Hashes of the public projection only | Hashes of that seat projection only | Hashes of referee projection only |
| Viewer assets and DOM | Generic viewer only; no artifact-specific private data | Same | Same |

There is no audience selector, hidden JSON field, comment, data attribute, unused asset, client-side
filter, or post-export privilege escalation. A separate bundle is exported for every audience.
`Id` means 1-128 ASCII bytes matching `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`; it is a review-display
identifier, not an engine identifier. All human-facing strings are NFC-normalized UTF-8 and at
most 512 bytes.

### Payload and frame semantics

`Payload` has exactly `frames` and `timeline` arrays. It is at most 66,060,288 canonical bytes
(63 MiB). There are no deltas in v1: every frame carries its complete projected `ReviewState`.
This deliberately trades some size for a simple, independently verifiable first artifact format.

`frames` is an array of 1-4,096 `Frame` values with contiguous `id` values beginning at zero.
Frame zero has `cause = {"kind":"initial"}`. Every later frame has
`cause = {"kind":"timeline","index": n}` for exactly one timeline entry, where `n + 1` is the
frame ID. A `Frame` has exactly `id`, `cause`, `state`, and `state_sha256`; the latter is SHA-256
of the canonical UTF-8 representation of its projected `state`.

`timeline` is an array of 1-4,095 entries with contiguous `index` values beginning at zero. Each
entry has exactly `index`, `kind`, `frame`, and `facts`: `frame = index + 1`; `kind` is exactly one
of `"decision"`, `"event"`, `"phase"`, or `"terminal"`; and `facts` is an ordered array of at
most 32 `{ "label": String, "value": String }` values. The final entry is exactly `"terminal"`;
it is the only terminal entry. Capture emits one entry for
each non-forced decision and each phase transition; it may emit audience-safe event entries.
Each entry represents the post-transition state in its frame, so a timeline selection has no
ambiguous delta chain or raw-record reference.

`ReviewState` is a closed presentation DTO, not `GameState`, with exactly `round`, `phase`,
`players`, and `systems`:

| Field | Exact v1 form and bounds |
|---|---|
| `round` | integer 0-1,000 |
| `phase` | `"setup"`, `"strategy"`, `"action"`, `"status"`, `"agenda"`, `"finished"`, or `"error"` |
| `players` | 2-8 `ReviewPlayer` values, strictly ascending by `seat`; each has exactly `seat: Id`, `name: String`, `faction: Id`, `score: 0..20`, `resources: 0..999`, `influence: 0..999`, `trade_goods: 0..999`, `strategy_cards: [Id]`, and `items: [DisplayItem]`; all per-player lists have at most 128 elements |
| `systems` | 0-256 `ReviewSystem` values, strictly ascending by `(q, r)`; each has exactly `q: -32..32`, `r: -32..32`, `kind: "home"|"centre"|"normal"|"hyperlane"`, `tile: DisplayItem`, `planets: [ReviewPlanet]`, and `units: [ReviewUnit]`; at most 8 planets and 256 units per system |

`DisplayItem` has exactly `id: Id`, `label: String`, and `count: 0..999`; `ReviewPlanet` has
exactly `id: Id`, `label: String`, `owner: Id | null`, `exhausted: boolean`, `resources: 0..99`,
and `influence: 0..99`; `ReviewUnit` has exactly `owner: Id`, `kind: Id`, `count: 1..999`, and
`damaged: boolean`. IDs must be unique within each appropriate list; no maps or arbitrary JSON
values occur anywhere in the payload. The exporter maps raw card/objective/unit identities to
audience-safe `DisplayItem`s before serialization. Arrays have the listed ordering; a renderer
must not rely on map iteration, locale, DOM order, or wall time. Board geometry is derived only
from integer axial `(q, r)` coordinates and `kind`; v1 has no arbitrary transforms or rotations.
Within a player, `strategy_cards` and `items` are strictly ascending by ID; planets are strictly
ascending by ID; units are strictly ascending by `(owner, kind, damaged)`; and timeline facts keep
their exporter-defined semantic order. These are data orders, never locale-dependent display sorts.
Player seats are globally unique; `(q, r)` system coordinates and planet IDs are globally unique;
`DisplayItem` IDs are unique within their containing list. Every non-null planet owner, unit owner,
completed-game winner, and seat-audience recipient references one listed player seat. All such
reference checks happen before rendering.

The closed `terminal` variants are `{"kind":"completed","winner":Id|null}`,
`{"kind":"horizon_reached"}`, `{"kind":"capture_failed","code":"capture_limit"|"export_failed"|"aborted"}`,
and `{"kind":"engine_failed","code":"engine_error"|"replay_error"}`. Only `completed` is a
completed game; every other value is rendered as incomplete or failed, never as a winner.

### Validation and rendering limits

Validation completes before any frame is rendered. In addition to the container and field limits,
it rejects invalid UTF-8, truncated input, duplicate keys, duplicate/case-colliding IDs, unknown
properties, invalid Unicode normalization, broken ordering/references, checksum mismatch, and
any supplied path that is not exactly one regular bundle file. The viewer permits no decompression,
network request, external resource, script execution from the bundle, or HTML injection. It
renders at most 100,000 SVG/DOM nodes and at most 16 MiB of serialized SVG per selected frame;
exceeding either limit is a validation failure rather than partial render.

The public validator error vocabulary is `invalid_utf8`, `json_syntax`, `duplicate_key`,
`noncanonical_json`, `root_shape`, `unknown_field`, `unsupported_version`, `invalid_value`,
`limit_exceeded`, `payload_checksum_mismatch`, `bad_reference`, `bad_frame_order`,
`bad_timeline_order`, `privacy_violation`, `terminal_conflict`, and `not_supported`. Implementations
may retain diagnostic detail internally but expose one of these stable codes.

### Acceptance tests to add in later implementation packages

1. Equal projected inputs produce byte-identical canonical bundles, payloads, and state hashes.
2. Validator fixtures cover every closed variant, frame/timeline continuity, and the completed versus
   incomplete terminal boundary.
3. Altered payload bytes, state hash, content/map fingerprint, or reference are refused before
   rendering; the evidence states that checksum validation is not authenticity validation.
4. Public and seat exports prove prohibited raw/private identifiers, entropy, and other-audience
   facts are absent from the complete JSON and rendered DOM/SVG.
5. Limits, duplicate keys/IDs, unknown fields, noncanonical JSON, malformed UTF-8, and a supplied
   directory/non-regular path are refused before rendering.
6. Capture-to-frame replay fidelity is tested only in R01-005 after its named M03 replay closure;
   R01-001 and R01-002 make no aggregate replay or raw event/decision hash claim.

### Definition of done

The schema, role matrix, bounds, hash algorithm, full-frame policy, error vocabulary, and test
matrix are unambiguous; an independent Tier-C review accepts the hidden-information and artifact
boundaries. This documentation package does not claim that v1 is implemented.

## Follow-on packages

| ID | Package | Depends | Deliverable |
|---|---|---|---|
| R01-002 | Bundle validator/types | R01-001 review | Rust v1 types, canonical writer/validator, malformed-input tests. |
| R01-003 | Optional capture adapter | R01-002, M03 replay closure | Opt-in audited/scripted-game capture; disabled path is behavior/allocation-neutral. |
| R01-004 | Offline viewer | R01-002 | Self-contained HTML/SVG board, timeline, decision/event/narrative panels; no server. |
| R01-005 | Review qualification | R01-003, R01-004 | Replay/hash, redaction, corruption, and deterministic SVG visual-regression campaigns. |
| R01-006 | Tier-C review | R01-005 | Independent review of privacy, schema evolution, artifact limits, and viewer claims. |

Training export is a later optional adapter after R01-003; it is not a dependency of this track or
an M10 exit criterion. A live watch mode and any local server are explicitly deferred to a separate
proposal with loopback, lifecycle, and resource-limit design.
