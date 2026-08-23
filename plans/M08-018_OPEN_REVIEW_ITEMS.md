# M08-018 independent Tier-B review — post-M07 authored-bot revalidation

## Status

**Accept.** The `sar` diagnosis the spec asked me to check is correct, and its load-bearing
sub-claim verifies. Three findings, none blocking; U3 was surfaced to the operator and closed with no action.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of this. No involvement with `ti4-policy`; reviewed M08-017 and M08-020. |
| Base | `00d6562` (M08-020 accepted) |
| Diff under `crates/` | `bot.rs` +447/−0, single hunk at 1804 — inside `mod tests` (opens 1018). No production code. |
| Checks | policy **118**, engine **847** + 5 doctests, workspace **1,327**, Clippy zero in `ti4-policy` — all reproduced |

## The `sar` diagnosis — verified, and correct

The spec directs the reviewer to check this specifically, and to confirm that scoping the redaction
invariant to scoring-window records is right rather than a weakening. It is.

**The collision is real.** Both records exist, in different content files:

```
secret_objectives.json   alias sar   "Spark a Rebellion"
technologies.json        alias sar   "Self-Assembly Routines"
```

**Why the scope matters is subtler than "aliases collide", and the test gets it right.** The
invariant resolves an offered id with a *typed* lookup —
`content.get(ContentType::SecretObjectives, offered)` — so a `sar` offered by a **research** choice
still resolves as a secret objective, because it genuinely is one in that table. The typed lookup
does not save you here; only the `prompt == "score an objective"` filter does. An all-records check
would report a leak every time any bot was offered Self-Assembly Routines. The evidence's
instruction that this scope must never be "fixed" back to an all-records check is correct and worth
keeping.

**The load-bearing sub-claim verifies.** The narrowed check is only sound if no public objective
alias collides with a secret one — otherwise a legitimate public offer would resolve as a secret
and fail `allowed.contains()`. Checked directly: 40 public aliases, 40 secret, **intersection
empty**. The scoping is sound.

## Other verification

**Diff shape as declared.** One hunk, `@@ -1803,0 +1804,447 @@ mod tests {`, zero deletions, and
`mod tests` opens at 1018 — test module only, no production code, matching "no engine or policy
production defect found."

**Counts reproduce.** policy 118 (was 112 at base, +6), engine 847 + 5 doctests unchanged,
workspace 1,327. Clippy on `-p ti4-policy --all-targets` gives exactly two warnings, both in the
`ti4-engine` dependency (`choice.rs:568`, `game.rs:1260`) — **zero in `ti4-policy`**, as claimed.
`features.rs:690/752` fmt drift is pre-existing; `git status` confirms only `bot.rs` was touched.

**The campaign's non-vacuity guards are real.** `total_offers > 0` and `re_offers > 0` mean a fixture
change that stopped producing scoring windows, or stopped re-offering within an unlimited window,
fails loudly instead of leaving a green test that exercises nothing. Same for `paused == true` in
the retained-pause test. This package anticipated the vacuity problem that took three rounds to
surface in the M07 chain.

## Findings

### U1 — LOW · the explanation half of the redaction invariant is untested, and safe only by a property nothing pins

The spec's third required invariant reads: *"Observations **and explanations** expose no opponent
secret alias, exact private eligibility, hidden note relation, or private payment detail."* The
campaign checks **offered options** in decision records. Nothing in the six new tests inspects an
explanation, and nothing pre-existing does either — `grep` for explanation-plus-leak coverage across
`ti4-policy` returns nothing.

I chased whether that is actually exploitable, and it is not. Every component name in the policy
scoring layer is a **static string literal** — `Components::of("victory", …)` in `score_objective`,
`Components::of("hull", …)` at `bot.rs:359`, and so on across `bot.rs`/`scoring.rs`; no
`format!`-built or alias-derived name exists. An explanation therefore cannot carry an alias, and
the invariant holds.

But it holds by a property no test protects. The day someone names a component after the thing it
scores — `format!("objective:{alias}")` is the obvious and tempting shape — the invariant breaks
silently, and this package's campaign will not notice.

**Recommended action.** Record the structural argument in the evidence (it is stronger than what is
there now, which is silence), and consider one assertion that no `explain()` output over a campaign
game contains a secret alias the seat does not own. Cheap, and it converts an argument into a guard.

### U2 — LOW · the alias collision is a systemic corpus property, not an incident

The evidence frames the collision as *"alias spaces collide across content categories — `sar` is
both a secret and a warfare technology."* True, and understated. Surveying every content file:

```
aliases appearing in more than one content file:  27
  e.g. galvanize  [abilities.json, public_objectives.json]
       mirage     [explores.json, planets.json, tokens.json]
       mr         [planets.json, technologies.json]
       crisis     [action_cards.json, agendas.json]
```

`galvanize` is the one adjacent to this package's subject: a public objective sharing an alias with
an ability. Nothing breaks today — the scoring-window filter and the empty public∩secret
intersection both hold — but the next person writing any content-alias check will hit this, and
they should meet it as a documented corpus property rather than rediscover it through a false
positive the way this package did.

**Recommended action.** Record the count and a couple of examples in the evidence, or better, in
`KNOWN_DIFFERENCES.md` as a corpus note: *aliases are unique within a content category, not across
them.*

### U3 — INFORMATIONAL · the policy suite went from 0.12 s to 85.6 s (test-only; operator reviewed)

Measured here: `cargo test -p ti4-policy --lib` now takes **85.60 s**, dominated by the 60-game
campaign. At the M08-017 base the same command was **0.12 s** — roughly a 700× increase in the
inner-loop cost of every policy test run, for every developer and agent, on every change to the
crate.

The evidence names this and accepts it: *"accepted as the standing cost of the invariant this
package exists to keep."* The invariant is genuinely valuable and I would not drop it. But the
tradeoff was chosen by the implementing agent inside a revalidation package, and it changes how the
crate feels to work in from here on — including for the M09 learned-policy work that lives in the
same crate and will be iterated on hard.

**Operator-reviewed 2026-08-22: no action.** The cost does not reach training. The campaign is a
`#[cfg(test)]` test; the training and sim binaries never build the test harness, so rollout
throughput is unaffected. What it costs is `cargo test -p ti4-policy` and `cargo test --workspace` —
the development and review loop, not the programme's compute.

Nor is the duration pathological. It plays 60 complete six-player games at ~1.4 s each; `ti4-sim`'s
determinism test plays full three-player games at ~0.22 s each, so double the seats plus the longer
horizon accounts for it. The suite went from playing zero games to playing sixty.

One adjacent check, recorded as a negative result: `units::catalogue()` rebuilds a full `BTreeMap`
of unit records per call and has 112 call sites across engine and policy, several added by M08-020's
`ground_force_owners`. That *would* reach training. But the hot instance was already found and
fixed — `valuation.rs:80` documents it: "called once per option on every casualty and production
choice, and building the whole catalogue to answer it was the single hottest thing the bot did",
now a point lookup via `unit_type`. The remaining calls are per-decision rather than per-option.
**No profiling evidence of a live training cost; none claimed.**

## Disposition

**Accept.** U1 and U2 are evidence additions with an optional cheap guard; U3 is closed, no action.
None blocks the commit.

The requested verification stands: the `sar` diagnosis is correct, the scoping is sound rather than
convenient, and the reason it is sound — public and secret alias spaces do not intersect — is a fact
about the corpus that I checked rather than took on trust. This package also did something the M07
chain took four rounds to learn: it guarded its own campaign against vacuity up front.

## Resolution (implementer, 2026-08-22)

**U1 — resolved in-package.** Structural argument recorded in full in
`plans/evidence/M08-018.md` ("Review resolutions"). The cheap guard was added: new test
`scoring_explanations_name_no_secret_the_seat_does_not_own` (bot.rs test module, +52 lines)
asserts that every token in `Decision::explain()` resolving as a secret objective is one the seat
owns. Non-vacuity proven by temporary inversion of the assertion — the scan found `btv` in real
explain() output and failed loudly (pasted output in evidence); restored, green. Design note:
token-based matching avoids substring false positives; scoping to scoring decisions avoids the ML-3
cross-category collision (a research decision's `sar` is a technology).

**U2 — resolved as ML-3.** Added to `plans/KNOWN_DIFFERENCES.md`: aliases unique within a content
category, not across them. Counts re-scanned directly from the corpus for this package rather than
copied: 6 collisions on the singular `alias` field (all six named); 23 across all four identifier
fields excluding planets/systems; no intra-file duplicates; public∩secret objective aliases empty.
The reviewer's "27" was not reproducible under any definition tried — recorded as such, with its
four cited examples confirmed real and named in ML-3.

**U3 — closed by operator (no action), as recorded.**

Post-resolution verification: policy **119/0**; workspace **1328/0 identical ×2**; Clippy zero
warnings in ti4-policy (two first-draft `doc_markdown` warnings from the new doc comment fixed);
bot.rs rustfmt-clean under edition 2024; `git diff --check crates/` clean.

**Disposition stands: Accept.** All findings resolved; package ready to commit.
