# What the TI4 podcasts teach: strategies, openings and bot-feasible heuristics

Research notes distilled from the two Twilight Imperium 4th edition strategy podcasts with the
largest back catalogues, and an assessment of which ideas the rule-based rule bot
(`crates/ti4-server/src/session/smart_bot.rs`) could adopt.

Everything below is a **paraphrased summary** with episode references (number, date, and the
timestamp of the passage where known, `h:mm`). No transcripts or audio are stored in the
repository. Timestamps come from our own speech-to-text pass and are approximate (±1 min).

Companion documents (other agents, different sources, no overlap intended):
`COMMUNITY_FACTION_STRATEGIES.md` (BGG/Reddit/wiki research) and `FACTION_PLANS.md`
(mid/late-game plans). This file is the podcast view: what top players *say they do and why*,
including where they disagree.

---

## 1. Sources and coverage (read this first)

### 1.1 Which podcasts

| Podcast | Hosts | Catalogue (RSS, 2026-10-12) | Strategy relevance |
|---|---|---|---|
| **Space Cats Peace Turtles** (SCPT) — `spacecatspeaceturtles.podbean.com` | Hunter Donaldson; Matt Martens (co-host until late 2025); since 2025 mostly "El Blasto" and "JJ" (JadiimJedi) as co-hosts | **505 episodes, ≈770 hours** (2017-09 … 2026-10) | The big one. Runs the SCPT online/in-person tournaments; faction "Super Guides" (PoK era, 2022-24), "Beginner's Guides" for Thunder's Edge factions (2025-26) and "Ultimate Guides" (2026), plus theory episodes (tempo, strategy phase, agenda phase, point blocking). A large share of the catalogue is TI strategy (judging by titles); the rest is news, tournaments, lore, RPG actual-play and other games (Root, Arcs, Dune, Spirit Island). |
| **Radio Mecatol** — `radiomecatol.podbean.com` | Kalle (described as a two-time TI champion) and Ola (game designer), Sweden; transcripts are not speaker-separated, so RM opinions are attributed to "a host" where it matters | 45 episodes, ≈22 hours (2025-01 … 2026-07) | Short (~30 min), dense, opinionated episodes on single topics (trade, fleet composition, secrets, last round, single factions). Useful as a second opinion that often disagrees with SCPT's "boat-float" table culture. |

Other TI podcasts exist (e.g. "Twilight Imperium Junkies", "Through the Wormhole") but are far
smaller; SCPT is the "hundreds of hours" show the task refers to.

### 1.2 How the text was obtained — and what was *not* possible

* **No published transcripts exist.** Podbean's episode pages report `transcriptUrl: null`;
  the SCPT website has no written guides (only tournament rules).
* **YouTube captions could not be fetched.** SCPT mirrors many episodes on YouTube, but every
  caption/metadata request from this machine was refused with YouTube's "Sign in to confirm
  you're not a bot" check (tried five player clients via yt-dlp and the transcript API). No
  cookies were used.
* **Show notes** (all 505 + 45 episodes) were read from the RSS feeds. They contain almost no
  strategy content (announcements, credits). **Chapter lists** (Podcasting-2.0 JSON) exist for
  58 recent SCPT episodes (2025-03 onwards) and were used to pick the strategy segments to
  transcribe.
* **Own transcription**: faster-whisper `base.en` (int8, CPU, 3 threads, `nice 19`, a
  watchdog for stalls; two episodes partly with `tiny.en`), decoding only the selected time
  ranges. Audio was deleted after each episode. Transcripts live only in
  `/root/podcast-scratch` (not committed).
* Speech-to-text has no speaker separation. Who said what (Hunter / Matt / Blasto / JJ) is
  inferred from conversational cues (hosts addressing each other by name, "my game as …") and
  should be read as approximate; game terms were sometimes misheard ("sticks" = Styx,
  "Mecatol" spelled many ways) and were normalised here.

### 1.3 Coverage actually achieved

| Episode | Date | Length | Transcribed range (min) | Notes |
|---|---|---|---|---|
| SCPT 226: Round One Negotiations | 23 Dec 2021 | 69 min | 0-69 (69 min) | read in full |
| SCPT 270: Super Naalu Collective Guide | 26 Oct 2022 | 106 min | 58-97 (39 min) | read 0:58-1:14 |
| SCPT 285: What is the Midgame? | 08 Feb 2023 | 94 min | 0-93 (94 min) | read 0:06-0:48 |
| SCPT 287: Super Naaz-Rokha Alliance Guide | 22 Feb 2023 | 128 min | 70-117 (47 min) | read 1:32-1:42 |
| SCPT 302: Super Vuil’raith Cabal Guide | 07 Jun 2023 | 141 min | 77-130 (53 min) | read 1:16-1:24 |
| SCPT 323: Super Nekro Virus Guide | 01 Nov 2023 | 133 min | 72-122 (50 min) | read 1:12-1:24 |
| SCPT 326: Super Ghosts of Creuss Guide | 22 Nov 2023 | 147 min | 80-134 (54 min) | read 1:20-1:30 |
| SCPT 328: Super Federation of Sol Guide | 06 Dec 2023 | 121 min | 66-111 (45 min) | read in full |
| SCPT 336: Super Clan of Saar Guide | 31 Jan 2024 | 111 min | 60-101 (41 min) | read in full |
| SCPT 340: Super Universities of Jol-Nar Strategy Guide | 29 Feb 2024 | 137 min | 75-125 (50 min) | read in full |
| SCPT 358: Super Yssaril Tribes Guide | 03 Jul 2024 | 108 min | 59-99 (40 min) | read in full |
| SCPT 360: Super Emirates of Hacan Guide | 17 Jul 2024 | 101 min | 55-93 (38 min) | read in full |
| SCPT 394: Super Guide to the Agenda Phase | 12 Mar 2025 | 136 min | 0-70 (70 min) | read 0:08-1:10 |
| SCPT 399: Super Guide to the Strategy Phase | 16 Apr 2025 | 161 min | 0-161 (161 min) | read in full |
| SCPT 430: Crimson Rebellion Beginner's Guide | 12 Nov 2025 | 150 min | 100-140 (40 min) | read in full |
| SCPT 434: Ral Nel Consortium Beginner's Guide | 10 Dec 2025 | 132 min | 88-117 (29 min) | read in full |
| SCPT 437: 2025 Faction Tier List | 31 Dec 2025 | 152 min | 5-143 (138 min) | read in full |
| SCPT 441: Last Bastion Beginner's Guide | 28 Jan 2026 | 134 min | 85-125 (40 min) | read in full |
| SCPT 445: Deepwrought Scholarate Beginner's Guide | 25 Feb 2026 | 126 min | 80-116 (36 min) | read in full |
| SCPT 449: The Firmament/Obsidian Beginner's Guide | 25 Mar 2026 | 126 min | 102-118 (16 min) | read in full |
| SCPT 454: The Ultimate Guide to THE ARBOREC | 29 Apr 2026 | 134 min | 81-109 (28 min) | read in full |
| SCPT 458: The Ultimate Guide to THE TITANS OF UL | 27 May 2026 | 194 min | 101-147 (46 min) | read in full |
| SCPT 462: The Ultimate Guide to Scoring and Tempo | 24 Jun 2026 | 118 min | 0-118 (118 min) | read in full |
| SCPT 463: The Ultimate Guide to THE BARONY OF LETNEV | 01 Jul 2026 | 174 min | 101-132 (31 min) | read in full |
| SCPT 467: The Ultimate Guide to THE MAHACT GENE-SORCERERS | 29 Jul 2026 | 149 min | 90-130 (40 min) | read in full |
| SCPT 472: The Ultimate Guide to THE COUNCIL KELERES | 02 Sep 2026 | 183 min | 114-144 (30 min) | read in full |
| SCPT 476: The Ultimate Guide to THE EMPYREAN | 30 Sep 2026 | 166 min | 6-15, 107-134 (36 min) | read in full |
| SCPT 477: The Ultimate Guide to Winslaying and Point-Blocking | 07 Oct 2026 | 87 min | 19-82 (63 min) | tiny.en model; read in full |
| Radio Mecatol: Fleet composition | 13 Feb 2026 | 33 min | full (33 min) | read in full |
| Radio Mecatol: Over-the-top vs Pop-off | 16 Jan 2026 | 26 min | full (26 min) | read in full |
| Radio Mecatol: Last round | 19 Dec 2025 | 29 min | full (29 min) | read in full |
| Radio Mecatol: Secret objectives | 12 Sep 2025 | 42 min | full (42 min) | read in full |
| Radio Mecatol: Trade | 15 Aug 2025 | 32 min | full (32 min) | read in full |
| Radio Mecatol: Hacan in Thunder's Edge | 13 Mar 2026 | 36 min | full (36 min) | read in full |
| Radio Mecatol: Sardakk revisited | 02 Jan 2026 | 28 min | full (28 min) | read in full |
| Radio Mecatol: The problem with Mentak | 03 Jul 2026 | 30 min | full (30 min) | read in full |
| Radio Mecatol: The problem with Jol-Nar | 14 Feb 2025 | 27 min | full (27 min) | read in full |
| Radio Mecatol: Mahact | 31 Jan 2025 | 33 min | full (33 min) | read in full |
| Radio Mecatol: Mechs | 17 Jan 2025 | 46 min | full (46 min) | read in full |
| Radio Mecatol: Factions we hate to face | 18 Jul 2025 | 34 min | full (34 min) | read in full |

Totals: **SCPT ≈ 25.7 h transcribed** (28 episodes; ≈ 21.5 h of it read closely, the rest
skimmed by keyword where marked "read …") = **≈ 3.3 % of SCPT's 770 h**; **Radio Mecatol
≈ 6.6 h** (12 of 45 episodes, ≈ 31 % of its 22 h). Grand total ≈ 32 h of audio, within the
≈ 40 h budget; wall clock ≈ 3.5 h on a shared machine (load 10-30), with a watchdog restarting
one stalled worker. Show notes of all 550 episodes and the 58 SCPT chapter lists were read
in addition. Not covered: the other ≈ 477 SCPT episodes and 33 Radio Mecatol episodes.

**Honest summary:** this is a *sample*, not "every episode": about 32 hours of audio were
transcribed (≈ 3 % of SCPT, ≈ 31 % of Radio Mecatol). The selection favours (a)
the newest material (Thunder's Edge era, 2025-26, which is the ruleset our engine implements),
(b) theory episodes that apply to every faction, and (c) the strategy segments (round-1 cards,
tech path, scoring) of faction guides. Everything else — hundreds of episodes of news,
tournament recaps, other games, lore, and most PoK-era faction guides (Arborec, Argent,
L1Z1X, Muaat, Mentak, Nomad, Sardakk, Winnu, Xxcha, Yin, Titans PoK guides etc.) — was **not**
transcribed; for those factions the per-faction section relies on the 2025 tier-list episode
(437), which discusses every faction for 3-6 minutes, and on Radio Mecatol.

### 1.4 Era caveat

Advice is tagged **[TE]** (Thunder's Edge, 2025-26) or **[PoK]** (Prophecy of Kings era,
before late 2025). The hosts repeatedly say their own older episodes "age like milk" (ep 360,
1:01). Where the two eras disagree, TE wins for our purposes. TE vocabulary used below:
*Fracture* (an off-board region opened during the game; its planets carry relics),
*Styx* (a Fracture planet whose victory point follows control — it can be taken away;
"sticks" in the transcripts), *breakthroughs* (per-faction unlockable cards with a tech-colour synergy),
*Entropic Scar* (anomaly that lets you buy a faction tech for a strategy token), *ingress*
tokens (entry points into the Fracture).

---

## 2. General principles

### 2.1 Scoring and tempo — "the only episode you need" (SCPT 462, 2026-06; 477, 2026-10)

* **Tempo beats points.** "Most points" is not "the lead". Judge each player by *scoring
  potential next round*: unscored publics they qualify for, secrets in hand (assume all are
  scoreable — "you have to assume secrets are scoreable until proven otherwise"), access to
  bonus points (custodians, Imperial on Mecatol Rex, Styx, supports), speaker position for
  Imperial. A player at 8 with all secrets scored and an undoable stage II is dead; a player at
  5 with Imperial and three secrets may win in one action phase (462 0:14, 1:00; 477 0:31).
* **Count to ten.** The canonical TE ten: 5 stage-I publics + 3 secrets + custodians + Styx;
  variants replace one bonus with Imperial's Mecatol point or a support for the throne. You
  always need at least one bonus point — five publics, three secrets and a support is the hard
  cap without one (462 1:41).
* **Hard per-round limits** shape everything: one public per status phase (more only via
  Imperial/Overrule/Winnu hero), one status-phase secret per round, any number of action-phase
  secrets, one tech per round from the card (so faction guides plan ~4-5 techs), structures
  only via Construction (462 0:29, 1:02).
* **Score every round.** Failing to score in a round is a failure (Radio Mecatol, "Over the
  top vs pop-off"). Getting ahead of tempo is safe: if knocked back you are merely *on* tempo
  (462 1:31). The opposite school (stay hidden, big last round) exists but both podcasts lean
  to early scoring.
* **Score the insecure objectives first** — control/"hold planets" objectives that can be
  taken from you — and keep secure ones (tech, trade goods already banked) for later; this also
  defeats late point-blocking (RM "Over the top"; 477 1:19).
* **Custodians are the best point in the game**: a bonus point that costs no secret draw
  (Imperial's Mecatol point replaces the secret draw). TE PSA: prioritise custodians in round 1
  more often (Diplomacy is now picked so often in R1 that more factions can reach 6 influence)
  and **take Imperial in round 2** — "if Imperial goes untaken in round 2, somebody made a
  mistake" (462 1:27; 463 1:46; 458 1:50).
* **Custodians vs scoring in R1.** If you can only do one: usually custodians (publics can be
  caught up with Imperial). Exception: take Construction to set up a double score next round
  when structure objectives are out (462 1:21). Titans/JJ: if the choice is "score R1" vs
  "custodians first action R2", score R1 and plan Imperial R2 (458 1:50).
* **Speaker tempo** mattered more in PoK (Politics R3→R4 to be speaker for the R5 Imperial);
  in TE the game usually ends in round 4 status or round 5 action, so just make sure you get
  Imperial once, early (462 0:52, 0:56).

### 2.2 Round-1 strategy card values (synthesised from the TE faction guides)

How the hosts rank round-1 picks for the factions covered in TE guides (✔ = favoured,
~ = situational, ✗ = avoid). The pattern matters more than any single row:

| Faction (episode) | Leadership | Diplomacy | Politics | Construction | Trade | Warfare | Technology | Imperial |
|---|---|---|---|---|---|---|---|---|
| Arborec (454) | ✗ | ✔ | ✔✔ | ~ | ✔ | ~ | ✗ | ✗ |
| Barony (463) | ~ | ~ | ✔✔ (Hunter) | ✔ (JJ) | ✔✔ (JJ) | ~ | ~ (single tech only) | ✗ |
| Titans (458) | ✔ | ✗ | ✔✔ if also scoring | ✗ | ✔ | ✔ | ~ | rare |
| Keleres (472) | ~ | ✗✗ | ✔ | ✔✔ | ✔ | must ensure a home build | ✔✔ (JJ) | ✗ |
| Empyrean (476) | ~ | ✗ | ✔ | ✔ | ✔✔ | ✗ | ✔ | ✗ |
| Mahact (467) | ~ | ✗ | ~ | ✔ (Hunter) | ✔✔ | ~ | ~ | ✗ |
| Deepwrought (445) | ~ | ✔ late pick | ✔✔ | ~ | ✔✔ | ✗ | ~ ("Tech ≈ Trade") | ✗ |
| Firmament (449) | ~ | ✗ | ✗✗ | ~ | ✔✔ | ✗ | ✔ (late initiative helps) | ✗ |
| Ral Nel (434) | — | ✗ | ✗ | ✔ | ✔ | after R1 | ✔ | ✗ |
| Last Bastion (441) | — | — | ✗ | ✔ | ✔✔ | ~ | ~ | ✗ |
| Crimson (430) | — | — | — | — | — | ~ | ✔ (Sling Relay) | R2-R3 ✔✔ |

General rules that fall out of this and of the strategy-phase super guide (399, 2025-04):

* **Trade** is "the best card in the abstract" and the default first pick when available
  (both podcasts). Money is flexible (resources *and* influence, storable).
* **Politics** is the custodians card: R1 Politics → speaker → Leadership first action R2 →
  Mecatol. It is also the safest way to unlock a breakthrough (discard action cards) for
  factions whose breakthrough unlock is a "discard" condition. Play it *immediately* if a
  faction that peeks at action cards (Ral Nel, Yssaril, Empyrean DT) could otherwise strand
  you (454 1:25; 476 2:00).
* **Diplomacy** rises in R1 mainly because taking it *denies Warfare*: "if you don't need
  Warfare, don't take it" — Warfare's secondary is what poor-start factions (Jol-Nar, Nekro,
  Naalu, Arborec) need; make them take it themselves (399 0:57, 1:41). If a spend objective is
  out and Diplomacy is your only way to score R1, always take it (399 0:54).
* **Warfare** in TE usually resolves on the first action of the round, which ruins the build
  of whoever holds Technology (they must follow before having moved). Hence TE Technology is
  weaker in R1 for most factions, unless the faction doesn't need a build (Keleres, Crimson)
  (430 1:40; 458 2:10; 463 1:52).
* **Leadership** in R1 is weakest early (tokens are the least valuable currency early; plastic
  and trade goods matter more) but is the only way to follow >2 secondaries. In R2 it is
  mandatory if you are taking custodians first action (399 0:23-0:30).
* **Construction** rises when structure objectives are revealed, or for a forward space dock
  (Keleres, Empyrean, Ral Nel), and it doubles as a Warfare denial that still gives a build
  (399 1:19; 434 1:30).
* **Imperial**: never R1 except emergency (someone else discarded secrets for breakthroughs —
  then it hands *them* their secret back). From R2 on, "Imperial should be your first
  consideration every strategy phase" (462 1:30).
* **Double-teching in R1 is out of fashion** in TE: expeditions and breakthroughs compete for
  the same money (476 2:08; 463 1:52).

### 2.3 Strategy cards through the game (SCPT 399, 2025-04; Matt + Hunter)

* **Leadership** — best raw economics (3 tokens ≈ 9 "bucks"; spend influence in clean groups
  of 3). Mid-game *Leadership stall*: a rich, stall-heavy player delays playing it until others
  have passed, so they cannot buy tokens this round — powerful but invites retaliation next
  round (they still buy the tokens). Best version: force a rival onto the objective they did
  not want to rely on. Counter: if being stalled, pass early. Round 5: Leadership is second in
  the "win-slay carousel" after Imperial (0:22-0:53).
* **Diplomacy** — late game possibly the best card: lock your home system so only a Warfare
  holder can get in; third in the carousel (1:01).
* **Politics** — order: choose speaker, then draw 2 action cards, then look at 2 agendas.
  Common R1 deal: sell speaker to the player one seat behind you for cash. Hunter: never give
  speaker to *yourself* in R2; policy "don't let the speaker token pass behind me" (stay a top-3
  pick). Mid-game: nobody should pay for speaker — the Politics player assigns it for their own
  benefit (predict who will take Imperial next round) (1:05-1:18).
* **Construction** — weakest card most of the game ("dead card") except structure objectives,
  Cabal/Sardakk-type needs, or a key forward dock (1:19).
* **Trade** — automatic early pick; mid-game solid economy; dies late. Keep a strategy token to
  replenish via the secondary when the Trade holder tries X-2 or refuses you (1:28, 0:51).
* **Warfare** — round-1 key for some custodians lines (Hacan, Jol-Nar); mid-game often
  unpicked; last-round only answer to a Diplomacy lock (1:38).
* **Technology** — tech tempo is irreplaceable; *stall* Technology (play it only when you
  need it). Coordinate R1 timing among Technology, Diplomacy and Warfare holders
  (tech→diplo→warfare or warfare→diplo→tech). Round 5: avoid picking Technology — it hands the
  table the third unit upgrade, Fleet Logistics or Light/Wave that lets them stop you or win
  (1:50-1:58).
* **Imperial** — round 5: take it only if you can win *in the action phase* (public +
  Mecatol point or a fresh action-phase secret); for a status-phase win it is the worst card
  (scores last). Mid-game Imperial (R2-R3) is "the platonic ideal": a bonus point and a slight
  lead without becoming the obvious target. If Imperial was skipped in R2, Matt suggests
  taking it in R3 for public + *secret draw* rather than the Mecatol point, to keep secret
  tempo (2:00-2:16).
* Radio Mecatol "Last round": with a status-phase win plan take the **lowest initiative** card
  available (otherwise you must stop everyone who scores before you). Initiative order decides
  roughly half of all last rounds (RM "Factions we hate to face").

### 2.4 Secret objectives

* Keep **action-phase secrets** (Hunter: ~99 %): they can be scored any time, several per
  round, and they hide your threat. Blasto: keep a status-phase secret instead if it is
  certainly scoreable by round-2 status — secret tempo (score in R2, R3, R4 status) matters
  (462 1:12; RM "Secret objectives" agrees strongly on action-phase secrets).
* Easy/controllable secrets are the ones you can force regardless of opponents (5 dreadnoughts —
  also hidden because "everyone builds dreads"; discard 5 action cards; 4 PDS if drawn early;
  ships adjacent to 3 anomalies; another player's promissory note). Hard: those needing
  opponents' choices (destroy a flagship/war sun; turn their fleets to dust), become a martyr,
  occupy the seat (3 ships on Rex at status), four planets of one trait (RM).
* **Never reveal secrets**, act the same whether or not you can score, hold action-phase
  secrets until the last moment (462 0:06-0:12).
* Imperial's Mecatol point costs a secret draw; draws are scarce (~3 per game) (RM; 399).

### 2.5 Command tokens

* Tokens are the least valuable resource early, the most valuable late (399 0:27).
* Keep about two in the strategy pool so a Leadership stall can't lock you out of Imperial and
  Technology secondaries (Matt); budget a strategy token for Trade's secondary if the table
  plays hardball on refreshes (Hunter) (399 0:48).
* Mahact/Saar-style advice: don't spend every token every round; save before the final round
  (RM "Mahact"; 467 1:38). Make each activation do several things.
* Hacan TE: Auto-Factories can turn cheap builds into extra fleet supply (RM "Hacan in TE").

### 2.6 Trade, deals and table politics — the big disagreement

* **SCPT ("boat float")**: a rising tide lifts all boats. Refresh and "wash" commodities for
  X-1, pay debts, give the poorest factions money — bringing Sardakk from 8 to 13 matters more
  than taking Jol-Nar from 30 to 31 (money above ~20 is useless). Starving the table only
  widens the rich factions' lead. "Ethical Hacan": make deals cheap and frictionless and the
  table will help you later (399 1:30-1:34; 360 1:02). But no ride-or-die alliances; help is
  temporary and returns to neutral (477 0:53; 340 1:42 "table boat float").
* **Radio Mecatol ("stingy")**: the X-1 wash meta is a gift to opponents. Make others pay a
  strategy token to replenish; above all be stingy in **round 1**, where an opponent's extra
  trade goods are their second carrier or a tech; be more generous later. Refresh only players
  who trade with you, loyal allies, or a weak player you want to prop up against a leader
  (RM "Trade", 2025-08).
* Both agree: trade goods are the best resource; watch for hoarders who can bank resource/TG
  objectives (unstoppable); Jol-Nar's research agreement is worth ~6-8/round — sell for ~3 per
  use and spread tech to weaker factions, not the richest (340 1:40).
* Supports for the throne: both SCPT hosts prefer games without support swaps and only engage
  once others start; then swap early. "Manipulator" path: extort a support when behind
  (462 0:33, 1:42).

### 2.7 Point blocking and winslaying (SCPT 477, 2026-10)

* TE shortened games (end R4 status / R5 action) so nobody has the plastic for big late
  winslays; **block points earlier and cheaply** instead: ideally **one command token + one
  capital ship** (e.g. carrier + infantry retaking Mecatol to deny Imperial's Rex point)
  (0:23, 0:47).
* Target players **ahead of you**; knock them from first to fourth, not out (eliminated
  players retaliate; knocked-back players keep playing for points) (0:32).
* Priority target: whoever took custodians **and** scored in round 1 (can reach 10 without a
  support if they also get Imperial). Custodians without a score is merely "on tempo" (0:55).
* What to block: **control objectives** (take their 4th industrial / edge / empty system —
  best when the planet helps you too); **spend objectives** (wait until they exhausted planets,
  then take one); **trade-good objectives** (propose a round-long trade embargo on the leader);
  **tech objectives** can't really be blocked, but declining to pick Technology/Construction
  can deny a leader's Imperial double-score (0:56-1:05).
* Rich utility factions (Creuss, Jol-Nar, Deepwrought, Nomad) should *enable* combat factions
  (Sardakk, Yin, L1Z1X) for free — wormholes, agents, money — since the bruiser pays the token
  and risks the ships (0:45-0:52). Keep helping a bruiser whose attempt failed.
* Combat margin for small fights: bring more HP *and* more dice than the defender (carrier +
  fighter vs carrier is risky; add a destroyer) (1:14).
* Mutually-assured-destruction threats are overused: take your licks and keep playing for
  points; it wins more games over a season (0:33).
* Radio Mecatol adds: people (correctly) attack the points leader more than the board-position
  leader; and a counter-Jol-Nar note — attack snowballing factions early or refuse to pick the
  Technology card they need (RM "Jol-Nar").

### 2.8 Defending and attacking

* Defend your home **when ahead**; when you move the starting carrier out, leave a destroyer
  or dock + fighter so the home can't be taken by a lone ship ("keep it gummed"). Early
  attacks only if decisive (eliminate or cripple) (462 0:24).
* TE: Styx plus three secrets = four points not tied to your home; defend your forward fleet
  more and your home less than in PoK (477 0:25).
* Factions with a home that can't be taken (Crimson, Deepwrought coexist) are top tier partly
  for that reason (437).
* Block your equidistant systems against Barony (they can take a neighbour's home R1) (437 0:16).
* Only infantry and fighters are cheap defensive HP (360 1:17).

### 2.9 Fleet composition and combat (RM "Fleet composition", 2026-02; SCPT faction guides)

* Hits are rare; one or two hits decide most battles → HP and sustain damage matter a lot.
* Production capacity (not money) is usually the binding limit; dreadnoughts are efficient per
  production slot, carriers + fighters per resource. Build **every round** — R2-R3 fleets
  fight the decisive battles; a mega-fleet built in the last round is a classic waste.
* Destroyer > cruiser as a cheap fast ship (empty-system objectives, anti-fighter barrage).
  Unit-upgrade order in a vacuum: Carrier II > Dreadnought II > Destroyer II; Fighter II only
  with hit bonuses (Sardakk, Naalu). Assault Cannon (pre-combat auto-hit) is often worth more
  than a unit upgrade. Counter-build: opponent builds fighters → destroyers; big ships →
  many cheap dice.
* TE tech consensus across the guides: **Gravity Drive**, **Carrier II**, **X-89** (Styx and
  Mecatol are decided on the ground — "lost Styx with 20 infantry to Cabal's X-89", 454 1:44)
  and **Fleet Logistics** (two actions in one turn wins action-phase rounds) are core for most
  factions; **Light/Wave** to leapfrog Fracture blockers; Dreadnought II is "dead" for several
  factions in TE (463 2:00); Destroyer II rose (Fracture fights vs fighter screens, 463 2:00).
* Don't use a battle calculator; learn cowboy averages (340 1:16).

### 2.10 Agenda phase (SCPT 394, 2025-03; PoK)

* If you are **ahead**, keep "nothing burger" agendas on top when you hold Politics; if
  **behind**, bury them and keep disruptive ones (0:14).
* Always keep **one vote** available (Mutiny) (0:24).
* Riders change the vote: Imperial Rider (VP) and Politics Rider (3 cards + speaker) are the
  big ones, Leadership Rider (3 tokens) is underrated; a juicy rider on one outcome pushes the
  table to the other; Politics Rider recruits every player whose pick order improves (0:46).
* Hold Assassinate Representative until riders are down; play Insider Information after the
  first agenda, not the second (0:43, 1:06).

### 2.11 Drafting and slices (several guides)

* Draft priority (Blasto, 476 1:50): 1. qualification for planet-based control objectives
  (four of a trait, tech skips, systems outside home), 2. production (a 3+ resource planet for
  a forward dock, ideally a 2-planet system), 3. Fracture access, 4. spending power. Poor
  factions move spending to the top.
* Read the draft: who can take custodians R1 (Saar nearly always), which factions hard-counter
  each other (Crimson vs Barony sustain; Benediction vs fleets) (462 1:06).
* Entropic Scar adjacency is worth a speaker spot for Mahact, Mentak, Last Bastion, Keleres
  (467 1:30; RM "Mentak").

### 2.12 Round-one negotiation and the "mid game" (SCPT 226, 2021-12; 285, 2023-02; PoK)

* R1 priorities in order: score R1 or set up custodians → fill out the slice (≈ 2 systems) →
  trade with neighbours → tech and plastic (226 0:06).
* Round-one deals are mostly about **timing**: everyone needs money before Technology pops and
  a build off Warfare; the Diplomacy holder is the natural coordinator and should agree the
  order with the Technology and Warfare holders *in the strategy phase* before committing
  (226 0:07-0:12). Hunter's ideal R1 strategy phase: first pick Trade, second Politics, third
  Diplomacy (if it looks like nobody later will take it) (285 0:22).
* If only one of two neighbours can score a shared R1 objective (e.g. outer-rim systems), the
  scorer owes the other something — an R1 point is tempo (226 0:14).
* X-1 is the community price of saving someone a command token; X-2 for rich factions
  (Jol-Nar, Hacan) is accepted; debt tokens for non-neighbours should be paid fast or upgraded
  into promissory notes; money is worth most in R1 (226 0:22-0:33).
* **Custodians is a sellable asset**: someone will take it first action R2; watch who has
  Gravity Drive or Warfare R1 plus six influence, and who can enable them (Mahact/Yssaril
  agents, Naalu selling its zero token, Cabal promissory) — help a likely taker for a price
  rather than shrugging (226 0:42-0:52).
* The mid game is about **secrets**: publics give 2-3 options per round, but secrets arrive
  slowly and status-phase ones need rounds of set-up (nine ground forces, four PDS, tech) — use
  mid-game actions to prepare them or to cycle unscoreable ones (285 0:36). Score tech
  objectives last (auto-score), spend objectives later (spend money on plastic now), hard
  control objectives early ("more planets than two neighbours" only gets harder) (285 0:40).
* Each player's late game starts when they can name their path to ten (often when the first
  stage II flips); players behind on tempo must drag the leaders' win back to a round where
  everyone is in it (285 0:26).

### 2.13 Common mistakes the hosts call out

1. Judging the leader by points instead of tempo (462, 477).
2. Leaving Imperial untaken in round 2; not trying for custodians in round 1 when Diplomacy is
   coming (462 1:27).
3. Waiting for round 5 to deal with the leader (too late in TE) and over-investing in
   winslays instead of cheap point blocks (477).
4. Mutually-assured-destruction revenge after being blocked (477 0:33).
5. Spreading thin/over-exploring early (Empyrean "Dark Energy Tap trap"; Saar "gallivanting";
   Cabal overextending into the Fracture without securing its own slice) (476, 336, 437 1:51).
6. Building the late mega-fleet that never moves (RM).
7. Choosing agendas or secrets by vibe (394 0:14).
8. Picking Technology in the last round and handing everyone their last upgrade (399 1:58).
9. Discarding a good secret for a breakthrough without a plan to replace it (463 1:56).
10. Selling a strong promissory/commander to someone who *asks* for it late (they hold the
    winning card) (437 1:08; 441 1:52).

### 2.14 Where the hosts disagree

| Topic | View A | View B |
|---|---|---|
| Trade refresh/wash | SCPT: boat-float, X-1 for everyone (399, 360) | Radio Mecatol: stingy, especially R1 (RM "Trade") |
| Mahact | SCPT TE tier list: S, #1 (437) | Radio Mecatol PoK: rated low, "best last turn, nothing early" (RM "Mahact") |
| Keep action secret vs scoreable status secret | Hunter: action ~99 % | Blasto: status if scoreable by R2 (462 1:12) |
| Best faction TE | Hunter: Deepwrought | Blasto: Saar (477 1:12) |
| Barony TE | Hunter: low A | others: C (437 0:12, settled B) |
| Warfare primary for Mahact | Blasto likes (extra system) | Hunter dislikes (spreads thin) (467 1:50) |
| Light/Wave vs Fleet Logistics (Empyrean) | Hunter: Light/Wave | Blasto: Fleet Logistics (476 2:10) |
| Over-the-top vs pop-off | one RM host: score every point early, eat the heat | the other: sometimes hold back to avoid heat (both end up "take points when offered") (RM) |
| Hacan docks | one RM host: triple dock at home | the host who played TE Hacan: spread three docks for Auto-Factories (RM "Hacan") |

---

## 3. Per-faction notes

Tier letters are the SCPT panel's **2025 TE tier list** (ep 437, 2025-12-31; Hunter, Blasto,
JJ) unless stated: S Mahact, Crimson, Sol, Saar, Deepwrought, Jol-Nar · A Yssaril, Titans,
Cabal, Hacan, Naaz-Rokha, Ghosts · B Naalu, Nekro, Empyrean, Keleres, Barony, Nomad · C Yin,
Ral Nel, L1Z1X, Arborec, Last Bastion · D Winnu, Muaat, Sardakk · F Argent, Mentak, Xxcha,
Firmament/Obsidian. In June 2026 (462 1:34) they added that Empyrean and Deepwrought were
underrated and Crimson slightly overrated. Faction-specific opening tables are in §2.2.

### Arborec — C [TE]
* **Plan:** everything hinges on the breakthrough (Psychospore). R1 **Politics** is the safest
  unlock (discard action cards), then **Diplomacy** (ready a tech-skip planet), **Trade**
  (3 TG straight into it; but Arborec has no trade ship), Construction as a fallback that can
  snipe a Diplo/Politics secondary. **Leadership is the worst R1 pick** (no guaranteed unlock,
  no catch-up); Technology almost never R1 (454 1:20-1:36).
* Then Imperial + sit on Mecatol Rex with a big infantry stack in R2; Rex's legendary
  ability cycles secrets.
* **Tech:** no fixed path; skip tech R1 ~90 %; pick what fixes the current problem —
  **X-89 is the most important** (ground decides Styx/Rex), Bio-Stims for money, Carrier II
  for capacity; War Suns only with a free-war-sun legendary/Industrex; stay green/red and use
  more fleet tokens + plain carriers rather than deep blue (454 1:38-1:48).
* Tier view: high ceiling (mobile production, hero), low floor (nothing to sell, planet-bound
  economy) (437 0:05).

### Argent Flight — F [TE]
* "Nine-point faction": consistent early (movement, decent home, good stage I) but no finisher;
  hero weaker after the TE Warfare change; breakthrough synergy is the useful part (yellow start
  → Gravity Drive, Fleet Logistics, Light/Wave). JJ: can fix a bad slice and custodians via
  Gravity Drive (437 0:08). SCPT 462: Argent can now reliably reach R1 custodians with Gravity
  Drive if it finds the money.
* RM: Argent mech (doesn't use capacity; destroyers carry mechs) is S/A tier.

### Barony of Letnev — B (C to low-A disagreement) [TE]
* Must unlock **Gravleash Maneuvers**; then takes Rex/Styx earlier and stronger than anyone.
  Weak without the Fracture; strong early token economy (+2 fleet); poor at spend objectives,
  good at controls; zero influence at home is a real custodians handicap (437 0:12; 462 1:11).
* **R1:** Trade (JJ) or Politics (Hunter: blue start + Gravleash + Antimass = likely R2
  first-action custodians, "a round-2 Imperial faction"); Construction (double dock at home,
  10 production) over Warfare (JJ) or the reverse (Hunter); Technology only as a single
  Gravity Drive; never double tech (463 1:40-1:54).
* **Tech:** Gravity Drive, Non-Euclidean Shielding, Carrier II (+ JJ's **Destroyer II often
  first**: two Destroyer IIs + carrier + fighters + 2 infantry reliably take the Fracture's
  2-planet relic system (Lethe/Phlegethon) in R2, and deter Rex contests). Dread II "dead" in
  TE (463 1:58-2:08).
* Counter-play: block your equidistants vs Barony (they can take a neighbour's home R1);
  Crimson hard-counters Barony's sustain (437 0:16; 462 1:08).

### Clan of Saar — S (Blasto A) [TE + PoK]
* Indirectly buffed by TE (faster games, Fracture, Construction changes); best at racing R1-R2;
  can nearly always take custodians if it wants (462 1:06; 437 0:18).
* **PoK guide (336):** always Chaos Mapping; Gravity Drive only if it is the difference for R1
  custodians; then Carrier II — "you could stop there", spend on plastic. Imsen's tip: build
  **infantry first** (nobody has ground forces in R1-R2, infantry don't clog capacity, a
  4-infantry-per-planet "slug trail" also gives retreat options), pivot to fighters mid-game.
  Fleet Logistics + Diplomacy in R5 = build, refund and strike in two actions (336 1:00-1:12).
* **Table behaviour:** you start with the heat — accept it; buy goodwill; extort only when a
  called bluff doesn't hurt you; cheap "go be someone else's problem" payments and support
  swaps are easy; let other threats sit at the top and be hired muscle; play tight; don't win
  from ahead (336 1:18-1:32). Vs Saar: taking both home planets is a favour to them; take
  planets in their wake; Diplomacy-lock their megafleet (336 1:17; RM "Factions we hate").

### Council Keleres — B [TE]
* Default win: Agency Supply Network → Mecatol Rex as staging area → Styx; hero can swing
  ~3 points in one turn. Very bad starting units; weak in the Fracture; outcome depends on
  table attitude (472 1:54).
* **R1:** Construction (forward **space dock** toward Rex; home build fixes the infantry gap)
  and Technology (JJ: consider first pick; being after Warfare is fine) are top; Trade great;
  Politics (custodians R2 with IIHQ building on Rex); Leadership middling; **Diplomacy very
  low** (already rich); never Imperial. If nobody takes Warfare, make sure *you* get a home
  build (472 2:00-2:14).
* **Tech:** Gravity Drive, Executive Order (Hunter hates the extra agenda phases), Agency
  Supply Network, Carrier II, X-89 (+Bio-Stims); **Fleet Logistics** is big; Fighter II as 2nd
  upgrade; buy Jol-Nar research agreement every round; a "cardboard" carrier/fighter faction;
  dreads pointless (472 2:16-2:22). Pattern from 437: root on Rex with infantry + X-89.

### Crimson Rebellion — S (#1-2) [TE]
* Untakeable home, starts with its breakthrough, DT start, bonus movement, great flagship;
  commander alliance produces huge trade goods for Crimson *and* the partner (437 0:25).
* **Standard plan (both hosts, every game):** Imperial in R2 or R3 via breaches — park a
  destroyer adjacent to Rex, start/observe a fight there to place a breach, flip it, fly the
  home fleet in, deploy a free mech, play Imperial for public + Rex point, then use Rex's
  ingress into the Fracture for Styx (430 1:42).
* R1: wants a second carrier (Sling Relay or Warfare secondary); Technology is better for
  Crimson than for others (Sling Relay = three stalls) (430 1:40).
* **Alliance:** give the commander to *static* rich factions that can't turn money into
  plastic fast (Hacan, L1Z1X, Empyrean, Deepwrought), not to Muaat/Mahact/Saar/Arborec;
  swap downstream in speaker order; Mentak (Pillage) is the worst neighbour; if you sit next to
  Crimson, bid to be its partner (430 1:46-2:02; 437 0:29).
* Keep a mech in reinforcements; place breaches frivolously; control objectives are your
  sneaky strength (430 2:06).

### Deepwrought Scholarate — S (bottom S; Hunter's #1) [TE]
* 4/4 home with coexistence, near-certain X-89 via breakthrough, huge production, rich;
  great at stage-I control objectives; weak finisher; relies on boat-floating (selling tech)
  so a cold table can starve it (437 0:31; 445 1:44).
* R1 custodians line: Leadership, follow Tech/Warfare/Diplomacy; Diplomacy refresh of the home
  planet + one planet = 6 influence. Politics high (prefer custodians at the top of R2);
  Diplomacy high as late pick; Trade always; "for Deepwrought the Technology card is roughly
  the Trade card" (money + tech from both); if holding Technology, do your own breakthrough
  first, then sell the agent and give the alliance early (445 1:20-1:40).
* "Early money is worth more than late money" — the first trade good matters, the twentieth
  doesn't (445 1:36).
* Tech: blue-green; Fighter II, Carrier II, Space Dock II; Fleet Logistics; Self-Assembly;
  Infantry II unnecessary (445 1:42).

### Embers of Muaat — D [TE]
* Slow start (move 1 until War Sun upgrade or Gravity Drive), sellable promissory, Star
  Forge stalls (437 0:35). RM: use the Muaat hero at the first good opportunity (block a path
  or an economy), don't wait for the perfect megafleet ("Factions we hate").

### Emirates of Hacan — A (low A) [TE + PoK]
* TE breakthrough pays for itself; War Suns via synergy + hero; QDN for Imperial-type
  finishes; build two docks at home early and keep a fighter screen — the 3-planet home is the
  weak point (437 0:38). TE win rate in the RM database only ~14 % (below average) (RM "Hacan").
* The RM host who played Hacan in three TE games (two wins): spread three docks over three
  systems and build three cheap ships per dock per round to farm Auto-Factories fleet supply;
  AI Dev + Sarween + 1 → War Suns R3; unlock the hero by R3 (score every round) and build war suns/flagship/dreads for
  free; QDN later for Imperial (RM "Hacan").
* Deals: fair deals spread across the table; agent "two for you, two for me"; two useless
  promissories are handy for paying Deepwrought/Naalu/Empyrean taxes (RM). SCPT "ethical
  Hacan": be the frictionless trade network; stingy Hacan loses (360 1:02).
* PoK guide (360): blue-yellow path (Gravity Drive R1, Carrier II R2, QDN — maybe early so the
  table gets used to it), Space Dock II before the hero build; the late game is hard because
  Hacan is slayable and QDN makes it "face up" — score all stage-Is early, add Imperial+Rex, and
  win through deals and surprise action cards.

### Empyrean — B (high B; later "underrated") [TE]
* Aetherstream mostly neutralised by the TE map; now a Carrier II + Gravity Drive faction; the
  main weakness is **production** — draft a 3+-resource planet for a forward dock (ideally a
  2-planet system); pitfalls: over-exploring frontier tokens ("Dark Energy Tap trap") and
  leaving plastic scattered (476 0:06-0:14, 1:46).
* R1: Trade first, Politics second (4 influence + blue start → custodians if nobody else),
  Technology next; Construction higher than usual (forward dock); Leadership/Diplomacy low;
  Warfare lowest (476 1:50-2:08).
* Core tech: Gravity Drive, Carrier II, X-89, Light/Wave (Hunter) / Fleet Logistics (Blasto).
* Good on the ground (blue-green, X-89); poor at Styx in the air; tethers can trap fleets.
  PSA: don't hand Empyrean ingress tokens (437 0:45; 462 1:34).

### Federation of Sol — S [TE + PoK]
* TE breakthrough "party bus": sling a carrier and fill it with fighters/infantry ignoring
  production; best use of a fleet token; 4-resource home; hero reaches Styx; "platonically set
  up for the TE game" (437 0:49; 476 0:06).
* PoK guide (328): R1 Warfare is viable *for Sol*: Gravity Drive via Technology secondary,
  sell Military Support (1 TG per use, as often as possible), play Warfare last and move again
  to take custodians with Trade money (+Diplomacy). Core tech is just Gravity Drive + Carrier
  II ("you could win with only that"); Bio-Stims + Sling Relay; double-teching R1 is win-more;
  shallow-tech factions should buy plastic R1 (328 1:06-1:24).

### Firmament / Obsidian — F (bottom) [TE]
* "Early economy beats late economy": bad start, monster from round 3 — too late in TE
  (437 0:51). R1: Trade if speaker; Technology (scoring last in initiative helps copy others'
  secrets); never Politics; Construction over Warfare; want someone else to take Warfare
  (449 1:42). Share secrets openly to set up copying deals; blue tech core (449 1:46-1:50).

### Ghosts of Creuss — B / low A [TE]
* Breakthrough transforms them: Malice becomes the real home; Sling Relay + dock on Malice →
  many dreadnoughts; Wormhole Generator → early Styx; easy Dread II; taking custodians more
  often (437 0:57).
* PoK guide (326): R1 **Trade** is ideal (take Malice first action, then Trade for ~15 TG of
  plastic, tech and Sling Relay); Warfare is acceptable at an uncooperative table (one starting
  carrier, but builds from home still reach good systems); Technology is "comfy" (Sling Relay
  → dreadnought to Malice); Construction/Diplomacy disliked. Tech: Sling Relay R1 (or DT if you
  survive R1 without it) → Carrier II R2; DT also lets you sit on the single choke-point out of
  the Creuss home so neighbours can't gum it (326 1:20-1:30).

### L1Z1X Mindnet — C (disputed) [TE]
* Breakthrough worth getting; extra infantry from planets/Rex; very good at Styx with little
  (X-89 easy; hero teleports so Gravity Drive is the only blue needed). Counter: park ships
  next to Styx/block the hero lane if L1 lacks Light/Wave or Fleet Logistics (437 1:01).
* PoK: L1Z1X is the table's natural slayer — leave them gas (399 1:44); their hero is now a
  Styx tool rather than a winslay tool (477 0:30).

### Last Bastion — C (bottom C) [TE]
* Not a custodians faction; mid-game Imperial; R1 Trade (needs money now), Construction
  (home build + dock), Warfare one tick below; Technology lower; tech AI Dev → Helios II dock
  upgrade, then Gravity Drive, Carrier II, Dread II, Fleet Logistics (441 1:24-1:36).
* Sell the commander (uncancellable action cards) and promissory **early** (R1 ~3-4 TG or a
  good alliance swap; R2 ~6); later, don't sell — a late buyer is holding the winning card
  (441 1:46-1:58; 437 1:08).

### Mahact Gene-Sorcerers — S (#1 by priors) [TE]; RM rated them low [PoK]
* Draft a resource-heavy slice (Bio-Stims is the early-game star); only first-pick Mahact if
  every slice works; sit next to Crimson/Deepwrought; give up speaker spots to sit next to the
  Entropic Scar (467 1:30).
* **R1:** Trade best; Construction underrated (controlled build + forward dock; Mahact skips
  the Warfare secondary since Warfare goes first and the home has 3 resources); Leadership
  mid; Warfare primary disputed; Technology mid (Hyper Metabolism R1 if blue skip, purge later);
  don't chase custodians — come to Rex in R3 (467 1:38-1:54).
* **Tech:** Gravity Drive, Carrier II, **Fleet Logistics** ("best Fleet Log faction"), X-89;
  Light/Wave, Transit Diodes (surprise Star Lancer moves), Fighter II; Cruiser II R1
  custodians is a trap (467 1:54-2:04).
* **Habits:** save tokens; every activation should capture tokens from two factions (ship of
  one, planet of another); sell your agent early in the round; R1 build plastic (it takes Rex
  in R3); bonus point mid-game, then Star Lancers to Styx (467 1:38, 2:08). RM: must *win*
  combats (Skilled Retreat denies the token); 3 captured tokens is good, 4 great; Mahact
  thrives in chaos; Mahact is the worst neighbour statistically (−13 % win rate for you).
  Counter: make every Mahact victory expensive; two-pronged attacks beat the mech "ceasefire".

### Mentak Coalition — F [TE]
* Cruiser III breakthrough still needs Cruiser II first; no closers; the biggest winner of an
  adjacent Entropic Scar (Mirror Computing R1-R2 doubles trade goods) (437 1:16; RM "Mentak").
* Sell Promise of Protection expensively; commander (4 cruisers) steals promissories after won
  combats; better in long games (RM).

### Naalu Collective — B (Hunter high A) [TE]
* Zero token still good for custodians/first-turn Imperial but R5 status matters less in TE;
  breakthrough + hero give tokens; four free mechs; not set up for Styx (437 1:18).
* RM "Factions we hate": initiative 0 + Neuroglaive + Fighter II everywhere make Naalu the
  most frustrating opponent; they score first in the decisive last round.
* PoK/Codex-3 guide (270): favourite card **Diplomacy** (R1 and again R2-R4 — ready a big
  planet on the second action); **Trade** if first pick (hope someone else takes Diplomacy and
  follow it); Construction for the forward dock (poor home production, agent can't be used at
  home); Politics useless (Neural Motivator start, zero token = no need for speaker);
  Leadership/Technology low. Path to ten: 4 stage I + 3 secrets + support + stage II or a
  late Imperial; Naalu benefits from boat-floating because it wins ties of the pack on the zero
  (270 0:58-1:14).

### Naaz-Rokha Alliance — A (mid A) [TE]
* "Voltron" breakthrough is a trap, but its colour synergy (Gravity Drive → Prefab → X-89) is
  valuable; hero (two relics) catches up secret tempo; mechs excellent (RM: S-tier mech —
  two dice, fights in space); strong in round-3 Fracture fights via the cruiser path (437 1:22).
* PoK guide (287): R1 custodians lines via Psychoarchaeology start → Cruiser II (Technology
  primary) or Gravity Drive with a blue skip, plus ~6 influence from Trade money, hazardous
  explores and Diplomacy; take Diplomacy yourself only if *you* will win the custodians race;
  if someone else holds Diplomacy and you pick early, take **Technology** to control the timing.
  Construction for a forward dock beats a double dock at home; Warfare low (287 1:32-1:42).

### Nekro Virus — B [TE]
* Faster games hurt a slow starter; Deepwrought's presence gives everyone tech (bad for
  Nekro's edge); closed-loop economy (agent, action cards), works in high-resource slices
  (437 1:27). Agenda phase: feed Nekro tech via Galactic Threat instead of being attacked (394
  0:58). RM: counter Nekro by making every victory costly.
* PoK guide (323): closed economy (agent money, commander cards) — Nekro can win even at a
  no-trade table. R1: often take **Warfare** yourself (plastic first, control objective);
  "vs Nekro, make Nekro take Warfare"; follow Politics; don't research extra tokens R1. Later:
  speaker control with Politics R2-R5 suits a faction without agenda votes. Let everyone
  research — more to copy; take Technology if nobody else will (323 1:14-1:24).

### Nomad — B (bottom B) [TE]
* Decent economy; breakthrough good only with specific agents (Hacan/Xxcha/Naalu; Hacan+Nomad
  agent loop = 8 TG each, RM); sell the Cavalry promissory, especially for Fracture fights;
  use the hero earlier (R3-R4) (437 1:32).

### Ral Nel Consortium — C ("the most average faction") [TE]
* Jack of all trades; only real closer is Overrule; not set up for custodians + Styx (437
  1:38). R1 Trade/Technology good, Construction fairly high (second, mobile dock), Diplomacy and
  Politics low; Warfare after R1 for the Fracture; blue-red tech despite a green-yellow
  breakthrough; Data Skimmer makes you a Sabotage magnet — buy the Last Bastion alliance (434).

### Sardakk N'orr — D [TE]
* TE breakthrough (win combat → token or free unit upgrade) "rescues" the faction (RM) but it
  still has no economy and gets pounced on when leading (437 1:41). Essential tech: **AI
  Development Algorithm** (with red/blue synergy unlocks nearly every unit upgrade); wants
  planet-heavy slices and planet borders with neighbours; +1 combat is best on cheap units
  (fighters, infantry) (RM "Sardakk revisited").

### Titans of Ul — A (top A; JJ bottom S) [TE]
* Versatile; Saturn Engine II all-rounder; sleepers give structures without Construction;
  weakness is influence/tokens (each Saturn Engine is a fleet token) (458 1:40).
* **Plan (very confident):** score every round from R1; either take custodians or be second to
  Rex and take Imperial R2; only Saar out-muscles them at Rex (458 1:46).
* R1: Leadership medium-high, Politics top if also scoring, Trade high, Warfare strong (no
  token, sleepers redeemable same round), Technology middling (but reach Saturn Engine II by
  R2), Construction/Diplomacy low (458 1:54-2:12).
* Tech: Saturn Engine II then **X-89** ("the finisher"); Bio-Stims, Transit Diodes, Integrated
  Economy on the way; AI Dev/Psychoarchaeology to reach Saturn II fast; Sling Relay below
  average; prefer X-89 over Fleet Logistics if only one (458 2:14-2:24).

### Jol-Nar Universities — S (bottom S) [TE + PoK]
* Gained X-89 and Mageon in TE; good at nearly every stage-I; "Strategize" action cards (437
  1:47). R2 Mageon + E-Res trick: invite others to activate your home for money.
* PoK guide (340): E-Res Siphons R1 only next to an aggressive neighbour, else R2 (sell R1
  tech via research agreement); unit upgrades Dread II, Carrier II, Space Dock II (or Sling
  Relay), Destroyer II situational; hero swap at R5 (R4 if needed) once the stage II is known —
  drop Antimass/Neural/Self-Assembly for Light/Wave, Fleet Logistics, Sling Relay+Bio-Stims,
  Assault Cannon, Duranium; research agreement ≈ 6-8 per round, sell ~3 per use and spread to
  weaker factions (340 1:14-1:46).
* Counter (RM): attack Jol-Nar early (few, weak ground forces) and refuse to pick Technology.

### Vuil'raith Cabal — A (mid A) [TE]
* Go to the Fracture in R2 for combat + relics but don't overextend — secure your own slice
  first and block the 2-planet relic system with a cruiser; Metali Void Armaments + Duranium
  becomes unbeatable; loud, draws heat; don't assume Cabal gets Styx (437 1:51).
* PoK guide (302): get **all three space docks** out fast (third one adjacent to many systems
  — equidistant or next to Rex — for Vortex and the hero), Construction R1 and R2; prefer many
  small "cookie" deals (a trade good per favour) over one big extortion; spend surplus planets
  for influence/tokens (302 1:16-1:24).

### Winnu — D (unrateable) [TE]
* "Best component in the game" (hero) — the table must point-block Winnu whenever it gets
  momentum (assume the hero is worth two points); example: Fleet Logistics + Thunder's Edge
  extra turns + hero → 3-4 point turn from behind (437 1:58; 477 0:44). "Always punch the Winnu"
  (340 1:54).

### Xxcha Kingdom — F [TE]
* Defensive abilities are worthless without a way to get ahead; only good at the agenda phase
  (437 2:02). RM: Xxcha PDS networks are frustrating; Muaat can bombard through them.

### Yin Brotherhood — C (top C) [TE]
* Hero into Styx reliably first; Yin Spinner + Integrated Economy pop-off; usually forced into
  blue tech; good at squatting on Mecatol; breakthrough-drawn commanders decide games (437
  2:06).

### Yssaril Tribes — A (top A) [TE + PoK]
* Action-card deck buffed, but heavy anti-Yssaril heat; **should not go for custodians or the
  Fracture** — sit in the slice and win the last round; R5 stalling matters less now; must get
  Gravity Drive (437 2:12).
* PoK guide (358): R1 Trade or Technology (double tech with Bio-Stims); core Bio-Stims, Mageon,
  Gravity Drive (Mageon by R3 beats Light/Wave by R3); agent copies the best agent at the table
  ("goblin fish" with Hacan ≈ 9 TG/round split); stall everyone out and finish control
  objectives last; Imperial R2 with a deployed mech is fine; get a support partner early.
* RM: ban below six players (Mageon + Bio-Stims steal too large a share of a small table's
  action cards).

---

## 4. Bot feasibility

### 4.1 What the rule bot does today (read-only review of `smart_bot.rs`)

`pick()` overrides the random bot on a few windows and leaves everything else random:
score whenever offered (first `score` option), activation/movement/landing/retreat valued by
planet worth and a Lanchester matchup (`WIN_MARGIN`), production, payment, token pools,
trades (`ACCEPT_NET`), strategy-card draft (`opening_card_score` for rounds 1-2, then "Imperial
if Mecatol is held or takeable, else most trade goods"), a scripted round-1/2 expansion
plan (three systems, Warfare free tactical action / Construction build at home), Gravity
Drive whenever offered, a few secondaries, Leadership token purchases. Technology choices
other than Gravity Drive, agenda votes, action cards, Politics' speaker choice and most other
windows are random.

The observation it gets (`SeatObservation`/`Observed`) is rich enough for most podcast
heuristics without peeking: every seat's VP, trade goods, commodities, token pools, held
strategy cards, technologies, number of secrets and action cards held (`PublicSeat`);
`scoreable_public(player)` for **any** player; `revealed_objective_progress(_gaining/
_imagining)` for counterfactual "would this action advance an objective"; `scored_by`,
`speaker`, `initiative_order`, `custodians_removed`, `laws`, `support_holders`,
`is_fracture_system`, the seat's own `held_secret_progress`. Windows exist for
`politics_choose_speaker`, `politics_place_agenda`, `cast_vote`, `research_technology`,
`return_over_secret_hand_limit` (there is no "keep 1 of 2" secret choice at setup in this
engine — one secret is dealt).

**Where the podcasts disagree with the current bot:**
* *Trade*: the bot gives Trade −10 in round 1 unless Hacan; both podcasts call Trade the best
  abstract R1 card. Caveat: much of Trade's value comes from converting commodities through
  deals, which the bot only does via its simple `propose/answer_transaction`; still, the 3 TG +
  own refresh alone is worth more than most alternatives in R1.
* *Warfare*: the bot rates Warfare highest (10) in the opening because its scripted expansion
  plan uses the free tactical action; the podcasts rank Warfare low for most factions and
  recommend leaving it to factions that need it. The bot's plan is a legitimate exception
  (the "make the poor faction take Warfare" logic assumes a human table), so this should be
  tested, not flipped blindly.
* *Imperial*: the bot already takes Imperial when Mecatol is held/takeable; podcasts want
  Imperial considered **first** from round 2 whenever it double-scores, even without Rex.

### 4.2 Candidate heuristics, rated

Value = expected win-rate impact against the current bots/humans; Effort = implementation size
in `smart_bot.rs` given the observation API (S < 1 day, M a few days, L a week+).

| # | Heuristic (source) | Window / data | Value | Effort |
|---|---|---|---|---|
| A | **Score the insecure objective first**: when several publics are scoreable in one status phase, prefer control/"hold planets"/structure objectives over tech/trade-good ones (RM; 477 1:19) | `score_*` options; objective kind via content + `revealed_objective_progress` | High | S |
| B | **Imperial priority from R2**: pick Imperial whenever `scoreable_public(me) ≥ 1` after this round's expected score (double score), even without Rex; always if Rex held (462 1:27; 458 1:50) | `draft_strategy_card`, `scoreable_public` | High | S |
| C | **Trade valued as top R1 card** (except when the scripted plan needs Warfare); keep −value for R5 (dies late) (399; RM "Trade") | `opening_card_score` | Med-High | S |
| D | **Opponent tempo model**: threat = VP + publics scoreable now (`scoreable_public(p)`) + unscored secrets held (assume scoreable) + bonus access (holds Rex & likely Imperial; custodians already) (462; 477 0:31) | `PublicSeat`, `scoreable_public` | High (enabler for E, F, K, L) | M |
| E | **Cheap point blocks**: if the top-threat player (D) is ahead of the bot and one of their next points is a control objective or Rex, value an activation that takes/contests that planet with ≤1 token + 1 capital ship at a bonus (477 0:47, 0:56) | `activation_value`, other players' `revealed_objective_progress` | High | M |
| F | **Don't feed the leader**: refuse trades whose counterparty is the top threat; prefer offers to the weakest seat (SCPT boat-float for the poor, RM stinginess toward leaders) | `pick_answer`, `pick_proposal` | Medium | S |
| G | **Faction core-tech lists**: replace "Gravity Drive only" with a per-faction priority list from §3 (e.g. Titans: Saturn Engine II→X-89; Sardakk: AI Dev; Mahact: GD, Carrier II, Fleet Logistics, X-89; Keleres: Agency Supply Network; default: GD, Carrier II, X-89, Fleet Logistics, Light/Wave) | `research_technology` | High | S-M |
| H | **Custodians R1/R2 planning**: pick Politics R1 if Mecatol is reachable next round and 6 influence will be available; Leadership in R2 when going first to Rex (462 1:19; 399) | `opening_card_score`, `mecatol_in_reach`, `available_spend` | Med-High | M |
| I | **Leave home gummed when ahead**: when the bot's threat (D) is top-2, never leave the home system without a ship/fighter; when moving the last carrier out, keep a destroyer (462 0:24) | `pick_move` | Medium | S |
| J | **Final-round card choice**: in the expected last round, if the bot can win only in the status phase take the lowest-initiative card (Leadership); take Imperial only if an action-phase win exists; avoid Technology (handing others upgrades) (399 1:58, 2:00; RM "Last round") | `draft_strategy_card`, threat model | Medium | M |
| K | **Politics speaker choice**: never give speaker to yourself in R2; give it so the bot stays a top-3 pick next round and the top threat isn't first (399 1:11) | `politics_choose_speaker` | Low-Med | S |
| L | **Agenda placement & votes**: as Politics holder, bury disruptive agendas when ahead, keep them when behind; vote against outcomes carrying the top threat's Imperial Rider; always keep one vote (394 0:14, 0:46) | `politics_place_agenda`, `cast_vote` | Medium | M |
| M | **Strategy-token reserve**: keep ≥2 strategy tokens in status-phase allocation from R2 on (Imperial + Technology secondaries) (399 0:48) | `pick_token_pool` | Low-Med | S |
| N | **Fleet composition rules**: build every round; prefer destroyers over cruisers as cheap fast hulls; counter-build destroyers vs fighter-heavy neighbours; no last-round megafleet away from home (RM "Fleet composition") | `pick_produce` | Medium | M |
| O | **Combat margin by HP and dice**: for small fights require both an HP lead and a dice lead (carrier+fighter vs carrier → add a destroyer) — refine `WIN_MARGIN` for ≤3-ship fights (477 1:14) | `matchup`, `pick_move` | Low-Med | S |
| P | **Keep action-phase secrets** when over the secret limit; discard the hardest status-phase one (462 1:12; RM "Secrets") | `return_over_secret_hand_limit`, `held_secret_progress` | Low | S |
| Q | **Diplomacy when it is the only R1 score** (spend objective revealed and planets would need readying) (399 0:54) | `opening_card_score`, objective kinds | Low-Med | S |
| R | **Construction when structure objectives are revealed** (R1-R2), else low (399 1:19; 462 1:21) | `opening_card_score` | Low-Med | S |
| S | **Technology stall / R5 avoidance**: play Technology late in the round; avoid picking it in the last round (399 1:52) | `pick_action`, draft | Low | S |
| T | **Ground-force-first build early for Saar-type factions; X-89 for ground-decided Styx/Rex fights** (336 1:06; 454 1:44) | `pick_produce`, research | Low-Med | S |
| U | Win-slay carousel coordination, alliances, supports, extortion, rider deals | needs multi-party negotiation | Medium | L (out of scope for rule-based bot) |

### 4.3 Ranked top 15 (value ÷ effort)

1. **A — Score insecure objectives first.** Trivial ordering change in the score window; directly
   protects points from the most common block. *(Overlaps the in-progress
   `smart-bot-objectives` branch, which adds "score order" — reuse rather than duplicate.)*
2. **B — Imperial first from round 2 when it double-scores.** One rule in
   `pick_strategy_card`; both podcasts' strongest single piece of advice.
3. **G — Per-faction core-tech priority list.** Research is currently random except Gravity
   Drive; a static table from §3 is cheap and affects every game.
4. **C — Re-value Trade in round 1** (behind the scripted plan's Warfare need). Run the
   existing smart-bot survey to confirm before/after.
5. **D — Opponent tempo/threat model.** Small module returning a per-seat threat score from
   public data; prerequisite for E, F, I, J, K, L.
6. **E — Cheap point blocks against the top threat.** Bonus in `activation_value` for systems
   whose planets the leader needs for a revealed control objective, capped at one token and a
   modest fleet.
7. **F — Trade stance toward the leader.** Refuse/penalise deals that help the top threat.
8. **H — Custodians planning (Politics R1 → Leadership R2 → Rex).** The bot already values
   Rex; add the card choices that make it go first.
9. **I — Keep the home system gummed when ahead.**
10. **J — Final-round strategy-card rule** (lowest initiative for a status win; Imperial only for
    an action win; never Technology).
11. **M — Strategy-token reserve of two from round 2.**
12. **L — Agenda placement/vote heuristics** (keep one vote; ahead=status quo, behind=chaos;
    vote against the leader's Imperial Rider).
13. **N — Fleet composition rules** (build every round, destroyers over cruisers, counter-build).
14. **K — Politics speaker choice** (stay a top-3 pick; deny the top threat).
15. **R/Q — Construction and Diplomacy only when a revealed objective calls for them** (one
    combined change to `opening_card_score`).

Not recommended for the rule-based bot (needs negotiation or hidden information): win-slay
carousel coordination, support-for-the-throne swaps and extortion, alliance/commander swaps,
rider auctions, secret bluffing. Ideas that require knowing opponents' secrets must use
"assume scoreable" (the podcasts' own rule) rather than peeking.

**How to validate:** the repository already has an all-smart survey (3/4/6-seat tables,
per-round standings, `SURVEY_SEEDS`) on the `smart-bot-improvements` line; each heuristic above
should be landed behind a constant and compared on that survey (win rate and VP by round),
since several podcast rules assume a human table (e.g. "make the poor faction take Warfare").
