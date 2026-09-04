# Evidence — OBS-003i prompt-free MLP projection (blocked checkpoint)

## Scope and provenance

- Branch: `wp/tier-c-review-remediation-obs008c2b-003e1`, after `cf807e8`.
- Normative source: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-003i.
- Permission: P1 only. No historical Python, network, external state, generated artifact, or
  destructive action.

## Result and focused evidence

The completed package keeps the schema-4 extractor unchanged and adds a prompt-free MLP source path:
it excludes `Choice.prompt` and `ChoiceOption.label`, retains stable option IDs and structured
facts, and retires `prompt-kind` from MLP admission. Focused regressions pass:

| command | result |
|---|---|
| `cargo test -p ti4-policy --lib mlp_vectors_ignore_prompt_and_label_rewording_but_keep_stable_ids --quiet` | 1 passed |
| `cargo test -p ti4-policy --lib the_schema_four_vector_is_untouched --quiet` | 1 passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 202 passed |
| `cargo test -p ti4-mlp --lib --quiet` | 91 passed |
| `cargo test -p ti4-mlp --lib a_prompt_bearing_schema_six_bundle_is_refused_before_model_construction --quiet` | 1 passed |

Schema-7 bundles record `projection_abi: 2`; the loader validates both before tensors or a model
are constructed. A schema-6 manifest is refused, so its historical prompt-bearing weights cannot
be silently ignored.

## Tier-C review and P1 resolution

Existing schema-6 MLP bundles may contain learned, nonzero `prompt-kind` rows because the current
projection admits that family. The draft removes the family before vocabulary lookup while leaving
the schema-6 manifest, slots layout, and loader acceptance unchanged. Such a bundle would still
load, but silently stop applying its learned prompt-kind weights. Retaining the reserved row only
preserves tensor shape, not input meaning.

The user selected the first design: a projection/extractor ABI in the bundle manifest, with
incompatible old bundles rejected. The implementation bumps the bundle schema to 7, requires ABI 2,
and has a regression that proves schema-6 rejection before model construction. Tier-C re-review
accepted the P1 resolution; its only P2 documentation note (stale schema-6 wording) was corrected.

Strict Clippy on the owned policy and MLP libraries is clean. The workspace all-target run remains
blocked by pre-existing out-of-scope MLP and training warnings (`positive_corpus.rs`, `ppo.rs`,
`stage1.rs`); none is in this package.

## Working tree and next action

- Next action: OBS-004 actor-owned inventory, unless a milestone integrator chooses to split its
  private-state surface further.
