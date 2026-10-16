### Bugs and rules

- [ ] Replace broken initial map generation with balanced predefined maps; fix system rings and the mix of planet and empty systems.
- [ ] Fix the bot crash when resolving Diplomacy.
- [ ] Fix Hacan trading.
- [ ] Fix picking up units along the movement route.
- [ ] Fix faction technology displays: players currently see Player 1’s faction technologies.
- [ ] Fully debug payment calculations and resolution, including command-token purchases, planet exhaustion, trade goods, and production requiring repeated payments.
- [ ] Fix automatic payment exhausting planets unnecessarily when trade goods are available.
- [ ] Fix resource-spending validation for Erect a Monument, including potentially stale eligibility.
- [ ] Investigate Blitz being offered during an opponent’s turn when it would have no useful effect.
- [ ] Restore the combat simulator.
- [ ] Fix UI flashing and duplicate event-log keys, including round:2:action:action:action_295.
- [ ] Update strategy cards to the latest expansion and official patch/errata.

### Decisions and turn flow

- [ ] Allow players to undo their own reversible actions during their turn, provided no randomness is involved.
- [ ] Let players prepare strategy-card decisions simultaneously, then confirm or edit them when their turn arrives; resolve in the correct order.
- [ ] Consolidate Trade strategy-card decisions into one workflow.
- [ ] Show a summary of the active player’s current decision.
- [ ] When waiting, show which player and decision are blocking progress.
- [ ] Add an optional end-of-turn summary, configurable per player.
- [ ] Play a turn-notification sound, with a per-player mute setting.
- [ ] Audit single-option decisions, document them in a file, and choose which should resolve automatically; include forced strategy-card picks and cases with no available payment.
- [ ] In the action menu, name the remaining strategic action when only one is available.
- [ ] Replace generic “Decision resolved” messages with meaningful summaries, including command-token purchases.

### Map and player interface

- [ ] Preserve the selected map view throughout decisions, including invasions.
- [ ] Use map-based planet selection wherever applicable, including exhaustion, Construction, the Xxcha agent, and agenda decisions.
- [ ] During Warfare, select the command token to recall by clicking its system on the map.
- [ ] Use consistent resource and influence icons throughout the UI.
- [ ] Enlarge player-specific map markers, such as hexagons, squares, and circles.
- [ ] Place the local player first and/or compact player cards to reduce scrolling.
- [ ] Always show player names alongside faction names.
- [ ] Add a faction reference panel and tooltips covering faction abilities, faction-specific units, mech abilities, and faction promissory notes.
- [ ] Show leaders and their locked, unlocked, exhausted, or purged status.
- [ ] Show owned relics and their exhaustion status.
- [ ] Show relic-fragment counts by type.
- [ ] Show privately held promissory notes and public face-up notes, including who holds whose Support for the Throne or Alliance.
- [ ] Show captured units and which are available to consume.
- [ ] Show dynamic map tokens and state: frontier tokens, Ion Storm, Creuss wormholes, and the Custodians token.
- [ ] Add command-token tooltips identifying the tactical, fleet, and strategy pools.
- [ ] Grey out used strategy cards.
- [ ] Show unclaimed strategy cards and their accumulated trade goods.
- [ ] Fix event-log scrolling when new events arrive.

### Combat and production

- [ ] Avoid showing noncombat invasions to uninvolved players.
- [ ] Show planet resources during invasion.
- [ ] Preselect an editable ground-force distribution across invasion planets, favoring planets with exploration events and/or higher resources.
- [ ] Show post-combat results, including a ground-combat summary popup.
- [ ] Add production tooltips with unit statistics and abilities, including movement, capacity, flagship abilities, and bombardment.
- [ ] Show fleet supply during production.
- [ ] Show remaining unit supply when building units or planning fleets.
- [ ] Make production payment options and sequencing clear, including how to avoid wasting planet resources.
- [ ] Add a command-token allocation/redistribution UI with pip bars, plus/minus controls, and an unassigned-token counter; enforce legal limits and require complete allocation before continuing.

### Cards, trading, agendas, and scoring

- [ ] Show timing/trigger text on every action card.
- [ ] Show the played card and its full text in reaction windows, including the card targeted by Sabotage.
- [ ] Highlight action-card names in the event log and show their text on hover.
- [ ] Allow players to mute action-card reactions, automatically passing the muted reactions.
- [ ] Allow players to select any action card for bluffing; make the server/engine offer timing windows as though they held that card.
- [ ] Fix duplicated “Play play action card” wording and rename “Pin” to explain that it pauses automatic passing.
- [ ] Simplify the exchange UI and show the effects of cards offered in a trade.
- [ ] During Politics, show agenda-card text when choosing top or bottom of the deck.
- [ ] Improve the agenda-phase presentation: show the current agenda, voting results, and resulting effects.
- [ ] Show active laws and their effects.
- [ ] Show a clear breakdown of victory-point sources, including objectives, Mecatol Rex, and Shard of the Throne.

### Verification

- [ ] Test every strategy card’s primary and secondary abilities.
- [ ] Test the complete agenda phase.
- [ ] Test winning the game and end-game resolution.