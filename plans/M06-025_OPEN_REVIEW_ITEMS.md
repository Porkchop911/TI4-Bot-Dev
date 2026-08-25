# M06-025 independent Tier-C review — play-area note scoring

## Status

**Accept.** Three findings, none blocking. L1 is the one that matters for how this package's
result is read.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the code under review. Reviewed M06-021a…024; the independence limitation recorded in the M06-024 adjudication still applies. |
| Reviewed | uncommitted working tree over `bfcdb73`, including the M06-024 F1 fix |
| Diff | `promissory.rs`, `combat.rs`, `secrets.rs`, `transactions.rs`, `invasion.rs` |
| Checks | `cargo test --workspace` green, `ti4-engine` **839** (from 835); Clippy 5, all pre-existing |

## Verification

### `is_play_area` — correct

Faction records bind to their owner (`convoys:hacan` is play-area for Hacan's copy and no other);
generic `<color>_` records apply under every owner; unknown or malformed keys are not face-up.
The alias-then-`<color>_`-fallback lookup is the right shape and is what makes the generic notes
resolvable at all.

**Support for the Throne is unaffected, for a reason the evidence asserts but does not show.**
`alias_of("support:jolnar")` is `"support"`, which matches no corpus record, so `is_play_area`
returns false for a Support key. That is harmless because `receive` writes Support to
`support_holders` **only** — it never enters `promissory_notes` — and both predicates read
`support_holders` unconditionally. Verified in `promissory.rs::receive`.

**Own notes cannot be spuriously face-up.** `deal` inserts each seat's own notes directly into
the hand map rather than through `take`, so the face-up set is only ever populated by an actual
transfer. Confirmed at `promissory.rs::deal`.

### Both filters — correct, and no regression to M06-023's check 5

`combat.rs::note_holdings` and `secrets.rs::rival_note_issuers_count` now both require
`promissory_faceup.contains(note)`. The dedup keying survives intact — `player:{seat.id}` when
the issuer's faction is seated, `faction:{name}` otherwise, with Support inserting
`player:{owner}`. A seated issuer's Support plus their faction note still collapse to one bond,
which is the property M06-023 check 5 was about.

### K1 — real, and the fix is right

The prior code resolved a note's issuer by looking up the corpus record for `alias_of(note)` and
reading its `faction` field. Generic notes carry the `<color>_` prefix in their corpus id, so
`"an"` never resolved and **held Alliances never counted**. Reading the owner straight out of the
key is uniform across both record kinds and removes the lookup entirely.

This is the third instance of one defect class in this milestone (M06-023 H1, M06-024 F1, and now
K1): an identifier resolved through the wrong domain, staying green because the test built the
identifier by hand. Worth naming as a pattern in the M06 exit evidence.

### Scope extension — within the declared bound

`transactions.rs` is signature threading and one test call site. No logic. Matches the
declaration exactly.

## Findings

### L1 — MEDIUM (evidence, not defect) · eight of the nine faction play-area notes cannot fire with the current roster

The package's headline is replacing a hard-coded two-note `FACEUP` list with the corpus's eleven
`playArea` records. Measured over 150 holdout games, instrumented on `promissory_faceup`:

| alias | held (game-observations) | **face-up** |
|---|---:|---:|
| `an` (Alliance, generic) | 900 | **43** |
| `convoys` (hacan) | 150 | **42** |
| `cf`, `ps`, `ta`, `ce`, `favor`, `ms`, `ra`, `war_funding` | 150–900 each | 0 |
| the eight other faction play-area notes | 0 | **0** |

The only two notes that ever go face-up are `an` and `convoys` — **exactly the two the old
hard-coded list already had.** The reason is structural, not statistical: of the nine faction
play-area notes, only `convoys` belongs to a faction in D11's roster.

```
convoys        hacan        IN ROSTER
blood_pact     empyrean     not in the six
pop            mentak       not in the six
gift           naalu        not in the six
antivirus      nekro        not in the six
terraform      titans       not in the six     <- named by baf's own notes field
dark_pact      empyrean     not in the six
shareknowledge deepwrought  not in the six
sever          crimson      not in the six
```

**This is not a defect and the change is still right** — the hard-coded list was wrong on its
face, and the corpus-driven model is what makes the roster widening in D11 work without another
engine change. But the package's measured effect is necessarily zero for the eight, **by
construction rather than by rarity**, and that distinction should be recorded so nobody later
reads the unchanged numbers as evidence the model does not work. It is the inverse of J1: there
the question was whether a path was reachable; here reachability is provably blocked by the
roster and will unblock when the roster changes.

**Required action.** Record in the evidence that the eight are untestable under D11's roster,
and add a re-check to whichever package widens it.

### L2 — INFORMATIONAL · M06-023's measured gain was counting notes the card forbids

Strengthen Bonds across this milestone:

| state | rate |
|---|---|
| before M06-023 H1 | 49/56 = 88% |
| after H1 (issuer resolution fixed, no play-area filter) | 51/56 = **91%** |
| after M06-025 (play-area filter) | 49/56 = **88%** |

The three seats H1 appeared to gain were hand-held faction notes, which `sb` explicitly excludes.
The return to 88% is the filter working, not a regression. Worth a line in the M06-023 evidence so
the 91% figure is not cited later as a durable improvement.

Betray a Friend is 11/58 = 19% throughout, unchanged by H1, F1 or M06-025.

### L3 — INFORMATIONAL · `note_holdings` does not exclude a seat's own notes

Unlike `rival_note_issuers_count`, `note_holdings` has no own-faction guard, so a seat holding its
own face-up note would list itself as an issuer. Harmless today: `baf` tests
`notes[winner].contains(loser)` and a combat has `winner != loser`, and `deal` cannot produce a
face-up own note. Worth a one-line guard or a comment recording why none is needed, so the
asymmetry between the two functions is deliberate rather than incidental.

## Toolchain note

Raised during this review and resolved by operator direction: the resident model is
**Qwen 3.8 27B**, and Pi is **0.84.2**. Both are current — `AGENTS.md:80` and
`PI_WORK_PACKAGE_STANDARD.md:5` were stale, naming Qwen 3.6 35B and Pi 0.84.1. Both governing
documents have been updated. Evidence files naming the older model record what actually
reviewed those packages at the time and were deliberately left unchanged.

## Disposition

**Accept.** L1 should be recorded in the evidence before the package commits; L2 belongs in the
M06-023 evidence; L3 is at the author's discretion.

With M06-025 accepted, F2 clears and the only thing standing between here and the M06 exit gate
is M06-024's own adjudication, which J1 has now closed on real activation counts.
