# Sol: reachable research versus expansion diagnostic

Operator chose Sol and requested execution. This package follows
[MLP_APPLICATIONS](../../docs/strategy-wiki/MLP_APPLICATIONS.md), specifically SCPT328 C17,
with C01–C03/C06/C23/C29/C30 as conditional expansion/technology context. Source:
[maintained Sol guide](../../docs/strategy-wiki/summaries/sol-strategy-summary.md) and
[cited claims](../../docs/strategy-wiki/summaries/sol.json). These are draft human strategy,
not official rules or teacher labels. First Gravity Drive must be distinguished from extra
research; objectives and slice can justify technology. The engine supplies actual legal
choices and observable outcomes.

## Authorized scope and protocol

New `crates/ti4-mlp/examples/sol_strategy_probe.rs` and analyzer/evidence/wiki navigation;
no actor weights, vocabulary, legal options, engine mechanics or existing benchmarks changed.
Work in existing `wp/rule-bot`, starting HEAD65a1f0a0; preserve unrelated dirty files.
Worktree subfolders permitted by operator; never create folders outside this worktree.
CPU16/Cargo8, one Cargo coordinator, pinned existing CPU libtorch, scratch under existing
worktree target/strategy-probe-scratch. GPU unnecessary. New output files only, under existing
out/strategy-wiki/extractions. Offline/locked build; no external service or training launch.

Construct real six-player DEFAULT/FULL games through the existing rollout setup API, with unchanged
checkpoint16448 MLPs at temperature0.001, exact setup baselines, no artificial money/units,
and checksum/Train-role verified full_np8_12_train pool. This is diagnostic/student-state
collection, not a strength benchmark; validation/confirmation maps remain untouched.
Default four origin seeds × diplomacyOFF/ON, fixed six-core lineup. Record every baseline
Sol Technology-secondary yes/no opportunity and paid-research menu with a real decline.
Paid extra primary technology is included; its engine context currently says secondary too,
so record actual offered cost and gate kind rather than trusting that field for attribution.

Select at most one baseline opportunity per round (paid6 extra-primary menu first, else
Technology secondary, else paid menu) across rounds1..4. Selection depends on gate type,
never on branch outcomes. Rerun each from identical setup with reset bot streams; force
exactly one offered take/skip option, then let all bots continue. Call the MLP even at the
overridden choice to preserve its stream. Require the whole executed-choice prefix hash and
root offer/own-visible input fingerprint to match baseline before applying the intervention.
The forced branch matching baseline's natural action must reproduce baseline outcome/hash.

Capture actual Sol ordinary actor inputs at candidate gates; preserve proposed versus executed
answers, technology inventory/Gravity Drive, resource/token/transport facts and own/public
objective progress. Full-game driver handles payments, production, loading, landing, limits,
scoring and later rounds. Record Sol decision timeline and actual board/progress snapshots at
end Action/pre-Status, next-round start and final cutoff. Scored VP/objective IDs accompany
readiness so post-scoring readiness is not mistaken for pre-scoring opportunity.

Each run bounded at40000engine steps and120seconds between steps; existing within-step
no-progress guards remain. Preserve setup/guard/inference/time/step/target failures, never
score them as0 or wins. Round4 cutoff standings are not full-game wins. Multiple gates from
one seed and two diplomacy settings are dependent, not independent evidence units.

## Acceptance and review

Focused routing/control tests, affected MLP checks, successful real baseline and paired replay
with exact natural-branch control, explicit missing gates/failures, immutable checkpoint/pool
checksums, exclusive outputs and reproducible factual analysis. Independent reviewer
`/root/review_probe` supplies design/source/raw-result review required by AGENTS.md.
No good/bad labels, finetuning or gameplay-improvement claim until actual continuations support
the source-conditioned comparison. This first campaign investigates rather than assumes the
guide's recommendation wins in the MLP's visited states.

## Implementation checks and initial setup correction

Two routing/selection tests pass (`tests-05`, exit0). Earlier logs retained: compiler rejected
sharing an Actor reference across the Rayon install boundary and an iterator lifetime; moved
actor copies into per-worker ownership with a bounded global pool and simplified unit census.
The first routing fixture used nonexistent `7technology`; corpus inspection corrected it to
`pok7technology`. These were implementation/fixture failures, not strategy evidence.

Initial smoke `--seeds1` failed both setups before gameplay: known training pool includes
system113 (Beta Rift), unavailable with a PoK-only source filter. Raw `smoke-01.json` preserves
the two setup errors and zero branches. Corrected diagnostic scope to the project's existing
DEFAULT/FULL (includes Thunder's Edge), matching the retained checkpoint's ordinary engine
environment; no pool/corpus/engine change. Guide remains PoK-era human advice, so expansion
and card-era compatibility are explicit limits. Do not treat these runs as pure PoK validation.

Independent source/analyzer reviewer `/root/review_probe` approved the design after paired
setup-error metadata retention and failed-control summary handling were corrected. Subsequent
completed gameplay, replay validation and final review are recorded below.

## First completed research campaign and budget follow-up

Smoke02: one origin, diplomacyOFF/ON;2 baselines,8 branches,4 exact natural controls, no
errors. Four-origin campaign:8 baselines/1 retained agenda guard,14 paired branches complete,
7 exact natural controls. Expanded to16 origins because the first sample had only7 eligible
decisions and none in round1; this is additional training-origin case collection, not holdout
tuning or strength evaluation. No checkpoints or thresholds changed between campaigns.

Main `results-16maps-01.json`/`summary-16maps-01.json`:32 baselines,2 retained baseline guards
(610170004 diplomacyON and610170014 OFF, both501 AFTER AGENDA_REVEALED asks);64/64 paired
continuations completed,32 exact natural-action controls. Runner exit2 correctly signals the
incomplete baseline set; analyzer exit0 validates the retained report. No failed baseline
becomes0VP. Selected gates:3 round2,13 round3,16 round4, none round1;15 already ownedGD,
17 did not. Paid menu costs were6 (two),2 (one),1 (one), plus28 secondary gates. Discounts
mean cost alone cannot identify all primary extra-research cases. All41 baseline gates are
retained;32 are selected according to the predeclared per-round priority.

Descriptive cutoff VP differences (take minus skip):6 positive,6 negative,20 ties. OFF:
1 positive/4 negative/5 ties; ON:5/2/15. These eligibility-selected dependent observations
are not independent-map estimates or teacher labels. Example610170001 OFF paid ac2 round2:
take gains1 planet/2 infantry/0VP at end round2, but1 planet/-1VP at cutoff. More expansion
alone does not establish stronger play.

Independent `/root/review_probe` verified all32 keys,2 guards,64 prefix/root matches,32 full
natural outcome/timeline equalities,4-map subset equality, hashes,32 final metric differences
and166 matched round checkpoint differences, with no discrepancies.

All32 baselines reached end-Action round1; none owned Gravity Drive then. Sol drafted
Leadership in all32. At the first recorded origin, Sol spent both strategy tokens on other
secondaries while four resources remained. This motivates a separate source-conditioned
single-decision budget test, rather than assuming insufficient money or declaring Leadership
wrong. The existing research-only experiment had no eligible round1 interventions.

Added `--mode reserve-gd`: eligible only at an actual round1 non-Technology secondary that
costs a strategy token, while Sol lacksGD, has1..2 strategy tokens and at least4 spendable
resources. Take/skip exactly the first eligible gate; retain all other original policies and
the same root/prefix/natural-control checks. Following the secondary is `take`; declining is
`skip`, not automatically a superior label. Only one saved token is tested, not a hardcoded
round-long reserve policy. Four origins first; compare actual GD acquired, expansion and
VP through cutoff. Source hypothesis C17 with C19's conditional secondary budgeting.

Archived the reviewed research source as local `sol-strategy-20261016-research-source-v1.rs`
before extension. Buffered report serialization after observing a long unbuffered output
tail; no comparative performance claim. Three focused tests pass (`tests-07`), including
typed seat-view eligibility for round/token/GD guards. Build04 completed exit0 in1m13s.

## Completed round-one budget experiment

Command: same unchanged checkpoint, `--mode reserve-gd --seeds4 --workers16`, new
`sol-strategy-20261016-budget-01.json`; analysis via the same Python analyzer. Eight baseline
attempts reproduce the corresponding research baselines' outcomes and Sol timelines exactly.
One baseline guard610170004 ON retained. Seven selected gates,14 paired attempts,7/7 exact
natural controls,6 complete pairs. Alternate skip branch610170001 OFF hits a501-ask
AFTER AGENDA_REVEALED guard; retained separately, not assigned a score. Runner exit2,
analyzer exit0; no prefix/input/integrity failure.

All seven roots had2 strategy tokens and4 resources, lackedGD and charged a token. Four
were Politics (001/002 ×OFF/ON); three were TE Construction (003OFF/ON,004OFF). The
root resource-cost field is0, not4:4 is an eligibility reserve threshold, not a payment for
the secondary. Actual costs_token and card IDs are preserved in raw and final summary.

None of the14 continuations ownedGD at end-Action round1, including the two paths in the
pair whose later skip continuation failed. Among six pairs complete through cutoff, follow
has higherVP in3, skip in2, tie in1. These dependent selected cases **do not support a
universal skip-secondary label**. Skipping only this one early secondary did not suffice to
obtainGD; the test does not establish the success/failure of reserving resources/tokens over
the entire round. Technology's draft/play timing remains an open explanation; no claim that
it was absent from the table or unaffordable is established by this report.

Independent reviewer `/root/review_probe` separately verified8 baseline equalities,
14 paired attempts/one alternate guard,7 natural controls,6 complete pairs, exact root
conditions/card identities and the GD-negative round-one checkpoints. Source extension
review approved; no new labels or model update from these findings.

## Final artifact and validation status

Authoritative research raw: `results-16maps-01.json`; final enriched summary:
`summary-16maps-02.json`. Authoritative budget raw: `budget-01.json`; enriched summary:
`budget-summary-02.json`. All names have `sol-strategy-20261016-` prefix in the existing
extraction directory. Earlier raw/summaries/logs remain retained. Analyzer handles omitted
empty Choice.details (paid menus) after an enriched-summary KeyError; final analyses pass.
Earlier independently verified metric differences are unchanged by metadata enrichment.

Natural-controls are39/39 across the two main experiments. This does not turn the retained
guard failures into passing games: research96 attempts/2 failed baselines; budget22
attempts/one failed baseline plus one failed alternate. The shared failed baseline is not a
new independent state. All cases are Train origins; no validation/final maps or historical
Python repository used, no files overwritten or deleted, no engine/weights/vocabulary changes.

Exclusive-output CLI refused existing budget raw with exit2 and preserved its SHA256.
Oversized rounds4294967297 refused before setup/output with exit2. Rustfmt passed. MLP
library source is unchanged from its earlier109-test passing run; this package's final three
example tests passed and driven-game integration is covered by the campaigns. No full
workspace or engine release qualification is claimed. Source+raw independent review passed.

Final verification: archived research source matches its embedded raw sourceSHA; current
source matches budget raw; both summaries match rawSHA; all checkpoint files and training
pool retain their hashes. `sol-strategy-20261016-manifest-01.json` records raw, final summary,
source/archive/analyzer/current binary and final test/build log checksums. Independent final
review approved evidence/wiki prose and enriched metadata/deltas. Final rustfmt/whitespace
checks passed;77 local file links resolved across proposal/navigation/log/Sol evidence
(anchor semantics not checked). All task processes exited and compute is released.

Next implementation target: capture Technology availability/draft/play timing and compare
complete source-conditioned round-one acquisition plans, rather than train from this
insufficient one-skip hypothesis. Later research cases remain useful contrast diagnostics:
both expansion and actual scored objectives must inform any teacher. Scope remains Sol only;
other seats are unchanged environmental MLPs. No ongoing compute process is required.
