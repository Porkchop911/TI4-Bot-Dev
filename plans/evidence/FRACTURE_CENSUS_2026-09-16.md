# Fracture census after the completion fix — 2026-09-16

Astra's review found that `audit_game_with_deciders` breaks its loop on an engine error and on its
step bound, then returns `Ok` with the partial state, so a game cut short is indistinguishable from
one that played the horizon and saw no Fracture. The census now derives completion from the final
state it already receives (finished, or reached round 1 + `rounds`), reports cut-short games
separately with the rounds they died in, and excludes them from every percentage. The shared audit
helper was **not** changed: it returns a bare five-tuple to a dozen callers.

`checkpoint-212544`, temperature 2.5, 4 rounds, seeds 910001000..+12 x 6 rotations = 72 games.

| | count | share |
|---|---|---|
| games played | 72 | |
| cut short by an error or the step cap | **0** | 0.0% |
| expedition slice claimed | 72 | 100% |
| a seat holds a breakthrough | 72 | 100% |
| Fracture in play at the end | 41 | 56.9% |

In the 41 games where it was in play:

| | count | share |
|---|---|---|
| activation choices | 2,083 | |
| listing a Fracture system | 1,223 | 58.7% |
| a Fracture system chosen | **27** | **2.2%** |

## What this settles, and what it does not

- **No contamination in this sample.** Zero games were cut short, so "the Fracture never came into
  play" is not masquerading failure here. The earlier worry about the audit boundary was real in
  principle but does not explain these numbers.
- **It is in play and it is offered.** 57% of games have it on the board by the end, and where it is
  in play it appears in 59% of activation choices. So neither "never in play" nor "in play, never
  offered" is the explanation.
- **The policy declines it: 2.2% of the time it is offered.** That is a valuation or exploration
  question, which is the third of the three causes the census was built to separate.

Two caveats that bound what can be concluded:

- **Diplomacy is off in this path.** The census uses the audit helper, which has no capability
  switch, so it measures a different game from the one we train.
- **"Offered" overstates opportunity.** `tactical::activatable` lists systems without the player's
  token; it does not require a usable movement route or a fleet that could get there. So 1,223
  "listings" are not 1,223 real chances to enter, and 2.2% is therefore a floor on how often the
  policy passed up something achievable.

## What would answer it

Astra's funnel, which needs route awareness the current census does not have: Fracture enters play →
a seat gets a later tactical opportunity → an eligible activation is offered → a usable route and
fleet exist → activation chosen → ships actually enter → invasion or control → resources or points
realised by the end of round 4. Plus a small panel of saved legal states with accessible Fracture
rewards and time left, compared against a forced legal entry followed by the same continuation
policy, with negative controls where entering is bad. A branch that pays off and is rejected
implicates valuation; one that cannot pay off by round 4 is correctly rejected under the stated
objective.
