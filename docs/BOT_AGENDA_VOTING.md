# Rule bot: agenda voting, vote budget and riders

Branch `bot-agenda-voting` (from `release-2026-10-16`). Bot-only work: every new behaviour sits behind new
knobs in `crates/ti4-rule-bot/src/tuning.rs` that are **off in bot versions 1 to 4**; adopting them would be a
version 5 decision (decided: all three knobs are in the v5 staging table, see the Knobs section). Code: `crates/ti4-rule-bot/src/agenda_vote.rs` (valuation, vote budget, riders), hooks in
`answers.rs`.

## 1. What the bot did before

`answers.rs` (`preferred_outcome`, `cast_vote`, `exhaust_for_vote`): a fixed table. Elections that give the
elected something: vote for itself; `execution`: the VP leader; planet attachments: its own planet, harmful ones: the
leader's planet; `seed_empire`, `economic_equality`, `mutiny`, `unconventional`, `articles_war`, `incentive` by a one-line
rule; **no opinion on everything else** (abstain). When it has an opinion it exhausts every planet for it, biggest
first. It never keeps votes, never looks at the board (units, planets, techs, faction) and it plays rider cards at
random: in a `reaction_when_AGENDA_REVEALED` window the bot has no rule, so the seeded random pick plays a rider about as
often as it declines.

## 2. Model (what the new code computes)

For each outcome `o` of an agenda the module computes a change `d_p(o)` of every seat `p` in **value points** (one VP
is 10; one trade good 1; an action card 1.5; a command token 2.5; a technology 5 to 6; a unit its
`ti4_engine::valuation::unit_value` cost-like price: dreadnought 4, war sun 10, ...; a planet 0.8 x (resources +
influence) per remaining round). From these:

    score(o) = d_me(o) - w * sqrt(n-1) * mean_r( t_r * d_r(o) )

with the rivals' weight `w` (knob `agenda_rival_weight`, 0.5) and a threat factor `t_r` = 1 + 0.15 x (VP of r - my VP),
clamped to 0.5..2, and a *win pressure* for VP gains (a gain that takes a seat to 10 VP counts 4 times). The bot votes for the
outcome with the best score when its margin to the second best exceeds `agenda_min_margin` (1.5); otherwise it has
**no opinion** and abstains, as before. Everything uses public information only (the board, VP, trade goods, token
pools, technologies, hand sizes, faction of the others) plus the bot's own seat.

Elections evaluate each candidate (a player: the gain/loss of that player; a planet: the effect on its controller and
the units standing on it). *Voter-conditional* agendas (Mutiny, Unconventional Measures, Articles of War, Conventions,
Representative Government, Wormhole Research) reward or punish the players who voted a side: the expected value of
casting a vote on side `X` is `P(X wins) x gain(X) + P(other wins) x loss`, with `P` from the vote model below, and
abstaining is an option.

Vote model (used by the budget and by the riders): the running tally per outcome is on the vote choice
(`current_votes`), the remaining voters (seating order from the speaker's left, the speaker last) are known, and
a remaining rival casts all its readied influence for *its* preferred outcome. That preference is the same valuation computed
from the rival's seat (public information only); for an unknown preference the rival goes against me with probability
`agenda_rival_share` (0.6). The total against me is approximately normal.

## 3. Vote budget (keeping votes for the second agenda)

Facts from the engine (`vote.rs`, `agenda.rs`): two agendas are revealed per phase; exhausting a planet casts its
**full** influence; exhausted planets are readied only after the second agenda (so spending on the first reduces the
second; there is no spending in between); trade goods are not spent. Only the Checks and Balances Against law (readies
only 3 planets) makes *which* planet is exhausted matter for the next round; otherwise the resource value of the
planets is irrelevant, only the influence total is.

Rule (`agenda_budget`): when the bot has an opinion it picks the number of votes `v` that maximises

    EV(v) - lambda x v,   EV(v) = P(my outcome wins | v) x score(mine) + (1 - P) x score(best alternative)

where `lambda` is the value of one vote in the *second* agenda (0 on the second agenda, and 0 for votes I could not use anyway):
`lambda = stake2 x 0.4 / sd2`, with `stake2` the margin of the second agenda when it is known and `agenda_second_prior`
(2.0) when not, and `sd2` the spread of rivals' votes. It never overspends: when the outcome is already safe or lost
with every vote it casts nothing (or the one vote that Mutiny-type agendas need). The planets are chosen as the subset
of readied planets with the smallest total influence that reaches `v` (smallest overshoot, so the most influence is
left for the second agenda), ties by the lowest resource value (they matter for Checks and Balances).

*Knowing the second agenda.* The observation exposes no queue. Honest sources: (1) the bot itself played Politics (or
Insider Information) in that round and placed agendas on top: the rule remembers its own placement (the engine reveals
the last card placed on top first) and checks the first revealed agenda against the memory; (2) the Naalu commander
look (`agenda_deck_ends`) shows the card *after* the second agenda, so it is not used. When the next agenda is known,
`stake2` is computed with the same valuation on it (and a known harmful agenda, e.g. Execution against me, makes the bot keep its votes).

## 4. Riders

Rider cards (`action_cards.json`, engine `action_cards.rs`): Imperial Rider (1 VP), Leadership Rider (3 command
tokens), Politics Rider (3 action cards and the speaker token), Technology Rider (research a technology), Trade Rider
(5 trade goods), Warfare Rider (a dreadnought in a system with own ships), Construction Rider (a space dock), Diplomacy
Rider (a token in a system for each other player), the Keleres Rider (an action card and 2 trade goods) and, from Codex 1, Sanction (a
prediction that returns tokens of the voters). A rider is played in the `reaction_when_AGENDA_REVEALED` window, *before*
any vote; the player then cannot vote on that agenda and predicts an outcome (`predict_agenda_outcome`). If right,
the payoff arrives.

Rule (`agenda_riders`), behind its own knob:

* **Play** when `P(correct) x benefit - cost >= 0`, `P(correct)` at least `rider_min_p` (0.7), where `benefit` is the value of the
  payoff (Imperial 10, Leadership 7.5, Politics 6.5, Technology 0 (the engine records a correct Technology Rider but does not perform its research), Trade 5, Warfare 4, Construction 4 when a planet
  without a dock is held, Diplomacy 1.5, Keleres 3.5) and `cost` is the value of my own votes on this agenda: the change
  `(P(win | my votes) - P(win | no votes)) x margin` plus the voter-conditional gain. The votes the rider frees are readied
  for the next agenda (a small bonus, only with the budget knob).
* So a rider is played when the outcome is **irrelevant to me** (margin below the threshold: my votes are worth nothing) and it
  is predictable, and also when my preferred outcome wins without me (probability above 0.85 even without my votes).
* **Do not** play it when I prefer an outcome that depends on my votes, when I would be on the likely losing side, when the outcome is a
  coin flip, or when the payoff cannot be used (Warfare without ships, Construction without a free planet).
* The prediction is the outcome with the highest forecast probability.
* In an agenda reveal window the other cards (Veto, Hack, ...) are declined under the knob; their previous random plays were noise.

## 5. Faction logic overview

| Faction | Needs specific logic because | Where |
|---|---|---|
| Muaat | war suns (Prototype War Sun II, Magmus Reactor) and a bombarding flagship | `schematics` never For; `conventions`, `arms_reduction` |
| Letnev | dreadnoughts, bombardment, Armada of Fire (small fleet pool) | `arms_reduction`, `regulations`, `conventions`, `schematics` |
| Sardakk | dreadnoughts, many cheap units | `arms_reduction`, `conscription`, `plowshares` |
| L1Z1X | Super-Dreadnought II counts as a dreadnought; infantry | `arms_reduction` |
| Nekro | mechs, technology, no research of its own kind | `articles_war`, `revolution` |
| Yin | cheap infantry / dreadnought + cruiser mix | `disarmament`, `redistribution` |
| Argent | PDS-like defence, fighters | `defense_act`, `prophecy`, `conscription` |
| Naalu | fighters, mechs, deck peek | `conscription`, `prophecy` |
| Winnu | mechs, Mecatol | `articles_war`, `artifact` |
| Xxcha | planets and votes (Elder Qanoj), PDS | `rep_govt`, `checks`, `defense_act` |
| Hacan | trade goods are votes and trade | `economic_equality`, `minister_commerce`, `checks` |
| Mahact | fleet pool holds foreign tokens | `regulations`, `cladenstine`, `standardization` |
| Creuss (`ghost`) | wormholes | `travel_ban`, `wormhole_recon`, `wormhole_research`, `nexus` |
| Jol-Nar | technology | `revolution`, `artifact`, `minister_sciences` |
| Yssaril | action cards | `sanctions`, `censure`, `unconventional`, `minister_policy` |
| Sol, Arborec | infantry and planets | `plowshares`, `disarmament`, `conscription` |
| Empyrean | nebula home, wormholes | `shared_research`, `wormhole_*` |

Everything not listed uses the generic valuation, which already reads the faction's units, planets, tokens and
technologies from the board.

The same table in code is the `faction` adjustments in `agenda_vote.rs`.


## 6. The agendas

All 63 agendas of the content (`crates/ti4-content/content/agendas.json`: 50 base-game cards, 13 Prosperity-of-Kings cards; the content has no Thunder's Edge agenda card; the decks `agendas_base_game` and `agendas_pok` draw from them) are listed once, grouped by what they do. Each entry: the printed text, the outcome type, who it helps and hurts, the heuristic and the factions that need special logic. `type` is Law (stays in play with the elected outcome) or Directive (one-off).


### A. Elect a player (a card or a one-off gain for the elected)

#### `secret` - Archived Secret (Directive, Elect Player, base)

- **Card:** The elected player draws 1 secret objective.

- **Helps / hurts:** Elected player draws 1 secret objective (about +0.35 VP). Helps only the elected.

- **Heuristic:** Elect myself. Value 3.5 for the elected; only the elected player matters, so the margin between me and any rival is the full value.

- **Faction logic:** Factions with few scoring routes value it slightly more; no special code.

#### `committee` - Committee Formation (Law, Elect Player, base)

- **Card:** The elected player gains this card. Before players vote on an agenda that requires a player to be elected, the owner of this card may discard this card to choose a player to be elected. Players do not vote on that agenda.

- **Helps / hurts:** Elected player owns the card and may later name the winner of an election agenda. Helps the elected, hurts the others in every later election.

- **Heuristic:** Elect myself (value 4). A rival holding it is worth -4 to me, so I vote for myself with everything when a rival leads the tally.

- **Faction logic:** Vote-rich factions (Xxcha, Hacan) lose less from a rival holding it; no special code.

#### `arbiter` - Imperial Arbiter (Law, Elect Player, base)

- **Card:** The elected player gains this card. At the end of the strategy phase, the owner of this card may discard this card to swap 1 of their strategy cards with 1 of another player's strategy cards.

- **Helps / hurts:** Elected may swap one strategy card with another player's at the end of the strategy phase (one use).

- **Heuristic:** Elect myself (value 3).

- **Faction logic:** -

#### `minister_commerce` - Minister of Commerce (Law, Elect Player, base)

- **Card:** The elected player gains this card. After the owner of this card replenishes commodities, they gain 1 trade good for each player that is their neighbor.

- **Helps / hurts:** Elected: +1 trade good per neighbour after replenishing commodities, each round.

- **Heuristic:** Elect myself; value = neighbours (2 by default) x remaining rounds x 0.7.

- **Faction logic:** Hacan (trade goods are votes and trade) x1.5.

#### `minister_exploration` - Minister of Exploration (Law, Elect Player, base)

- **Card:** The elected player gains this card. When the owner of this card gains control of a planet, they gain 1 trade good.

- **Helps / hurts:** Elected: 1 trade good whenever it gains control of a planet.

- **Heuristic:** Elect myself (about 0.8 per remaining round).

- **Faction logic:** Expanding factions (Saar, Arborec, Sol) x1.3.

#### `minister_industry` - Minister of Industry (Law, Elect Player, base)

- **Card:** The elected player gains this card. When the owner of this card places a space dock in a system, their units in that system may use their PRODUCTION abilities.

- **Helps / hurts:** Elected: a space dock placed lets that system's units produce at once.

- **Heuristic:** Elect myself (value 2.5).

- **Faction logic:** -

#### `minister_peace` - Minister of Peace (Law, Elect Player, base)

- **Card:** The elected player gains this card. After a player activates a system that contains 1 or more of a different player's units, the owner of this card may discard this card - immediately end the active player's turn.

- **Helps / hurts:** Elected may cancel an opponent's activation of a system holding their units (one use).

- **Heuristic:** Elect myself (value 4: a one-shot panic button). A rival holding it is the same 4 against me.

- **Faction logic:** Attackers (Letnev, Sardakk, Muaat) lose most from a rival holding it.

#### `minister_policy` - Minister of Policy (Law, Elect Player, base)

- **Card:** The elected player gains this card. At the end of the status phase, the owner of this card draws 1 action card.

- **Helps / hurts:** Elected draws 1 action card at the end of each status phase.

- **Heuristic:** Elect myself (1.5 per remaining round x 0.9).

- **Faction logic:** Yssaril, Nekro x1.3.

#### `minister_sciences` - Minister of Sciences (Law, Elect Player, base)

- **Card:** The elected player gains this card. When the owner of this card resolves the primary or secondary ability of the "Technology" strategy card, they do not need to spend resources to research technology.

- **Helps / hurts:** Elected researches for free with the Technology strategy card (primary and secondary).

- **Heuristic:** Elect myself (2.5; x1.6 when I hold the Technology card).

- **Faction logic:** Jol-Nar, Nekro, Hacan x1.3.

#### `minister_war` - Minister of War (Law, Elect Player, base)

- **Card:** The elected player gains this card. The owner of this card may discard this card after performing an action to remove 1 of their command tokens from the game board and return it to their reinforcements - then they may perform 1 additional action.

- **Helps / hurts:** Elected may discard it after an action to gain one extra action (one use).

- **Heuristic:** Elect myself (value 3).

- **Faction logic:** -

#### `prophecy` - Prophecy of Ixth (Law, Elect Player, base)

- **Card:** The elected player gains this card. The owner of this card applies +1 to the result of their fighter's combat rolls. When the owner of this card uses PRODUCTION, they discard this card unless they produce 2 or more fighters.

- **Helps / hurts:** Elected fighters hit +1; the card is discarded when its owner produces fewer than 2 fighters.

- **Heuristic:** Elect myself only with 4 or more fighters on the board (value 1.5), otherwise 0.3. Almost no stake; the margin threshold turns it into 'no opinion'.

- **Faction logic:** Naalu, Sol, Argent, Saar (fighters).

#### `shard_of_the_throne` - Shard of the Throne (Law, Elect Player, base)

- **Card:** The elected player gains this card and 1 victory point. A player gains this card and 1 victory point when they win a combat against the owner of this card. Then, the previous owner of this card loses 1 victory point.

- **Helps / hurts:** Elected gains the card and 1 VP; another player takes both (and the VP) by winning a combat against the holder.

- **Heuristic:** Elect myself when my fleet is the strongest or second strongest (value 7, 5 otherwise); a rival holder is -7 for me (worse near 10 VP). If I cannot win it, vote for the rival with the weakest fleet (the VP will move).

- **Faction logic:** Letnev, Sardakk, Muaat keep it; Mentak and Yssaril lose it easily.

#### `crown_of_emphidia` - The Crown of Emphidia (Law, Elect Player, base)

- **Card:** The elected player gains this card and 1 victory point. A player gains this card and 1 victory point after they gain control of a planet in the home system of this card's owner. Then, the previous owner of this card loses 1 victory point.

- **Helps / hurts:** Elected gains the card and 1 VP; lost to a player who takes a planet in the holder's home system.

- **Heuristic:** Elect myself (value 8, 6 when a rival fleet stands next to my home). A rival holder is -8.

- **Faction logic:** Raiders (Mentak, Sardakk, Nekro) treat the holder as a target.

#### `crown_of_thalnos` - The Crown of Thalnos (Law, Elect Player, base)

- **Card:** The elected player gains this card. During each combat round, the owner of this card may reroll any number of dice; they must destroy each of their units that did not produce a hit with its reroll.

- **Helps / hurts:** Elected may reroll combat dice; units that miss on the reroll are destroyed. Optional.

- **Heuristic:** Elect myself (value 3). The card is optional so it never hurts the holder; a rival holder is a threat (-3).

- **Faction logic:** Sardakk, Jol-Nar slightly higher.

#### `standardization` - Armed Forces Standardization (Directive, Elect Player, pok)

- **Card:** The elected player place command tokens from their reinforcements so that they have 3 tokens in their tactic pool, 3 tokens in their fleet pool, and 2 tokens in their strategy pool. They return any excess tokens to their reinforcements.

- **Helps / hurts:** Elected resets pools to 3 tactic / 3 fleet / 2 strategy and returns the excess.

- **Heuristic:** Elect the player with the fewest tokens in total (gain 2.5 per missing token up to 8, x0.7 because the split is fixed). Never myself when I hold more than 8.

- **Faction logic:** Mahact (foreign tokens in the fleet pool) and Letnev lose from the fixed split; computed from the public tokens.

#### `censure` - Political Censure (Law, Elect Player, pok)

- **Card:** The elected player gains this card and 1 victory point. The elected player cannot play action cards. If the owner of this card loses this card, they lose 1 victory point.

- **Helps / hurts:** Elected gains the card and 1 VP but cannot play action cards.

- **Heuristic:** Elect myself (net +8.5: 1 VP minus the action cards I would play). A rival holder is -8.5 for me.

- **Faction logic:** Yssaril (action-card faction) values it far lower (-4 net).

#### `grant_reallocation` - Research Grant Reallocation (Directive, Elect Player, pok)

- **Card:** The elected player gains any 1 technology of their choice. Then, for each prerequisite on that technology, they remove 1 token from their fleet pool and return it to their reinforcements.

- **Helps / hurts:** Elected gains any 1 technology and returns fleet tokens equal to its prerequisites.

- **Heuristic:** Elect myself when I have 2 or more fleet tokens (value 6 minus 2.5 per prerequisite, about +1.5); otherwise neutral.

- **Faction logic:** Letnev and Mahact (fleet pool matters) lower.

#### `warrant` - Search Warrant (Law, Elect Player, pok)

- **Card:** The elected player gains this card and draws 2 secret objectives. The owner of this card plays with their secret objectives revealed.

- **Helps / hurts:** Elected gains the card, draws 2 secret objectives and plays with secrets revealed.

- **Heuristic:** Elect myself (value 4.5: two secrets for about 0.7 VP minus the denial risk).

- **Faction logic:** Surprise scorers lower.

#### `minister_antiquities` - Minister of Antiquities (Directive, Elect Player, pok)

- **Card:** The elected player gains 1 relic.

- **Helps / hurts:** Elected gains 1 relic.

- **Heuristic:** Elect myself (value 7).

- **Faction logic:** Explorers x1.3.

#### `execution` - Public Execution (Directive, Elect Player, base)

- **Card:** The elected player discards all of their action cards. If they have the speaker token, they give it to the player on their left. The elected player cannot vote on any agendas during this agenda phase.

- **Helps / hurts:** Elected discards all action cards, cannot vote this agenda phase, passes the speaker token on.

- **Heuristic:** Never elect myself. Elect the rival who loses most (action cards x1.5 + 0.6 x readied influence + 2 for the speaker token); the readied influence term also makes it a vote-denial play when a second agenda follows.

- **Faction logic:** Xxcha (votes cannot be silenced) lose only the cards; Yssaril loses most.


### B. Elect a planet (attachments, destruction)

#### `core_mining` - Core Mining (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. Then, destroy 1 infantry on the planet. The resource value of this planet is increased by 2.

- **Helps / hurts:** Elected planet: +2 resources, one infantry destroyed. Helps its controller.

- **Heuristic:** Elect my best planet (value 0.8 x remaining rounds x 2 minus 1.5); a rival's planet is a gain for them.

- **Faction logic:** -

#### `demilitarized_zone` - Demilitarized Zone (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. Then, destroy all units on that planet. Player's units cannot land, be produced, or be placed on this planet.

- **Helps / hurts:** Elected planet: all units destroyed, nothing may land or be produced there again.

- **Heuristic:** Elect the rival planet with the most unit value (docks count 4) and the highest resource value; never one that holds units of mine, and never Mecatol Rex if I hold it. Replaces the old 'rival leader's planet' rule with a loss estimate.

- **Faction logic:** Planet-based factions (Titans, Arborec, Sol docks) lose most.

#### `holy_planet_of_ixth` - Holy Planet of Ixth (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. The planet's owner gains 1 victory point. Units on this planet cannot use PRODUCTION. When a player gains control of this planet, they gain 1 victory point. When a player loses control of this planet, they lose 1 victory point.

- **Helps / hurts:** Attached planet: owner gains 1 VP (the VP follows control), no PRODUCTION there.

- **Heuristic:** Elect my own planet (value 9: 1 VP minus a dock if present), the one least exposed to rival fleets. A rival's planet is +9 for them (more when they near 10 VP).

- **Faction logic:** Aggressors (Letnev, Sardakk) can take it back.

#### `rt_biotic` - Research Team: Biotic (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 green prerequisite.

- **Helps / hurts:** Attached: owner may ignore 1 green prerequisite when researching.

- **Heuristic:** Elect my planet (value 2.5).

- **Faction logic:** -

#### `rt_cybernetic` - Research Team: Cybernetic (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 yellow prerequisite.

- **Helps / hurts:** Attached: owner may ignore 1 yellow prerequisite.

- **Heuristic:** As Biotic.

- **Faction logic:** -

#### `rt_propulsion` - Research Team: Propulsion (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 blue prerequisite.

- **Helps / hurts:** Attached: owner may ignore 1 blue prerequisite.

- **Heuristic:** As Biotic.

- **Faction logic:** -

#### `rt_warfare` - Research Team: Warfare (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. When the owner of this planet researches technology, they may exhaust this card to ignore 1 red prerequisite.

- **Helps / hurts:** Attached: owner may ignore 1 red prerequisite.

- **Heuristic:** As Biotic.

- **Faction logic:** Muaat (War Sun prerequisites) x1.5.

#### `senate_sanctuary` - Senate Sanctuary (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. The influence value of this planet is increased by 2.

- **Helps / hurts:** Attached: +2 influence on the planet.

- **Heuristic:** Elect my planet (value 0.7 x remaining rounds x 2); the extra votes also help later elections.

- **Faction logic:** Hacan, Xxcha x1.3.

#### `terraforming_initiative` - Terraforming Initiative (Law, Elect Planet, base)

- **Card:** Attach this card to the elected planet's card. The resource and influence values of this planet are increased by 1.

- **Helps / hurts:** Attached: +1 resource and +1 influence.

- **Heuristic:** Elect my planet (value 0.8 x remaining rounds x 2).

- **Faction logic:** -

#### `redistribution` - Colonial Redistribution (Directive, Elect Non-Home Planet Other Than Mecatol Rex, base)

- **Card:** Destroy each unit on the elected planet. Then, the player who controls that planet chooses 1 player with the fewest victory points - that player may place 1 infantry from their reinforcements on that planet.

- **Helps / hurts:** Non-home planet other than Mecatol: all units on it destroyed; the lowest-VP player may put an infantry there.

- **Heuristic:** Elect the rival planet with the greatest unit value at stake; never a planet I garrison. Structures count. The consolation infantry is ignored.

- **Faction logic:** Planet-based factions lose most.

#### `disarmament` - Compensated Disarmament (Directive, Elect Planet, base)

- **Card:** Destroy each ground force on the elected planet. For each unit that was destroyed, the player who control that planet gains 1 trade good.

- **Helps / hurts:** Elected planet: all ground forces destroyed, the controller gains 1 trade good each.

- **Heuristic:** Elect the rival planet with the most ground-force value net of the trade goods paid out; never one of my garrisons, and prefer not to let Mecatol be elected when I hold it with many ground forces.

- **Faction logic:** Cheap-infantry factions (Sol, Arborec, Yin, Sardakk) lose most.


### C. Elect something else (law, secret objective, strategy card)

#### `classified` - Classified Document Leaks (Law, Elect Scored Secret Objective, base)

- **Card:** The elected secret objective becomes a public objective - place it near the other public objectives in the common play area.

- **Helps / hurts:** Elect a scored secret objective: it becomes public. In this engine no candidates are ever offered, so the card is discarded without a vote.

- **Heuristic:** No decision exists in this engine (documented, no code). If it ever did: elect an objective I have scored myself.

- **Faction logic:** -

#### `abolishment` - Judicial Abolishment (Directive, Elect Law, base)

- **Card:** Discard the elected law from play.

- **Helps / hurts:** Elect a law in play: it is discarded. Skipped when no law is in play.

- **Heuristic:** Vote to abolish the law that costs me most (or helps rivals most): value = minus the law's ongoing value to me plus the rivals' ongoing value. Never abolish a card I hold.

- **Faction logic:** Letnev/Mahact dislike regulations, Yssaril/Hacan sanctions, Muaat schematics, Creuss travel ban.

#### `miscount` - Miscount Disclosed (Directive, Elect Law, base)

- **Card:** (When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) Vote on the elected law as if it were just revealed from the top of the deck.

- **Helps / hurts:** Elect a law in play: it is voted on again. Skipped when no law is in play.

- **Heuristic:** Re-vote a law that hurts me (half the abolishment value, the re-vote may confirm it); neutral for laws that help me.

- **Faction logic:** As abolishment.

#### `crisis` - Galactic Crisis Pact (Directive, Elect Strategy Card, pok)

- **Card:** Each player may perform the secondary ability of the elected strategy card without spending a command token - command tokens placed by the ability are placed from a player's reinforcements instead.

- **Helps / hurts:** Elect a strategy card: everyone may use its secondary for free. In this engine no candidates are offered, discarded without a vote.

- **Heuristic:** No decision exists in this engine (documented, no code). If it ever did: elect a card whose primary I hold.

- **Faction logic:** -

#### `covert` - Covert Legislation (Directive, For / Against, pok)

- **Card:** 

- **Helps / hurts:** The speaker draws the next agenda secretly; players vote on its outcomes unseen. In this engine the card is a plain For/Against vote with no effect (`agenda_effects.rs`: a no-op).

- **Heuristic:** No stake: no opinion and no votes cast, so every planet stays readied for the second agenda (the budget rule's cheapest win). If a real hidden agenda is ever modelled: elect myself when players are offered, otherwise abstain.

- **Faction logic:** -


### D. Victory-point agendas

#### `seed_empire` - Seed of an Empire (Directive, For / Against, base)

- **Card:** For: The player with the most victory points gains 1 victory point. Against: The player with the fewest victory points gains 1 victory point.

- **Helps / hurts:** For: the player(s) with the most VP gain 1 VP. Against: the player(s) with the fewest gain 1 VP.

- **Heuristic:** Value the VP change of every seat per outcome with a win-pressure term (a rival reaching 10 VP is a lost game, so it counts triple). For when I lead, Against when I trail or when the leader is a rival close to winning.

- **Faction logic:** -

#### `mutiny` - Mutiny (Directive, For / Against, base)

- **Card:** For: Each player that voted "For" gains 1 victory point. Against: Each player that voted "For" loses 1 victory point.

- **Helps / hurts:** For: each voter For gains 1 VP. Against: each voter For loses 1 VP. Only voters For are affected.

- **Heuristic:** Voter-conditional: vote For (with the votes needed to secure it) when For is likely to pass; if rivals are on the For side and Against can win, vote Against to strip them; otherwise abstain. Never join a For side that will lose.

- **Faction logic:** Xxcha, Hacan (large vote totals) can force the outcome.

#### `incentive` - Incentive Program (Directive, For / Against, base)

- **Card:** For: Draw and reveal 1 stage I public objective from the deck and place it near the public objectives. Against: Draw and reveal 1 stage II public from the deck and place it near the public objectives.

- **Helps / hurts:** For: reveal a stage I public objective. Against: reveal a stage II objective (2 VP, harder).

- **Heuristic:** Stage II rewards the strong: Against when I lead on VP by 2 or more, otherwise For (the old rule is the default). Tiny stake.

- **Faction logic:** -


### E. Destroy or add units, combat rules

#### `arms_reduction` - Arms Reduction (Directive, For / Against, base)

- **Card:** For: Each player destroys all but 2 of their dreadnoughts and all but 4 of their cruisers. Against: At the start of the next strategy phase, each player exhausts each of their planets that have a technology specialty.

- **Helps / hurts:** For: everyone keeps at most 2 dreadnoughts and 4 cruisers. Against: at the start of the next strategy phase every planet with a technology specialty is exhausted.

- **Heuristic:** For costs 4 per dreadnought above 2 and 2 per cruiser above 4, for me and for each rival. Against costs 0.6 x (resources + influence) of my specialty planets for a round. Take the outcome with the best (rivals' loss - mine).

- **Faction logic:** Letnev, Sardakk, L1Z1X (dreadnoughts) vote Against; Jol-Nar, Winnu, Nekro (specialty planets) lean For.

#### `artifact` - Ixthian Artifact (Directive, For / Against, base)

- **Card:** For: The speaker rolls 1 die. If the result is 6-10, each player may research 2 technologies. If the result is 1-5, destroy all units in Mecatol Rex's system, and each player with units in systems adjacent to Mecatol Rex's system destroys 3 of their own units in each of those systems. Against: No effect.

- **Helps / hurts:** For: speaker rolls a die; 6-10: everybody may research 2 technologies; 1-5: all units in Mecatol's system die and 3 own units in each adjacent system. Against: nothing.

- **Heuristic:** For is 50/50: 0.5 x (2 techs x 5) minus 0.5 x (all my units in Mecatol's system + my cheapest 3 units in each adjacent system), same for rivals. Holding Mecatol with a fleet means Against.

- **Faction logic:** Holder of Mecatol: Against. Jol-Nar, Nekro: For.

#### `plowshares` - Swords to Plowshares (Directive, For / Against, base)

- **Card:** For: Each player destroys half of their infantry on each planet they control, rounded up. Then, each player gains trade goods equal to the number of their infantry that were destroyed. Against: Each player places 1 infantry from their reinforcements on each planet they control.

- **Helps / hurts:** For: everyone destroys half their infantry on each planet (rounded up) and gains 1 trade good each. Against: each controlled planet gets +1 infantry.

- **Heuristic:** For is near neutral (a trade good per infantry) but empties garrisons; Against is +1 infantry per planet for everyone, better for the wide player.

- **Faction logic:** Sol, Arborec, Xxcha (infantry, many planets) Against.

#### `rearmament` - Rearmament Agreement (Directive, For / Against, pok)

- **Card:** For: Each player places 1 mech from their reinforcements on a planet they control in their home system. Against: Each player replaces each of their mechs with 1 infantry from their reinforcements.

- **Helps / hurts:** For: everyone places 1 mech from reinforcements on a home planet. Against: every mech is replaced by an infantry.

- **Heuristic:** For: +3.5 each (mech in reinforcements). Against: -2.5 per mech on the board. A mech-poor bot votes Against when rivals field 3 or more.

- **Faction logic:** Mech factions (Sol, Nekro, Winnu, Naalu) vote For.

#### `schematics` - Publicize Weapon Schematics (Law, For / Against, base)

- **Card:** For: If any player owns a war sun technology, all players may ignore all prerequisites on war sun technologies. All war suns lose SUSTAIN DAMAGE. Against: Each player that owns a war sun technology discards all of their action cards.

- **Helps / hurts:** For: if anyone owns a war sun technology everyone ignores war sun prerequisites; all war suns lose SUSTAIN DAMAGE. Against: players owning a war sun technology discard all action cards.

- **Heuristic:** For: each war sun I own loses 3.5; a bot building war suns loses its plan (Muaat flat -15); players without one gain access (+2). Against: owners discard their cards (1.5 per card). Muaat always votes Against, as does any seat with war suns or more war sun owners than rivals' gain.

- **Faction logic:** Muaat (Prototype War Sun II): never For. Letnev, Sol, Titans lean Against once they own the technology.

#### `articles_war` - Articles of War (Law, For / Against, pok)

- **Card:** For: All mechs lose their printed abilities except for SUSTAIN DAMAGE. Against: Each player that voted "For" gains 3 trade goods.

- **Helps / hurts:** For: all mechs lose their printed abilities (keep SUSTAIN DAMAGE). Against: each voter For gains 3 trade goods.

- **Heuristic:** Voter-conditional. For costs 1.5 per mech on the board. If Against wins, voters For get 3 trade goods (the old rule voted For always). Vote For with minimal votes when I own no mechs, Against otherwise.

- **Faction logic:** Mech factions (Sol, Nekro, Winnu, Naalu, Titans, Letnev) Against.

#### `conscription` - Regulated Conscription (Law, For / Against, base)

- **Card:** For: When a player produces units, they produce only 1 fighter and infantry for its cost instead of 2. Against: No effect.

- **Helps / hurts:** For: producing fighters/infantry yields 1 unit instead of 2 for the cost. Against: nothing.

- **Heuristic:** Against costs nothing, so For only when rivals lose more cheap production than I do (fighters + infantry on the board as the proxy).

- **Faction logic:** Sol, Arborec, Naalu, Saar (cheap production) vote Against.


### F. Command tokens, exhausted planets, voting rules

#### `regulations` - Fleet Regulations (Law, For / Against, base)

- **Card:** For: Each player cannot have more than 4 tokens in their fleet pool. Against: Each player places 1 command token from their reinforcements in their fleet pool.

- **Helps / hurts:** For: fleet pool capped at 4 tokens. Against: everyone adds 1 token to the fleet pool.

- **Heuristic:** For costs 2.5 per fleet token above 4; Against is +2.5 for everyone. For only when rivals' pools exceed mine.

- **Faction logic:** Letnev and Mahact (fleet pools) hate For.

#### `cladenstine` - Clandestine Operations (Directive, For / Against, pok)

- **Card:** For: Each player removes 2 command tokens from their command sheet and returns those tokens to their reinforcements. Against: Each player removes 1 command token from their fleet pool and returns that token to their reinforcements.

- **Helps / hurts:** For: everyone returns 2 command-sheet tokens. Against: everyone returns 1 fleet-pool token.

- **Heuristic:** For: -2.5 per token (up to 2); Against: -2.5 if the fleet pool is non-empty. The cheaper one relative to rivals.

- **Faction logic:** Fleet-pool-heavy factions (Mahact, Letnev) pick For.

#### `shared_research` - Shared Research (Law, For / Against, base)

- **Card:** For: Each player's units can move through nebulae. Against: Each player places a command token from their reinforcements in their home system, if able.

- **Helps / hurts:** For: units may move through nebulae. Against: everyone places a token from reinforcements in their home system.

- **Heuristic:** Against costs 1.5 for everyone; For lets fleets cross nebulae (small). For unless a nebula guards my home.

- **Faction logic:** Empyrean (nebula home) votes Against.

#### `checks` - Checks and Balances (Law, For / Against, pok)

- **Card:** For: When a player chooses a strategy card during the strategy phase, they give that strategy card to another player who does not have 1 (or a player who does not have 2 in a three- or four-player game), if able. Against: Each player readies only 3 of their planets at the end of this agenda phase.

- **Helps / hurts:** For: a chosen strategy card is handed to another player (who has none; two in 3-4 player games). Against: only 3 planets are readied at the end of this agenda phase.

- **Heuristic:** For breaks the draft (bots have no give-away logic): -4 for me and -4 for each rival (net mild Against). Against costs each player the value of the planets exhausted beyond 3, which is why the vote budget exhausts few planets once it is in force.

- **Faction logic:** Xxcha and Hacan (many planets exhausted for votes) hate Against.

#### `rep_govt` - Representative Government (Law, For / Against, pok)

- **Card:** For: Players cannot exhaust planets to cast votes during the agenda phase. each player may cast 1 vote on each agenda instead. Players cannot cast additional votes. Against: At the start of the next strategy phase, each player that voted "Against" exhausts all of their cultural planets.

- **Helps / hurts:** For: planets cannot be exhausted for votes; each player casts 1 vote per agenda. Against: voters Against exhaust all their cultural planets next strategy phase.

- **Heuristic:** Votes become equal: good for a bot with below-average influence, bad for an influence-rich one. Against exhausts the voter's cultural planets (voter-conditional cost).

- **Faction logic:** Xxcha, Hacan vote Against.

#### `representative_government` - Representative Government (Base Game) (Law, For / Against, base)

- **Card:** For: Players cannot exhaust planets to cast votes during the agenda phase. Each player may cast 1 vote on each agenda instead. Against: At the start of the next strategy phase, each player that voted "Against" exhausts all of their cultural planets.

- **Helps / hurts:** Base-game wording of Representative Government (same effects).

- **Heuristic:** As rep_govt.

- **Faction logic:** As rep_govt.

#### `constitution` - New Constitution (Directive, For / Against, base)

- **Card:** (When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) For: Discard all laws from play. At the start of the next strategy phase, each player exhausts each planet in their home system. Against: No effect.

- **Helps / hurts:** (Skipped when no law is in play.) For: every law is discarded; next strategy phase every home-system planet is exhausted. Against: nothing.

- **Heuristic:** For: minus each law's ongoing value (a gain if laws hurt me) and 0.6 x (resources + influence) of my home planets for a round.

- **Faction logic:** Bots facing laws that hurt them vote For.


### G. Trade goods, action cards, technology

#### `economic_equality` - Economic Equality (Directive, For / Against, base)

- **Card:** For: Each player returns all of their trade goods to the supply. Then, each player gains 5 trade goods. Against: Each player returns all of their trade goods to the supply.

- **Helps / hurts:** For: everyone returns all trade goods and gains 5. Against: everyone returns all trade goods.

- **Heuristic:** Per seat: For = 5 - goods, Against = -goods. With 5 or more goods vote Against unless every rival holds more.

- **Faction logic:** Hacan (goods are votes) Against when holding goods.

#### `sanctions` - Executive Sanctions (Law, For / Against, base)

- **Card:** For: Each player can have a maximum of 3 action cards in their hand. Against: Each player discards 1 random action card from their hand.

- **Helps / hurts:** For: hand limit of 3 action cards. Against: everyone discards 1 random action card.

- **Heuristic:** For costs 1.5 per card above 3 (own hand known, rivals' counted); Against costs 1.5 for every non-empty hand. Cheaper for me relative to rivals.

- **Faction logic:** Yssaril (big hands) Against.

#### `unconventional` - Unconventional Measures (Directive, For / Against, base)

- **Card:** For: Each player that voted "For" draws 2 action cards. Against: Each player that voted "For" discards all of their action cards.

- **Helps / hurts:** For: voters For draw 2 action cards. Against: voters For discard all action cards.

- **Heuristic:** Voter-conditional (like Mutiny): For when it will pass (+3); if Against wins the loss is 1.5 per card held.

- **Faction logic:** Yssaril, Hacan value cards more.

#### `revolution` - Anti-Intellectual Revolution (Law, For / Against, base)

- **Card:** For: After a player researches a technology, that player must destroy 1 of their non-fighter ships. Against: At the start of the next strategy phase, each player chooses and exhausts 1 planet for each technology that they own.

- **Helps / hurts:** For: after a player researches, they destroy 1 non-fighter ship. Against: next strategy phase each player exhausts 1 planet per technology owned.

- **Heuristic:** For: minus the cheapest non-fighter ship value per remaining research (about 1 per round). Against: minus 0.6 x the cheapest min(techs, planets) planets. Compare per seat.

- **Faction logic:** Tech-rich seats (Jol-Nar, Nekro, Hacan, Winnu) lose most either way; ship-poor seats vote For.


### H. Movement and wormholes

#### `travel_ban` - Enforced Travel Ban (Law, For / Against, base)

- **Card:** For: Alpha and beta wormholes have no effect during movement. Against: Destroy each PDS in or adjacent to a system that contains a wormhole.

- **Helps / hurts:** For: wormholes have no effect during movement. Against: every PDS in or adjacent to a wormhole system is destroyed.

- **Heuristic:** For hurts wormhole-dependent seats (Creuss -10, others owning wormhole planets -3); Against costs 2 per PDS in or next to a wormhole system.

- **Faction logic:** Creuss, Cabal, Empyrean: Against. Xxcha, Argent (PDS): For.

#### `defense_act` - Homeland Defense Act (Law, For / Against, base)

- **Card:** For: Each player can have any number of PDS units on planets they control. Against: Each player destroys 1 of their PDS units.

- **Helps / hurts:** For: any number of PDS per planet. Against: everyone destroys 1 PDS.

- **Heuristic:** Against costs 2 for every seat with a PDS; For gives 0.3 per PDS owned. For when I own PDS, Against when rivals own more.

- **Faction logic:** Xxcha, Sol (PDS) For.

#### `wormhole_recon` - Wormhole Reconstruction (Law, For / Against, base)

- **Card:** For: All systems that contain either an alpha or beta wormhole are adjacent to each other. Against: Each player places a command token from their reinforcements in each system that contains a wormhole and 1 or more of their ships.

- **Helps / hurts:** For: all wormhole systems are adjacent to each other. Against: a token for each wormhole system with own ships.

- **Heuristic:** Against costs 1.5 per wormhole system with my ships; For helps wormhole seats (+3).

- **Faction logic:** Creuss, Cabal, Empyrean For.

#### `nexus` - Nexus Sovereignty (Law, For / Against, pok)

- **Card:** For: Alpha and beta wormholes in the wormhole nexus have no effect during movement. Against: Place a gamma wormhole token in the Mecatol Rex system.

- **Helps / hurts:** For: alpha and beta wormholes in the nexus have no effect. Against: a gamma wormhole is placed in Mecatol Rex's system.

- **Heuristic:** Tiny stake: Against exposes Mecatol's holder (-1.5); otherwise no opinion.

- **Faction logic:** Creuss/Cabal affected; negligible otherwise.

#### `wormhole_research` - Wormhole Research (Directive, For / Against, base)

- **Card:** For: Each player who has 1 or more ships in a system that contains a wormhole may research 1 technology. Then, destroy all ships in systems that contain an alpha or beta wormhole. Against: Each player that voted "Against" removes 1 command token from their command sheet and returns it to their reinforcements.

- **Helps / hurts:** For: each player with ships in a wormhole system may research 1 technology, then all ships in those systems are destroyed. Against: voters Against return 1 token from the command sheet.

- **Heuristic:** For: ships in wormhole systems die (minus their value, plus 5 for the technology); Against: -2.5 for each Against voter. For when none of my ships sits in a wormhole system, Against otherwise.

- **Faction logic:** Creuss, Cabal, Empyrean: Against.


### I. Rules that depend on how you voted

#### `conventions` - Conventions of War (Law, For / Against, base)

- **Card:** For: Players cannot use BOMBARDMENT against units that are on cultural planets. Against: Each player that voted "Against" discards all of their action cards.

- **Helps / hurts:** For: bombardment cannot target units on cultural planets. Against: each Against voter discards all action cards.

- **Heuristic:** Voter-conditional. For is safe for me; bombarders lose power. Vote For unless I bombard and hold at most 1 action card.

- **Faction logic:** Letnev, Muaat (bombardment) vote Against with few cards.


## 7. Potential gains

Method: `selfplay compare`, baseline = bot version 4 (all agenda knobs off), candidate = version 4 plus the knobs,
mixed mode (every table played twice with the candidate on alternate seats), fixed tables on `4pBgaBalancedD`
rotated cyclically (100 seeds x 4 rotations = 400 tables, seeds 200000.. and 210000..), and a 6-player mix
(`6pStandard`, `6pOuterRim`, 100 tables, seeds 220000..). Intervals are 95%, paired by table. Games, logs and scripts:
`/root/rulebot-results/agenda-voting/` (`run_ab6.sh`, `analyze.py`, `occurrence.py`, `agenda_vp.py`; the `r4_*` files are
the final round). The default bot, golden benchmark (`tools/rulebot/selfplay.sh bench`): 200 of 200 games identical
with the knobs off.

Arms: **all** = `agenda_value,agenda_budget,agenda_riders`; **value** = `agenda_value` (outcome valuation, every planet
cast as before); **budget** = `agenda_budget` (the old table picks the outcome, votes follow the budget);
**riders** = `agenda_riders`.

### Results

Candidate minus baseline. "ctrl" is the controllable VP (objectives, custodians, Imperial's Mecatol point; no agenda,
Support for the Throne, riders, relics). "agenda VP" and "Imperial rider VP" are the VP the seat scored from those sources.


#### tabA (sol/letnev/jolnar/mentak), tables 400

| arm | win share | VP per seat | total VP/round | ctrl VP/round | ctrl after R4 | ctrl after R5 | agenda VP | Imperial rider VP |
|---|---|---|---|---|---|---|---|---|
| all | +2.4 pp [-0.7, +5.5] | +0.133 [-0.003, +0.270] | +0.020 [-0.003, +0.043] | +0.004 [-0.015, +0.023] | +0.022 [-0.037, +0.081] | +0.049 [-0.036, +0.134] | +0.042 [+0.017, +0.068] | +0.027 [+0.013, +0.041] |
| value | +1.5 pp [-1.5, +4.5] | +0.055 [-0.073, +0.183] | +0.008 [-0.014, +0.030] | +0.001 [-0.018, +0.020] | +0.007 [-0.049, +0.064] | +0.010 [-0.073, +0.093] | +0.031 [+0.007, +0.055] | -0.006 [-0.016, +0.005] |
| budget | -0.5 pp [-3.5, +2.5] | -0.064 [-0.201, +0.073] | -0.013 [-0.036, +0.010] | +0.003 [-0.017, +0.023] | +0.037 [-0.025, +0.099] | +0.008 [-0.080, +0.096] | -0.113 [-0.150, -0.075] | +0.002 [-0.008, +0.012] |
| riders | +0.1 pp [-2.6, +2.9] | +0.035 [-0.088, +0.159] | +0.009 [-0.012, +0.029] | +0.004 [-0.013, +0.021] | +0.003 [-0.051, +0.057] | +0.061 [-0.016, +0.138] | -0.007 [-0.031, +0.016] | +0.031 [+0.018, +0.043] |

Per faction, arm all (win share, seat as candidate minus as baseline): jolnar +1.0 pp, letnev +1.0 pp, mentak +3.8 pp, sol +3.8 pp


#### tabB (muaat/sol/letnev/jolnar), tables 400

| arm | win share | VP per seat | total VP/round | ctrl VP/round | ctrl after R4 | ctrl after R5 | agenda VP | Imperial rider VP |
|---|---|---|---|---|---|---|---|---|
| all | +3.8 pp [+1.1, +6.5] | +0.156 [+0.036, +0.277] | +0.030 [+0.009, +0.050] | +0.019 [+0.001, +0.037] | -0.013 [-0.068, +0.041] | +0.030 [-0.048, +0.107] | +0.054 [+0.028, +0.081] | +0.039 [+0.027, +0.051] |
| value | +2.6 pp [-0.3, +5.5] | +0.098 [-0.028, +0.225] | +0.019 [-0.002, +0.040] | +0.014 [-0.005, +0.033] | +0.031 [-0.017, +0.080] | +0.012 [-0.070, +0.094] | +0.047 [+0.024, +0.070] | +0.001 [-0.010, +0.011] |
| budget | -0.4 pp [-3.2, +2.5] | -0.061 [-0.186, +0.065] | -0.008 [-0.029, +0.013] | -0.002 [-0.019, +0.016] | +0.016 [-0.037, +0.070] | +0.024 [-0.056, +0.104] | -0.065 [-0.104, -0.026] | +0.003 [-0.006, +0.013] |
| riders | -1.8 pp [-4.5, +0.9] | -0.075 [-0.192, +0.042] | -0.009 [-0.029, +0.011] | -0.013 [-0.030, +0.004] | -0.049 [-0.104, +0.005] | -0.069 [-0.145, +0.008] | -0.015 [-0.036, +0.006] | +0.027 [+0.015, +0.039] |

Per faction, arm all (win share, seat as candidate minus as baseline): jolnar +7.1 pp, letnev +1.5 pp, muaat +6.1 pp, sol +0.5 pp


#### mix6 (6p mixed (6pStandard, 6pOuterRim)), tables 100

| arm | win share | VP per seat | total VP/round | ctrl VP/round | ctrl after R4 | ctrl after R5 | agenda VP | Imperial rider VP |
|---|---|---|---|---|---|---|---|---|
| all | +0.5 pp [-4.3, +5.3] | +0.019 [-0.162, +0.200] | +0.011 [-0.017, +0.040] | +0.012 [-0.014, +0.038] | +0.017 [-0.053, +0.087] | -0.037 [-0.160, +0.085] | -0.002 [-0.037, +0.034] | +0.024 [+0.007, +0.041] |
| riders | +0.5 pp [-4.8, +5.9] | +0.120 [-0.055, +0.294] | +0.016 [-0.010, +0.043] | +0.011 [-0.014, +0.036] | +0.019 [-0.036, +0.074] | +0.012 [-0.100, +0.124] | -0.035 [-0.063, -0.007] | +0.024 [+0.009, +0.040] |

Per faction, arm all (win share, seat as candidate minus as baseline): arborec +0.0 pp, argent -13.0 pp, cabal +6.1 pp, empyrean -10.0 pp, ghost -13.8 pp, hacan +3.2 pp, jolnar +0.0 pp, l1z1x +7.7 pp, letnev +0.0 pp, mahact +16.7 pp, mentak +4.3 pp, muaat +7.7 pp, naalu -4.2 pp, naaz +0.0 pp, nekro +8.3 pp, nomad -18.2 pp, saar -14.3 pp, sardakk +8.3 pp, sol -4.2 pp, titans +4.0 pp, winnu +5.0 pp, xxcha +5.3 pp, yin +7.4 pp, yssaril +0.0 pp


### Reading the numbers

* **All three together** win +2.4 pp [-0.7, +5.5] on the Sol/Letnev/Jol-Nar/Mentak table and **+3.8 pp [+1.1, +6.5]** on
  the Muaat/Sol/Letnev/Jol-Nar table (Muaat +6.1 pp, Jol-Nar +7.1 pp); the pooled 800 tables give about +3.1 pp. VP per
  round +0.020 and +0.030 (the second significant). On six players: +0.5 pp [-4.3, +5.3], no effect. The controllable VP
  per round does not move (+0.004, +0.019): the gain is not extra objectives.
* Where the VP come from: agenda VP +0.042 and +0.054 per seat and game (significant), Imperial Rider VP +0.027 and +0.039
  (significant). Per agenda (Muaat table): Mutiny +0.024, Seed of an Empire +0.011, Political Censure +0.018 VP per seat.
* **Valuation alone** is positive but not significant on win share (+1.5 pp, +2.6 pp) and positive on agenda VP
  (+0.031 and +0.047, significant). It is the part that makes Muaat vote against the war sun schematics (Against in 43 of
  its 53 votes; the old table had no opinion) and the dreadnought and Mecatol agendas.
* **Budget alone** is not an improvement: -0.5 pp and -0.4 pp, agenda VP -0.113 and -0.065 (significant loss). The
  budget arm keeps the old table, which knows nothing about voter-conditional agendas (Mutiny, Unconventional
  Measures): there it saves votes it should have cast. With the valuation (the "all" arm) the loss disappears, so the
  budget should only be adopted together with the valuation. It spends 29% of the available influence (13% on the first
  agenda, 64% on the second) at a cost of 0.9 percentage points of win probability per decision (0.650 against 0.659
  with every planet).
* **Riders alone** do nothing measurable: +0.1 pp and -1.8 pp [-4.5, +0.9]; the total VP per round is -0.009 on both
  tables. The Imperial Rider adds +0.03 VP per seat as intended, but the costs (the seat's vote on that agenda, the other cards declined in
  the window) take it back. In the "all" arm the riders add +0.027 to +0.039 Imperial Rider VP on top of the valuation.
  Not an argument for riders in version 5 on its own.

Iterations (each was a full run; the earlier numbers are in the `r2_*`, `r3_*` and plain files): the first run gave
-0.045 agenda VP, because the forecast of the other bots' votes read the valuation of a bystander on Mutiny, which sees
only the rivals' gain. Forecasting voter-conditional agendas with the old table fixed it (+0.028 agenda VP). A second
fix made the rider logic mark its play at the right question, valued the Technology Rider at zero (this engine
records but does not perform its research) and removed the vote-saving bonus when the budget knob is off.

### How often each agenda occurs and how often the new vote changes the outcome

Muaat table, 800 games (one decision is one candidate seat voting on one agenda; "abstains" is no opinion or not worth the
votes; "opinion added": the old table had none; "flipped": a different outcome than the old table; "same": the old
table's outcome with the planned number of votes):

games 800, agendas revealed 6366 (7.96 per game)
| agenda | per 100 games | passed For / Against / elected / none | decisions | no opinion now | opinion added | flipped | same outcome | votes spent / available |
|---|---|---|---|---|---|---|---|---|
| `grant_reallocation` | 37.0 | 0 / 0 / 296 / 0 | 519 | 62% | 0% | 2% | 35% | 24% |
| `minister_policy` | 34.6 | 0 / 0 / 277 / 0 | 484 | 47% | 0% | 2% | 49% | 50% |
| `warrant` | 31.4 | 0 / 0 / 251 / 0 | 427 | 46% | 0% | 1% | 51% | 52% |
| `minister_war` | 30.8 | 0 / 0 / 246 / 0 | 446 | 52% | 0% | 2% | 44% | 43% |
| `arbiter` | 30.0 | 0 / 0 / 240 / 0 | 431 | 53% | 0% | 1% | 44% | 42% |
| `minister_sciences` | 29.2 | 0 / 0 / 234 / 0 | 412 | 57% | 0% | 2% | 40% | 34% |
| `censure` | 27.1 | 0 / 0 / 217 / 0 | 380 | 26% | 0% | 4% | 68% | 70% |
| `minister_antiquities` | 26.4 | 0 / 0 / 211 / 0 | 378 | 27% | 0% | 2% | 70% | 68% |
| `minister_peace` | 25.6 | 0 / 0 / 205 / 0 | 348 | 42% | 0% | 2% | 55% | 53% |
| `mutiny` | 25.1 | 179 / 22 / 0 / 0 | 320 | 2% | 0% | 6% | 90% | 16% |
| `incentive` | 23.4 | 185 / 2 / 0 / 0 | 307 | 100% | 0% | 0% | 0% | 0% |
| `minister_commerce` | 22.4 | 0 / 0 / 179 / 0 | 302 | 41% | 0% | 1% | 57% | 54% |
| `committee` | 22.2 | 0 / 0 / 178 / 0 | 317 | 45% | 0% | 2% | 52% | 52% |
| `minister_exploration` | 22.1 | 0 / 0 / 177 / 0 | 320 | 60% | 0% | 0% | 38% | 34% |
| `minister_industry` | 21.0 | 0 / 0 / 168 / 0 | 307 | 58% | 0% | 3% | 38% | 33% |
| `prophecy` | 19.9 | 0 / 0 / 159 / 0 | 280 | 97% | 0% | 0% | 2% | 0% |
| `secret` | 19.8 | 0 / 0 / 158 / 0 | 284 | 48% | 0% | 1% | 49% | 47% |
| `standardization` | 19.4 | 0 / 0 / 155 / 0 | 260 | 66% | 0% | 6% | 26% | 28% |
| `schematics` | 17.9 | 101 / 42 / 0 / 0 | 219 | 34% | 65% | 0% | 0% | 35% |
| `redistribution` | 17.8 | 0 / 0 / 142 / 0 | 256 | 91% | 0% | 7% | 1% | 5% |
| `unconventional` | 17.4 | 114 / 25 / 0 / 0 | 223 | 4% | 0% | 12% | 82% | 17% |
| `seed_empire` | 15.2 | 47 / 75 / 0 / 0 | 198 | 36% | 0% | 0% | 62% | 35% |
| `revolution` | 14.9 | 61 / 58 / 0 / 0 | 185 | 42% | 57% | 0% | 0% | 35% |
| `execution` | 14.9 | 0 / 0 / 119 / 0 | 178 | 64% | 0% | 12% | 23% | 25% |
| `economic_equality` | 14.4 | 104 / 11 / 0 / 0 | 197 | 82% | 0% | 2% | 15% | 4% |
| `conventions` | 14.0 | 41 / 71 / 0 / 0 | 202 | 95% | 4% | 0% | 0% | 2% |
| `wormhole_research` | 12.1 | 19 / 78 / 0 / 0 | 184 | 100% | 0% | 0% | 0% | 0% |
| `wormhole_recon` | 12.0 | 28 / 68 / 0 / 0 | 181 | 100% | 0% | 0% | 0% | 0% |
| `rearmament` | 11.9 | 65 / 30 / 0 / 0 | 160 | 44% | 55% | 0% | 0% | 32% |
| `artifact` | 11.9 | 59 / 36 / 0 / 0 | 159 | 47% | 52% | 0% | 0% | 30% |
| `articles_war` | 11.6 | 74 / 19 / 0 / 0 | 133 | 49% | 0% | 17% | 33% | 17% |
| `disarmament` | 11.0 | 0 / 0 / 88 / 0 | 163 | 87% | 0% | 6% | 6% | 8% |
| `arms_reduction` | 10.9 | 48 / 39 / 0 / 0 | 144 | 30% | 69% | 0% | 0% | 39% |
| `travel_ban` | 10.5 | 22 / 62 / 0 / 0 | 155 | 100% | 0% | 0% | 0% | 0% |
| `plowshares` | 10.4 | 30 / 53 / 0 / 0 | 130 | 49% | 50% | 0% | 0% | 22% |
| `miscount` | 9.0 | 0 / 0 / 72 / 0 | 132 | 87% | 12% | 0% | 0% | 6% |
| `conscription` | 9.0 | 28 / 44 / 0 / 0 | 124 | 66% | 33% | 0% | 0% | 24% |
| `checks` | 8.8 | 37 / 33 / 0 / 0 | 123 | 50% | 49% | 0% | 0% | 23% |
| `defense_act` | 8.8 | 43 / 27 / 0 / 0 | 119 | 70% | 29% | 0% | 0% | 16% |
| `sanctions` | 8.4 | 46 / 21 / 0 / 0 | 110 | 57% | 42% | 0% | 0% | 26% |
| `covert` | 8.2 | 18 / 48 / 0 / 0 | 121 | 100% | 0% | 0% | 0% | 0% |
| `rep_govt` | 8.2 | 38 / 28 / 0 / 0 | 120 | 63% | 36% | 0% | 0% | 15% |
| `constitution` | 8.2 | 32 / 34 / 0 / 0 | 109 | 46% | 53% | 0% | 0% | 27% |
| `shared_research` | 7.1 | 29 / 28 / 0 / 0 | 102 | 100% | 0% | 0% | 0% | 0% |
| `abolishment` | 6.5 | 0 / 0 / 52 / 0 | 89 | 79% | 20% | 0% | 0% | 12% |
| `cladenstine` | 6.4 | 25 / 26 / 0 / 0 | 94 | 100% | 0% | 0% | 0% | 0% |
| `nexus` | 6.1 | 32 / 17 / 0 / 0 | 76 | 69% | 30% | 0% | 0% | 15% |
| `regulations` | 5.0 | 25 / 15 / 0 / 0 | 70 | 91% | 8% | 0% | 0% | 4% |

decisions 10978: flipped to another outcome than the old table 2.5%, abstains 57.6%, old table had no opinion but the new has 8.6%, same outcome 31.4%
same outcome as old table: P(wins) with the planned votes 0.650 vs 0.659 with all planets (n=3442)

Over all decisions the outcome differs from the old table in about 2.5% (mostly Mutiny, Unconventional Measures, Articles of
War, Execution and the dreadnought and sanctions agendas), the new bot has an opinion where the old one had none in
8.6%, and it abstains in 58%. The outcome of an agenda itself is changed less often than the decisions: the abstentions
are mostly elections the seat could not win (the richest seat takes a Minister anyway). Agendas that the table
leaves neutral (Incentive Program, Travel Ban, Clandestine Operations, Schematics without a war sun owner) stay
without a vote.

### Knobs (all off in bots 1 to 4, all in `tuning.rs`)

| knob | default | meaning |
|---|---|---|
| `agenda_value` | false | outcome valuation for all 63 agendas, voter-conditional agendas, Politics placement and tie-breaks |
| `agenda_budget` | false | votes cast only to the extent that they pay, planets chosen to waste least influence, second agenda kept in mind |
| `agenda_riders` | false | rider cards played only when predictable and the seat's votes are not needed; other cards in that window declined |
| `agenda_rival_weight` | 0.5 | weight of the rivals' change in an outcome's value |
| `agenda_min_margin` | 1.5 | value points the best outcome must lead by, else no opinion |
| `agenda_rival_share` | 0.6 | reserved: chance a rival without opinion votes against the seat |
| `agenda_second_prior` | 2.0 | assumed stake of an unknown second agenda |
| `rider_min_p` | 0.7 | least probability of the predicted outcome for a rider |

### Decision for a version 5

Felix decided to enable all three knobs (`agenda_value`, `agenda_budget`, `agenda_riders`) in the v5 staging table
(`tuning::v5()`, selectable as `bot_version=5`). They stay off in bots 1 to 4 and `BOT_VERSION` stays 4. The
recommendation below, written before that decision, advised leaving `agenda_riders` off; the risk it names is accepted.

### Recommendation for a version 5 (earlier analysis)

Adopt `agenda_value` together with `agenda_budget`: win share +3.8 pp on the Muaat table and +2.4 pp on the other,
agenda VP significantly positive in both, no effect on six players, no loss anywhere, and a cheap decision (the cost per
game is a few valuation calls in the agenda phase). Do not adopt `agenda_budget` alone, and leave `agenda_riders` off:
alone it does not pay, and in the combination it adds Imperial Rider VP but also a risk the A/B runs cannot rule out.
A version 5 run of the usual gates (golden benches of versions 1 to 4, `completion`, `stress`) is still due; version 4
games are bit-identical with the knobs off (golden benchmark 200 of 200).

### Known limits

* The second agenda is only known when the seat placed it itself with Politics in the same round (the observation shows
  no queue); otherwise the stake of the second agenda is the prior. Naalu's commander look shows the card behind the
  second agenda, so it is not used.
* Rider forecasts assume the others vote as the valuation or the old table says; the correctness of predictions is
  not logged per game (the Imperial Rider VP is the proxy).
* Rivals' unknown action cards, secret objectives and the exact influence of attached planets are not modelled; planet
  influence is the printed value.
* `classified` and `crisis` never reach a vote in this engine; `covert` is an empty vote.
* The experiment log (`AGENDA_VOTE_LOG`) and the `agendas` key in the stored game timeline (`timeline.rs`) are the only
  changes outside the bot; they affect no play.
