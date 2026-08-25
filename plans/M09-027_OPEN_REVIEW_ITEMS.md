# M09-027 open review items

## Independent Tier-C review of `ab9c896` (2026-08-26) — changes required

Reviewer: Codex frontier model, independent of the Claude Opus 5 implementation.

Reviewed `docs/MLP_PLAN.md` revision 5 §§4.1–4.2, the package diff, extractor and
value APIs, invariance tests, evidence, and the existing typed hidden-information
boundary in `SeatObservation`.

Independent gates:

```
cargo test -p ti4-policy --lib critic             5 passed, 0 failed
cargo test -p ti4-mlp --test critic_invariance    2 passed, 0 failed
cargo clippy -p ti4-policy --lib                  exit 0; one pre-existing engine warning
cargo clippy -p ti4-mlp --test critic_invariance exit 0; no warning in touched files
rustfmt --edition 2024 --check <touched files>    clean
git diff aed3304..ab9c896 --check                 clean
```

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-027-1 | **HIGH** | The critic's private input is enforced by caller convention, not the typed boundary. `critic_facts`/`critic_vector` accept a public `Observed`, an arbitrary `PlayerId`, and arbitrary caller-supplied `held_secrets`. This recreates the exact boundary that `SeatObservation` was introduced to remove: a caller can request any seat's public position and attach any seat's unredacted secret progress. The documentation's claim that the records are engine-bound is not expressed by the API. | Take `&SeatObservation` and derive both the acting player and held-secret progress from that capability. Do not accept either as caller arguments. Add an API regression proving an `Observed` value cannot produce a full critic and a runtime regression proving two bound seats receive only their own secret progress. |
| F-M09-027-2 | **HIGH** | `Actor::value` accepts the public, generic `SparseOption`. A caller can pass the sparse vector for a policy option—including option/legal-set-derived columns—directly into the value head. Thus the statement that the value function “has no way to see the legal set” is false at the actual inference API. The invariance test does not exercise a production boundary: it constructs each `Choice`, discards it with `let _ = choice`, and repeats the identical extractor call. | Introduce a critic-specific input type that cannot be constructed from option vectors through the public API, and make `Actor::value` accept only that type. Exercise the real extraction-to-value path while changing legal-set order and contents; add an API/compile-fail regression preventing `SparseOption` from being supplied as critic input. |
| F-M09-027-3 | **HIGH** | The submitted accepted vocabulary maps every `critic-state:*` fact to the single global OOV column. The evidence correctly measures that all semantic identities collapse, but then calls M09-027 implemented and moves the missing acceptance behavior to 027b. Under the package split rule, the original acceptance criterion must be preserved across the children; the parent cannot be complete while its value input is only a rank-1 sum over one shared row. | Keep M09-027 open through M09-027b. Implement the registry/projection/corpus migration, regenerate the accepted generation, and rerun the value/invariance tests against that accepted vocabulary with a non-vacuity assertion that distinct critic facts occupy distinct intended/reserved columns. |

The separate namespace, deterministic ordering, gated objective/ability groups, shared trunk/value
readout, and focused numerical checks are otherwise sound. No source correction was made by the
reviewer.

**Verdict: changes required.** M09-027 and M09-027b remain open; M09-028 remains blocked.
