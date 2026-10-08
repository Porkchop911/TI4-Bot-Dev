# Card-text features for unseen factions - design (2026-10-09)

Status: proposal, no code changed. Branch wp/base-factions. Needs operator decision (section 5) and, because it adds a projection family, an architecture review (projection.rs module header, lines 17-30).

## 0. Findings that shape the design

- Faction rows are nearly inert (zeroed-rows test: 91.62%/3.643 vs 91.58%/3.667). Transfer to the 27 new factions therefore rests entirely on the `ability_facts` family set (`crates/ti4-policy/src/features.rs:758-813`), which today names *identities*: `ability:<id>` (781), `faction-start-tech:<id>` (784), `faction-tech:<id>` (787), `faction-start-unit:<id>` (799), `faction-home:<id>` (806), `faction-commodities` (810).
- Those identity names are exactly what an unseen faction lacks: a new faction's `ability:foo` / `faction-tech:bar` are new strings, so they fall to the family OOV column (or, once appended, to freshly zero/OOV-initialised columns the policy has never been trained on). The decomposition is by identity, not by meaning. Card text is the only available source of meaning.
- Flagships, mechs, leaders, breakthroughs and promissory notes currently contribute nothing to the faction decomposition (no `faction-flagship`/`mech`/`leader`/`breakthrough` fact in 758-813). A new faction's most distinctive cards are invisible to the policy.
- Embeddings are NOT available: `POST http://127.0.0.1:8080/v1/embeddings` returned `{"error": {"message": "not found"}}`. Design below does not depend on it.
- Corpus is small: abilities.json 73 entries, units.json 136 (fields `ability`, `faction`), leaders.json 103 (`abilityText`, `type`, `faction`), technologies.json (`text`, `types`, `requirements`, `faction`), breakthroughs.json (`text`, `faction`), promissory_notes.json, action_cards.json, agendas.json. ~957 KB of content in total. Hand review of every tag assignment for the faction-bound cards (roughly 33 factions x ~12 cards = ~400 texts) is feasible; this is not a big-data problem.

## 1. Which cards, which decisions

| Card class | Source | Seen as | Phase |
|---|---|---|---|
| Faction abilities | abilities.json `permanentEffect`, `window`, `windowEffect` | acting seat, option-invariant | 1 |
| Faction techs, starting tech | technologies.json `text`, `types`, `requirements` | acting seat; also the *option* when the decision is "research X" | 1 / 2 |
| Flagship, mech (and unit upgrades with faction text) | units.json `ability`, `baseType`, stats | acting seat; also option when producing/moving that unit | 1 / 2 |
| Leaders (agent/commander/hero) | leaders.json `abilityText`, `abilityWindow`, `type` | acting seat; option when the decision is a leader use | 1 / 2 |
| Breakthrough | breakthroughs.json `text`, `synergy` | acting seat | 1 |
| Faction promissory note | promissory_notes.json | acting seat (as holder-origin) and option when played/offered | 2 |
| Action cards in hand | action_cards.json | option when the option plays that card | 2 |
| Agendas | agendas.json | option when voting on the agenda (outcome-effect tags) | 3, probably skip |

Where it appears, following the existing hidden-information discipline:
- **Phase 1: acting seat only, option-invariant**, emitted from `ability_facts` beside the existing six families. Same rule as line 754-756: only the acting seat's faction is read, absent is absent, resolved through `seen.content()` and `seen.sources()` (line 762-768), never `ContentStore::embedded()`. Opponents' cards get nothing (public-knowledge opponent modelling is a separate decision).
- **Phase 2: option-referencing**, in `action_facts_within` (projection.rs ~326-360 region, per option) - when an option's id/payload resolves to a card (tech to research, leader to use, PN to play, action card to play), emit the same tags under `card-tag-opt:`. Only for cards the acting seat may see (own hand, public faction cards). Held action cards in hand are hidden from opponents, so this goes through the same typed `held_secrets`-style boundary as `mlp_choice_features` (projection.rs ~1262), not through opponent views.
- Not all decisions need them. Per-option MLP means option-invariant seat facts only help via interaction in the trunk (projection.rs 271-275); the sharper signal is Phase 2, where the tag sits on the option that uses the card.

## 2. Representation options

| | (a) Fixed text embeddings | (b) Semantic tags (recommended) | (c) Keyword / n-gram bags |
|---|---|---|---|
| Source | Needs an embedding model. Local server has no /v1/embeddings. Would need an offline model (e.g. a sentence-transformer) run once, vectors frozen into a content artifact | Closed taxonomy of ~64-96 effect tags (`grants-move`, `plus1-combat`, `produces-units`, `gains-trade-goods`, `cancels-hit`, `steals-card`, `extra-activation`, `ignores-adjacency`, `token-economy`, `timing:combat-start`...). Labelled by rules + one-off LLM pass (local qwen), then human-reviewed, stored as a versioned JSON artifact keyed by card id | Lowercased stems from card text |
| Entering the sparse vocabulary | Dense vector must become named features: either K dims x Q quantised buckets (`card-embedding:d17:b3`, K=32,Q=8 = 256 names) which loses geometry, or K real-valued names `card-embedding:d17` with value = coordinate (K=32; honest, but dims are arbitrary so the MLP must learn what each means from ~10 seen factions - weak transfer) | Name = tag, value = capped count. Meaning is in the name, so the column trained on faction A's `plus1-combat` fires for faction B | Names = tokens, thousands of them |
| Bounded vocabulary | 32-256, fixed | Fixed by taxonomy (~100) x optional source (~6) = under 700 | Unbounded in principle; needs a pruned list; ngrams are exactly the `prompt-bigram` shape the architecture ruling suppressed (projection.rs:145) |
| Determinism / reproducibility | Frozen vectors are deterministic, but model and tokenizer version must be pinned and checksummed; any float drift in regeneration moves features | Fully deterministic at inference (table lookup); generation is one-off, output reviewed and checksummed. Rule extractor can be re-run as a drift check | Deterministic |
| Offline/online cost | Offline embed once; online lookup | Offline label once (minutes on local qwen + review); online lookup | Online tokenising unless cached |
| Unseen-faction benefit | Depends on whether the 10 trained factions span the embedding space; opaque; risks overfitting 32 arbitrary dims to few examples | Direct: an unseen faction's cards map to tags the shared network has already learned weights for. Failure mode is visible and debuggable (read the tags) | Weak: wording differs across cards with the same effect; sparse |
| Debuggability | Poor | Good, tags are auditable and diffable | Medium |

Why not (a) now: no endpoint, 10-ish training factions is too few to learn arbitrary-direction dims, and the vocabulary is name-based. Keep as a later add-on (`card-embedding` could ride the same artifact schema) if (b) shows gains and the operator wants more.

## 3. Recommended design

### 3.1 Artifact
`crates/ti4-content/content/card_tags.json` (new content file, versioned; add to `CHECKSUMS.sha256` and `manifest.json` the same way the other content files are). Shape:
`{"schema": 1, "taxonomy_version": 1, "tags": ["grants-move", ...], "cards": {"<content_type>:<id>": ["tag", ...]}}`. Tag count per card capped (e.g. 6). Lookup by (content type, id) so homebrew/unknown cards simply have no tags (absent, not zero).
Production: (1) a rules pass over keyword patterns for the obvious tags, (2) a one-off local-LLM pass proposing tags per card from a closed list (prompt pinned in the plans evidence), (3) human review of the diff, (4) commit with a checksum. No network and no LLM at inference or training time.

### 3.2 Feature family names
- `card-tag:<tag>` - Phase 1. Value = number of the acting seat's faction cards (abilities, faction techs incl. starting tech, flagship, mech, leaders, breakthrough, faction PN) carrying the tag, as `count_value(n)` (the helper already used at features.rs:801; bounded). Optionally split by source as `card-tag-src:<src>:<tag>` (src in {ability,tech,unit,leader,breakthrough,pn}, <= 6 x ~100 = 600 names) if the first pilot shows source matters; start without it.
- `card-tag-opt:<tag>` - Phase 2, per option, value 1.0.
Bounded by construction: the tag list is closed, so neither family is an unbounded cross (no card id, no option id in the name). The shape meets the predicate at projection.rs:19-27.

### 3.3 Where it lands in code
1. features.rs: append the `card-tag:` loop to `ability_facts` after line 806 (sorted `BTreeSet` of tags, same emission-order rule as 778-779). This changes `explicit_choice_features` output, so the schema-4 pins (M09-019b inventory and legacy-subvector pins, noted at projection.rs:9-15) must move. To keep them untouched instead, emit the family from `seat_state_facts` / a new `card_tag_facts` in projection.rs, next to `action_facts`, and merge in `project_vector` (projection.rs:1235). **Recommended: projection.rs**, so schema 4 stays byte-for-byte as it is and the change is a view, per the module's own rationale (projection.rs:9-15).
2. projection.rs `FAMILY_ROLES` (line 82, length 48 -> 49/50): add `("card-tag", Transferable)` (and `card-tag-opt`). The test `the_classification_covers_exactly_the_registry` will fail until the OOV registry matches; that is the intended gate.
3. vocabulary.rs: new OOV family entries => `OOV_REGISTRY_VERSION` 11 -> 12 with a new `OOV_FAMILIES_V12` and fingerprint (lines 45, 312-382, same pattern as V11). Alternative that avoids the OOV bump: no OOV row for these families, names always known because the taxonomy is closed. Operator/architecture decision, see section 5.
4. Vocabulary: append the full closed tag list for each family as named columns via `Vocabulary::append_reallocating` (vocabulary.rs:855). The whole taxonomy is appended up front, including tags no trained faction has, so a future faction never meets an unknown name.

### 3.4 Versioning
- Artifact: `schema` + `taxonomy_version` + sha256 recorded in the vocabulary manifest and in the checkpoint bundle, refusing to load on mismatch.
- Feature ABI: new projection version label recorded in the bundle next to the OOV registry version. Taxonomy changes are append-only (new tags only, never renaming/reusing), mirroring the vocabulary's append-only rule.

### 3.5 Checkpoint migration
Use the established tools: `migrate_bundle_append_names --grow --init-from-oov` with the new names. Two init options: (i) zero rows (inert until trained; strict proof of unchanged play), (ii) OOV copy. **Use zero**: the OOV column of a new family carries no meaning, and the gate below is simpler. Padding comes from the append tool.
Proof that old play is unchanged (before any training): with zero rows, the new inputs contribute `0 * x`, so logits must equal the old checkpoint's bit-for-bit (or within f32 noise) on the existing decision-boundary corpus; additionally greedy 600-seed eval of old vs migrated must be identical. Only then train.

## 4. Evaluation plan

Hypothesis: with `card-tag` features, held-out factions play better than with identity features alone.

Design (greedy 600-seed protocol, paired seeds, same machine/workload):
- Split the factions into TRAIN (the six already trained + some of the 27, say ~14 total) and HOLD-OUT (>=6 untouched factions, spanning mechanical variety).
- Arm C (control): current features, trained on TRAIN, faction rows included (<=50 PPO updates as pilots per standing rule).
- Arm T: same plus `card-tag`, migrated from the same starting checkpoint with zero rows, same seeds, same schedule.
- Metric: HOLD-OUT factions' greedy clearance and mean VP; also TRAIN factions to check for regression.
- Noise: table sigma ~0.8pp on clearance; per-faction up to 5.4pp. Pool hold-out factions (6 x 600 seeds) and report paired differences with CI; require a pooled gain of at least ~2pp clearance (about 2.5 sigma of the table), and no hold-out faction lost more than its own noise (~5pp). Per-faction wins are not evidence; pooled and paired only.
- Controls: (1) a shuffled-tags arm (tags permuted across cards) to prove that gain comes from meaning, not from extra parameters; (2) a zeroed-faction-rows check on Arm T to confirm identity is carried by features.
- Do not use in-training tables: judge by paired greedy eval (memory: in-training tables mask regressions). Report seeds, worker count (16 workers allowed), full logs.

## 5. Risks and open decisions (operator)

1. **Architecture review required.** A new projection family is a Tier-C change by the projection.rs rules; approve the family names and whether Phase 1 (seat-invariant) goes first.
2. **OOV bump or not** (V12 migration vs no OOV row for a closed taxonomy). The no-OOV route is smaller but needs the family-role/registry test updated by agreement.
3. **Label quality.** An LLM-labelled taxonomy can carry systematic errors; mitigation is human review of the faction-bound cards and the shuffled-tags control. Who reviews ~400 card texts?
4. **Tag granularity.** Too coarse and unseen factions look alike (the Keleres problem, features.rs:744-746); too fine and tags are seen once. Start ~80; revisit after the pilot.
5. **Signal may stay weak** in the option-invariant form (Phase 1 only reaches the trunk through seat-state interactions); Phase 2 may be where the benefit actually is.
6. **Could not verify:** no embedding endpoint; I did not inspect the full homebrew/franken content shape, the exact `action_facts_within` insertion line or the FAMILY_ROLES/OOV test expectations beyond the module headers, and I ran no tests (cargo is off limits). Line numbers are from today's tree and will drift (projection.rs and features.rs both carry unrelated uncommitted edits elsewhere in the tree).
