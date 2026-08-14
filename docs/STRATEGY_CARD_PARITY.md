# Strategy-card parity

The driven Rust game now resolves strategy-card effects. A strategic action applies its primary,
opens the clockwise follower window, charges the shared secondary cost when applicable, and applies
the selected secondary. Every card choice uses `Table::ask_seeing`, so learned policies receive the
public board observation rather than the blind fallback.

## Implemented effects

| card | primary | secondary | special handling |
|---|---|---|---|
| Leadership | allocate 3 tokens, then buy tokens for 3 influence each | influence purchases only | no strategy-token cost |
| Diplomacy | lock a controlled non-Mecatol system; ready up to 2 planets | ready up to 2 planets | followers pay normally |
| Politics | choose a new speaker, draw 2 action cards, place each of the top 2 agendas on top/bottom | draw 2 action cards | agenda decisions match the Python sequence |
| Construction | place a dock/PDS, then a PDS | place a dock/PDS | structure ids are `unit\|system\|planet` as in Python |
| Trade | gain 3 goods, replenish, grant any number of free replenishments | replenish | Hacan's Masters of Trade waives the token |
| Warfare | recall a board token, then allocate 1 token | produce at the home system | no gain is awarded if no token can be recalled |
| Technology | research 1 free; optionally pay 6 for another | pay 4 and research 1 | Jol-Nar's Brilliant substitutes the primary |
| Imperial | optionally score a public; gain 1 VP for Mecatol or draw a secret | draw a secret | victory is checked immediately |

Thunder's Edge ids are dispatched before printed names. `te4construction` resolves its
structure-or-production choice followed by a structure. `te6warfare` opens a free tactical action
without spending or placing a token, allows a system that already contains the player's token, and
defers its follower window until that tactical action finishes.

The printed TE Warfare card also permits command-token redistribution before and after the free
action. The Python reference implements neither redistribution, and Rust currently matches that
measured environment. This is an explicit rules-coverage gap; adding it to Rust alone would make
solved-checkpoint comparison less faithful.

## Driver order

1. `Game::apply_choice` opens the structural strategic action and applies the primary.
2. Ordinary cards expose followers clockwise. TE Warfare first completes its free tactical action.
3. The follower window handles Leadership's no-token rule and faction waivers.
4. A follower that accepts resolves the secondary; Brilliant resolves Technology's primary.
5. Faction strategy hooks run for the primary player and each follower, then the card exhausts.

Nested card decisions currently run synchronously inside the strategic-action or follower step,
but each is independently observed and recorded in the table decision log. This matches driven
production and is usable by policy-gradient collection. Converting every nested effect into a
separate `Game::step` window remains an architectural improvement, not a missing card effect.

## Verification

Focused tests cover all eight base primaries, the simple secondaries, Warfare home production,
the shared follower cost rules, and both Thunder's Edge replacements. The Stage-1 solved-profile
comparison must still be rerun before any learning-parity claim; map geometry remains a separate
known blocker.
