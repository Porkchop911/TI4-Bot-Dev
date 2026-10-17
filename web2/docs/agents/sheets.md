# web2: reference sheets

Part of `web2/AGENTS.md`: the Patterns and the rules of Screen size and Text there apply here too.

## Reference sheets

General points from the Objectives sheet (owner, 2026-10-11). Use them for the next sheet (agendas, technologies, relics).

- **A sheet answers one question, and its section here says which in its first line.** Ask the owner for the question before the layout. Two sheets do not answer the same question: the Objectives sheet says what points can still be scored or lost, the sheet of a player says why the seat has its points.
- **A sheet about what comes does not show what is done.** A thing that every seat has, that is used up, or that can no longer come is not listed. It is in the sheet that is about what was.
- **A thing is on a sheet once.** A list of a deck does not repeat what the sheet shows above it.
- **A deck is a list in groups by meaning**, not by a printed field alone. The groups are written by hand in `ti4-content`, beside the corpus (`secret_objectives.rs`, `public_objectives.rs`), with a test that every card has one. A group is closed until the player opens it, and its heading has a count.
- **Printed text is in `ti4-content`.** Look there before you say that a text is missing; a fact of `ti4-host` (`src/view/`) or the catalogue brings it to the page. A card of a sheet shows its text. A line of a list has the text behind the name (`Peek`, with the dotted underline).
- **Seats.** The viewer is first, then the other seats by their points. A seat on a card is a tag: the symbol and the faction. Eight seats are one row of a phone as symbol, number and faction in two lines. When a sheet can be seen for one seat, the row of the seats is the filter.
- **A phone sheet has the whole pane**: no footer and no tab bar. It does not scroll to the side. A heading that stays in view has two lines at most; a caption goes into a line that is there already.
- **Which rows a sheet shows is a flag of the view model** (`ScoreRowView.open`), set in `session/select`. The component does not decide it from names.
- **How to work on a new sheet.** The owner looks at it on the phone, in a late-game scenario, after each step. The owner may ask for typechecks only while the layout changes; the tests are brought up to date with each step and are run before the commit.

## Action cards

The owner asked for this on 2026-10-11: the Cards sheet has the hand of the viewer and the whole deck.

- "Your action cards" has the count against the hand limit ("Hand 5 / 7"; over the limit it is red with "⚠") and one card for each printed name, in the frame of the cards of a player sheet: the name, "×2" for copies, when it is played ("ACTION:", a timing) and the text.
- A card that a window offers has its setting at the right of the name (the Reaction windows section of `decisions.md`): "Offered", "Only on my turns" or "Never". A card with "ACTION:" has none: no window offers it.
- "Action card deck" says how many cards are left and how many are discarded, then the cards in groups by when a card is played ("Action", "Your activation", "Combat", "Agenda phase"), each with the number of its names. A group is closed until the player opens it (owner, 2026-10-11), with "▸" and "▾" as the rounds of the log; there are too many cards for one list. The groups and their order come from the catalogue (`action_card_groups`, written by hand in `ti4-content`); the hand has the same order, with no headings. A group has one row for all cards of a name, as the exploration decks: "3 of 4 unseen" (not discarded and not in the hand of the viewer, so in the deck or in the hand of another player), "1 in your hand · 1 discarded", and the printed text. A card with none unseen is dimmed; "0 of 1 unseen" is its sign, with no "–" before it (owner, 2026-10-11).
- Exception to "rule text is behind hover": the sheet shows the printed text of each card, as the deck sheet of the exploration does.
- "Fake" is on the row of a card that the viewer does not hold and that can still be in a hand. A card with "ACTION:" has none (owner, 2026-10-11): no window offers it, so there is nothing to fake. A faked card is shown after the hand, with a dashed frame and the word "Faked", and "Stop faking". It is a setting of the page and is sent nowhere yet: later it gives the viewer the windows of that card.
- The facts come from the engine: the hand (`held_action_cards`), the deck count, the discard pile and the hand limit (`action_cards` of the update). The printed cards and the deck that a game begins with are the catalogue (`components.action_cards`). Copies of a card have ids of their own; the sheet counts by printed name.
- How the setting of a card is applied and kept is in the Reaction windows section of `decisions.md`.

## Secret objectives

The owner asked for this on 2026-10-11: the Secrets sheet has the secret objectives of the viewer and the whole deck. It is a reference sheet of its own, after Cards.

- "Your secret objectives" has "1 scored · 3 / 3" (scored and unscored together against the limit) and one card for each objective, in the frame of the cards of a player sheet: the name, the phase in which it is scored and the text. The state is at the right of the name: "✓ Scored" or "Not scored". The hand is first, then the scored objectives.
- "Secret objective deck" says how many objectives are unseen and how many cards the deck holds ("31 of 40 unseen · 22 in the deck"), then the objectives in groups by what an objective asks for ("Planets you control", "Ships on the board", "Win a combat"). A group is closed until the player opens it, as a group of the action card deck; its heading says how many of it are unseen ("4 of 6 unseen"). The groups and their order come from the catalogue (`secret_objective_groups`, written by hand in `ti4-content`).
- Every objective is in the deck once (owner, 2026-10-11), so a row has a word and no count: "Unseen" (in the deck or in the hand of another player), "In your hand", "✓ Scored by you", or "✓ Scored by" with the seat. A hand that the game reveals (Search Warrant) is "In the hand of" with the seat. A row that is not unseen is dimmed. The row has the phase and the printed text.
- Exception to "rule text is behind hover": the sheet shows the printed text of each objective, as the Cards sheet does.
- The facts come from the engine: the hand (`held_secret_objectives`), and the deck count, what each seat has scored and the limit (`secret_objectives` of the update). The printed objectives and the deck that a game begins with are the catalogue (`components.secret_objectives`). The "Secrets" column of the player table counts the scored objectives from the same fact.
- Not there yet: how far a seat is with an objective, and the sheet for another seat sorted by that.

## Agendas sheet

The sheet answers: which laws hold now, and what can an agenda phase still bring? The owner asked for it on 2026-10-11. It is a reference sheet of its own, after Secrets, and every viewer has it.

- "Laws in play" has the count ("2 laws") and one card for each law, by name, in the frame of the cards of the other sheets: the name, "Law" with what the vote is about ("Law · Elect Player"), and the text of the outcome that passed only ("For: …" or "Against: …"). What passed is at the right of the name: the elected seat as a tag, or a word ("For", "Against", the name of what was elected). With no law it says "None".
- The agenda under vote is not on the sheet: it is in the action panel (owner, 2026-10-11).
- "Agenda deck" says how many agendas are not in play and how many cards the deck holds ("48 not in play · 47 in the deck"), then the agendas in groups by what an agenda affects ("Victory points and objectives", "Ministers and offices", "Planets", "Units and combat", "Map and movement", "Technology", "Command tokens, trade goods and cards", "Laws and votes"). A group is closed until the player opens it, and its heading has the number of its agendas. The groups and their order come from the catalogue (`agenda_groups`, written by hand in `ti4-content/src/agendas.rs`).
- Every agenda is in the deck once, so a row has no count. A row has the name, "Law" or "Directive" with what the vote is about, and the printed text of both outcomes, one line each. A law in play is not listed again.
- A row has no state (owner, 2026-10-11): the engine keeps no discard pile for agendas, so an agenda that was voted on and is gone stays in the list. The two numbers of the heading say how many are gone.
- Exception to "rule text is behind hover": the sheet shows the printed text of each agenda, as the Secrets sheet does.
- The facts come from the engine: the laws with their outcome (`table.laws` of the view) and the deck count (`agendas` of the update). The printed agendas and the deck that a game begins with are the catalogue (`components.agendas`). A note that a card prints with what is elected ("When this agenda is revealed, …") is with its text.
- Not there yet: which agendas are discarded; a law that is blank for a turn (Keleres) is shown as in play.

## Objectives sheet

The owner asked for this on 2026-10-11. The two sheets have two goals. The Objectives sheet is about what comes: the points that a seat can still score or lose. The sheet of a player is about what was: every point of the seat with its reason (the Player sheet section).

- The sheet has the sources that are not done: a public objective that not every seat scored, the custodians while no seat has them, Support for the Throne, and every agenda that a seat has or lost a point from (a law can be repealed; the owner asked for all agendas on 2026-10-11). What is done is not there: an objective that every seat scored, the secret objectives, Imperial, a relic. A stage whose objectives are all done says "All scored by every seat".
- Every source is a card, in the frame of the cards of the other sheets: the name, what an objective is worth ("1 VP"), its printed text, and a tag for each seat that has something from it. The owner chose this on 2026-10-11. Tried before on that day: one cell for each seat under each name (the heading was too high on a phone), and a table that scrolls to the side.
- A tag is the symbol and the faction of a seat. For a source that is not an objective it has the points after the faction ("Sol 2", "Xxcha −1"), and for Support for the Throne "←" and the symbol of the seat whose note it is. A card that no seat has anything from says "Nobody yet".
- The standings are on top and stay in view while the sheet scrolls: one column for each seat, with the symbol, the points and the faction. The viewer is first, then the other seats by their points, the most first. The first group line says the points that win ("10 VP to win").
- A seat of the standings is the filter (a control that only this sheet has: the seats are its heading, and the filter is where the seat is). A tap chooses the seat, a second tap shows all again. With a seat chosen the sheet shows what that seat can still score: the public objectives that it has not scored, with the count ("3 of 5 open for Hacan", "All scored for Hacan"), and of the other sources the ones that it has points from (what it can lose) or that no seat has. The tags of the other seats stay. The filter is not kept.
- The groups are "Public objectives · stage I", "Public objectives · stage II" and "Other points". An objective says what it is worth at the right of its name ("1 VP"). The objectives are in the order of the reveal.
- The other sources of the game are rows of the same facts, with a name of their own: "Secret objectives", "Custodians of Mecatol Rex", "Imperial · Mecatol Rex", "Support for the Throne", and a row for an agenda, a relic, an action card or a planet that a seat has a point from ("Agenda · Mutiny", "Relic · Shard of the Throne"). Points that no source explains are a row "Other". A row says whether it is still open (`ScoreRowView.open`); the Objectives sheet shows the open rows, the sheet of a player all rows of its seat.
- "Public objective deck" is under the sources: the public objectives that the game can still reveal ("18 not revealed"), in groups by what an objective asks for ("Spend", "Planets you control", "Units on the board", "Structures", "Technology"). A group is closed until the player opens it, as a group of the secret objective deck, and its heading has the number of its objectives. A row has the name, what it is worth ("2 VP") and the printed text. An objective that is revealed is not listed again: it is a card above. Once an objective of stage II is revealed, the objectives of stage I are not listed (owner, 2026-10-11). A game draws five objectives of each stage and no fact says which, so the list is everything that can come. The objectives and the groups come from the catalogue (`components.public_objectives`, `public_objective_groups`, written by hand in `ti4-content`); the demo has none.
- Exception to "rule text is behind hover": the sheet shows the printed text of each public objective, agenda and deck row, as the Secrets sheet does.
- The facts come from the engine (`score` of the update, `ti4-host/src/view/score.rs`): the objectives as printed with who scored each, and the other sources, with the printed text of an agenda, a relic, a card or a planet, from the ledger of the engine, which has a row for every point that is given or taken (`vp_ledger`). web2 only words the rows.
- Not there yet: a point that a seat can lose from a relic (Shard of the Throne) is not in the Objectives sheet, because the engine does not say which points can be lost; how far a seat is with a public objective.

## Player sheet

The owner asked for this on 2026-10-11: the sheet of a player shows what that player can use and what is used.

- "Victory points" is on top, with the points against the points that win ("5 of 10"): one line for every source that the seat has something from, with the points at the right ("Expand Borders 1", "Imperial · Mecatol Rex 2", "Agenda · Mutiny −1"). The lines sum to the points of the seat. The printed text of an objective, an agenda, a relic, an action card or a planet is behind its name (`Peek`, as the values of a unit in the production). The custodians, Imperial and Support for the Throne have none: every player knows them. "Secret objectives" names the scored objectives under it, each with its text behind the name; Support for the Throne has "←" and the symbol of the seat whose note it is. The facts are those of the Objectives sheet.
- The sheet has three groups of cards, as printed: "Faction abilities", "Leaders" (agent, commander, hero) and "Action components". A card has its name, its kind, when it is used ("ACTION:", a timing) and its text.
- "Action components" are the cards that a player uses: a technology with an ACTION or one that exhausts, the relics, an exploration card that stays in the play area, and the ability card of a legendary planet. A technology that always holds is not a card. Every technology stays in the list "Technologies" of the sheet.
- The state of a card is one word at its right edge: "Ready", "Exhausted", "Locked", "Purged". A card that cannot be used now is dimmed. A locked leader says how it is unlocked ("Unlock: Have 3 scored objectives."). A card with no state (a faction ability, a relic that always holds) has no word.
- Exception to "rule text is behind hover": the sheet shows the printed text of each card, as the deck sheet does. The reason: the player opens the sheet to read what a seat can do.
- The hover sheet of the player table has the names and the states only, with no text.
- The sheet has public information only. The action cards of the viewer are in the Cards sheet, the secret objectives in the Secrets sheet.
- The facts come from the engine (`leaders`, `exhausted_technologies`, `exhausted_relics`, `exhausted_legendary` of the player). The names, the texts and which cards are used are the catalogue (`Transport.catalog`, `components`); without it a card is shown by its id.
- A faction ability has no state yet: the engine does not say when one was used.
