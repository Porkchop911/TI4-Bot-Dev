# M03 — Choice, timing, and replay

## Goal

Establish the load-bearing determinism boundary before porting the game rules.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M03-001 | Option and Choice | M02 | `engine/choice.py` | Stable IDs, kinds, labels, payloads, prompts, player; canonical golden tests. |
| M03-002 | Choice validation | 001 | `validate`, `IllegalChoice` | Only an offered option can execute; altered payload/duplicate ambiguity rejected. |
| M03-003 | Decider interface | 001,002 | `Decider`, `Table` | Bounded interface receives only choice and authorized view/context. |
| M03-004 | Simple deciders | 003 | first/decline/scripted/random | Behavior and exhaustion errors match oracle fixtures. |
| M03-005 | Decision log | 001–004 | `DecisionRecord`, `DecisionLog` | Versioned append-only records, explanations, canonical serialization, replay cursor errors. |
| M03-006 | Native RNG | M02 | dice/deck/random uses | Pinned algorithm and domain-separated streams; golden vectors and version rejection. |
| M03-007 | Legacy entropy translator | M00,005,006 | Python seeded traces | Convert legacy scenario to explicit entropy plus decisions; 100 fixtures reproduce. |
| M03-008 | Event model | M02 | `engine/timing.py` | Event ID/type/payload/cancellation/result with validated typed access. |
| M03-009 | Ability registration | 008 | `Ability`, resolver registry | Deterministic `(event, relation)` registration and frequency metadata. |
| M03-010 | Timing window resolver | 003,008,009 | `Resolver.emit` | WHEN/resolution/AFTER, priority order, pass/reoffer behavior, cancellation. |
| M03-011 | Nested emission | 010 | timing recursion | Depth-first nested events with bounded recursion/diagnostic failure; exact trace tests. |
| M03-012 | Frequency scopes | 009,010 | once-per-trigger/turn/round | Scope keys and lifecycle transitions match oracle. |
| M03-013 | Event/decision hashes | 005,008 | M00 canonicalizer | Versioned canonical hashes unaffected by allocation/map iteration. |
| M03-014 | Differential timing suite | 010–013 | `tests/test_choice.py`, `test_timing.py` | All applicable cases ported and oracle traces match. |
| M03-015 | Timing property/fuzz suite | 010–012 | — | Generated registries terminate, resolve at most allowed frequency, and never execute ineligible ability. |
| M03-016 | Frontier critical review | 001–015 | — | Review determinism, legality boundary, recursion, frequency, and legacy replay. |

## Exit gate

A scripted sequence replays to the same canonical state/event hash; choice and timing behavior matches
the oracle; randomized resolver tests terminate without illegal or repeated effects.

