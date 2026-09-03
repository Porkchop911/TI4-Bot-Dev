# OBS-002b — empirical observation-aliasing census

The empirical half of OBS-002b. The rule-dependency matrix (mapping every state read by legality or
application to state/context/option/hidden/stochastic/irrelevant) is still to do; this measures
where the current observation actually collapses distinct situations, so the matrix can be built
against evidence rather than intuition.

Tool: `cargo run -p ti4-training --example observation_alias --release -- --games 4 --rounds 4`.

## The test, and the version of it that is wrong

The obvious formulation — *identical observation with different legal actions proves the observation
is incomplete* — is **wrong for this architecture**, and the first version of this tool implemented
it and reported 453 defects.

This model scores each option from state features crossed with that option's own features. The
option set is therefore part of what it sees. Two decisions sharing a state context but offering
different options remain distinguishable through the options themselves. That is ordinary, not a
defect, and those 453 are reclassified accordingly.

What actually indicates aliasing is a collision where the state context **and** the option set are
both identical: only then has one input been given for two situations.

## Instrument check

Run before trusting any number from it, because the state key is a projection and a projection can
manufacture collisions:

```text
mean features on option 0 = 93, mean option-invariant = 35
decisions whose state key contains any prompt feature = 1753 of 3678
```

Only 35 of 93 features are option-invariant; the rest are crossed with option identity, and the
prompt reaches the invariant subset in under half of decisions. So this key is **under-inclusive**:
it is what the model sees *before* it reads an option, not everything it sees. Collisions here are
therefore candidates, not proofs.

## Result

```text
decisions recorded          3678
distinct observations       2459
observations seen more once  703
CANDIDATE ALIASES (same state context AND same option set)  250
same context, different options (expected, not a defect)    453
PROVEN (candidate whose seat facts differed between decisions)  164
```

## From candidate to proof

A candidate becomes a proof without any value signal: record the seat's own facts at each decision
and check whether they differ inside a group that already shares an observation and an option set.
If they do, the engine held a distinction the model did not receive.

**164 of the 250 are proven on that test.** The clearest:

```text
head tokens   7 decisions, 7 DISTINCT seat states, one input
  prompt: gain a command token into which pool
  options: fleet_tokens, strategic_tokens, tactic_tokens
  round=3, tactic=0, strategic=0, fleet=2
  round=3, tactic=1, strategic=0, fleet=2
  round=4, tactic=1, strategic=1, fleet=2

head other    9 decisions, 7 DISTINCT seat states, one input
  prompt: Warfare: redistribute your command tokens
  round=3, tactic=1, strategic=1, fleet=7
  round=3, tactic=2, strategic=1, fleet=6
  round=3, tactic=2, strategic=2, fleet=5
```

The decision is which pool to put a token in. Seven different pool positions, across two different
rounds, arrive as one input.

`seat_facts` computes `round`, `tactic_tokens`, `strategic_tokens` and `fleet_tokens`. They exist.
They do not survive into anything that separates these decisions, because the per-seat facts are
crossed with option identity and that crossing collapses here. The information is present in the
engine, computed by the feature layer, and still absent from the decision.

The round is worth stating separately: a four-round objective in which the model cannot tell round 3
from round 4 at a token decision is missing the one fact that decides how much future there is to
invest in.

| head | observations | repeated | candidate aliases |
|---|---:|---:|---:|
| tokens | 131 | 108 | **108** |
| landing | 53 | 50 | **48** |
| other | 131 | 77 | 20 |
| turn | 298 | 119 | 17 |
| secondary | 334 | 15 | 15 |
| cargo | 322 | 101 | 12 |
| ability | 88 | 26 | 11 |
| movement | 132 | 76 | 7 |
| combat | 45 | 10 | 6 |
| trade | 89 | 44 | 3 |
| production | 219 | 7 | 2 |
| strategy | 98 | 1 | 1 |
| activation | 181 | 46 | 0 |
| payment | 163 | 17 | 0 |
| scoring | 55 | 4 | 0 |
| transit | 2 | 2 | 0 |
| agenda, development, exploration | — | 0 | 0 |

**Tokens and landing are 156 of the 250.** Every repeated `tokens` decision is a candidate alias,
and 48 of 50 repeated `landing` decisions are.

The two most repeated single inputs:

```text
head other    9 decisions on one input   "Warfare: redistribute your command tokens"
head tokens   7 decisions on one input   "gain a command token into which pool"
                                          options: fleet_tokens, strategic_tokens, tactic_tokens
```

## Reading

The `tokens` head is asked "gain a command token into which pool" with three fixed options, and its
current pool counts do not reach the option-invariant state. Every such decision therefore looks
identical to the model regardless of how many tactic, fleet or strategy tokens it holds — which is
precisely the distinction the decision is about.

This joins up with a Stage-1 result that was recorded and never explained: tokens were found almost
irrelevant to opening clearance. A head that receives one input for every situation cannot learn a
policy over those situations, so "irrelevant" may have been a measurement of the observation rather
than of the game.

`activation`, `payment`, `scoring` and `transit` show zero candidates: their repeats all carried
different option sets, so the options separate them.

## Limits

- Candidates are not proven aliases. Two genuinely identical positions should collide. Separating a
  true alias from an honest repeat needs a value signal this diagnostic does not have, and that is
  the natural follow-up.
- The state key is under-inclusive, as measured above, so it can over-report.
- Four games is a lower bound on aliasing and never an upper one. Absence is not a clean bill.
- Viewless asks cannot be censused here at all: they have no seat observation to hash. OBS-002a
  names all fifteen as migration work, and their absence is a consequence of that.
