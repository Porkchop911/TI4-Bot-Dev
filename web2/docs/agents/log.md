# web2: the Log

Part of `web2/AGENTS.md`: the Patterns and the rules of Screen size and Text there apply here too.

## What it is for

- The Log answers one question: what are the others doing, and what did they do, so that I can decide. The decision of the viewer is the step sheet on the map (or a seat in the player table); the Log only reads (owner, 2026-10-16).
- Desktop has it in the right column under the player table, where it fills the column and scrolls inside itself. A phone has it as the pane Log. Both show the same Log (`src/game/log/LogPanel.tsx`).
- The Log has two parts, in this order: the turns since the viewer's last turn (the history), then the action that is played now, last and open.

## The current action

- The current action is what the action panel shows (`ShellView.action`), roughly the same content, read-only: the turn of another seat ("Hacan’s turn", "Turn ended"), a wait, the progress of a strategic action (the list of its seats and "Then: …"), a tactical action with its steps (the resolved steps, the battle tables, the odds), and the band of a window. It is always there; the history may be empty.
- Its heading is one line: the heading and its context ("Leadership · Strategic action · You"; a tactical action has its context as the heading, "Tactical action · Nebula · #42 · Sol"), the pill of its state (less what the status of the toolbar says, `pillBeyond`), and "?" with the rules. "Not sent" and the footer are not in the Log: they are of the decision.
- Its parts are groups that open and close with "▸" and "▾", as the groups of the reference sheets (`GroupHeading` in `src/ui`). A block of one line (a note, the source card) is no group. The name of a group is the name of its block ("Seats", the title of a card); the block under it does not say it again.
- What is open at first: the part in progress is open. In a strategic or component action every part is open except a list of choices and the technology tree, which are the decision of a seat and long. In a tactical action every step is a line with its sign (✓ done, – skipped, ▶ in progress, ✎ drafted, ! needs review, · not reached) and its caption; the step in view is a group that is open, with its content. The view has only the content of the step in view, so the other steps have no content in the Log yet.
- A toggle of the viewer is kept while the action stays the same; another action starts with its defaults.
- While the viewer decides, the Log shows the same action as the step sheet. Later the blocks that only inform (the rows of the other seats, the results of earlier steps) leave the step sheet for the Log, by a flag of the view model.

## The rows

- The history is a list of rows, oldest first, above the current action (`src/game/log/LogHistory.tsx`). It is the overview: what every seat did, in one line each, so that the player reads the round at a glance and opens only what matters.
- A turn is one dense row: "▸", its number in the round (the round is on the divider of its phase and in the tooltip), the seat (its symbol, the name of the player and the faction in a column of fixed width, cut with "…", the full name on hover; pattern 13), "You" on a turn of the viewer, the action ("Tactical · Mecatol Rex · #18", "Strategic · Warfare", "Component · Sabotage", "Pass") and the summary, muted and cut with "…". The turn that is played now has "▶" before the seat and a light tint. A phone has the summary in a second line under the seat and the action.
- A phase is a band: "Strategy phase · Round 3" with its summary ("7 strategy cards picked"). A band with details opens as a row does; the start of the action phase has none and is a divider.
- A row with details is a toggle, "▸" closed and "▾" open, as the groups of the sheets. Every row is closed at first. A pass has nothing to open and no "▸". What the viewer opened stays open while its entry is in the Log; an entry that leaves the Log (a new turn of the viewer) takes it with it. It is not kept over a reload.
- An open row shows its groups (Activation, Movement, Space combat, Invasion, Production; Primary, Secondaries; Picks, Scored, Command tokens, the votes of each agenda), each a muted title and its lines, dense. A line has the symbol of its seat first. A line does not repeat the title of its group: "Space combat · round 2" in Space combat is "Round 2".
- The dice of a roll follow its line: each face in its frame (the `Die` of the battle table), a face that hits bold and in the frame of a hit (weight and frame, not colour only; pattern 4), then "hits on 9 · 1 hit" muted. A face hits when it is at least the value; the count of hits is the host's.
- The Log scrolls inside itself, on desktop in the column under the player table and on a phone as the pane Log. The newest is at the end: the Log opens at its end, and follows what is new while it is there (within 24px of the end); scrolled up, it stays where the viewer is. A row that the viewer opens does not move the Log.
- Nothing in a row is a control but its toggle. There is no filter of the seats: a row says its seat, and the Log is short (since the viewer's last turn).

## Read-only

- Nothing in the Log is a control (patterns 10 and 11): the Log is the same content with nothing open. Only what changes the view stays: a group that opens, a rule text behind "?", the rolls of a battle.
- This is done once, at the root of the Log, not per block: `ReadOnly` (`src/game/context.tsx`) puts the content in a `ReadOnlyScope` (`src/ui/readOnly.tsx`) and gives it a dispatch that does nothing. Inside it a `Button` is not there (an `ActionButton` that is on is its state, "✓ …"), a `Counter` is its count, a `Press` (a row, a token, a cell, the whole control that is its content) is its content with its state, a shortcut key (`Kbd`) is not shown, the setting of a reaction is its word, and the search of a system is not there.
- A new control in a block uses these primitives (`Button`, `Counter`, `Press`), never a bare `<button>` that dispatches, so that it is read-only in the Log by itself.

## The history

- The history is made from the public events of the game (`GameEvent`), which the transport keeps next to the rolls: the events of a snapshot replace the list, since an undo takes the later ones back, and each `event` message adds one, once by its `id` (`src/session/events.ts`). Both kinds of game get them the same way: the engine of a local game sends the event messages and the snapshot with its events as the server does. An event of a seat or of the referee is not kept. `selectLog` (`src/session/select/log.ts`) makes `ShellView.log` (`LogView`, `src/model/log.ts`) from them. `LogPanel` shows it above the current action (`LogHistory`, the section "The rows").
- Only the typed facts of an event are read (`fact`, `roll`), never its `detail` text. An event without a fact adds nothing, and a fact that the Log does not know yet (a newer host) is skipped. Every id is named as the rest of the page names it: a seat by `seatLabel`, a system as "Mecatol Rex · #18", a planet by the board, a unit by its card ("Carrier I"), a technology, a strategy card, an objective, an action card, a leader, a relic, an ability by the catalogue. No raw id is on the screen; without the catalogue a name is the id in words.

### The turn model

- A turn is the events of one `action_id`. It starts at the fact `action_started` (its action: tactical, strategic with its card, component with its source, pass) and ends at `turn_ended`, a pass, the next `action_id`, a phase transition or the end of the game. The rolls of the action phase carry their `action_id` and belong to their turn. A component action within a turn (an agent) is a line of that turn.
- What a seat does at its turn menu before it chooses its action (a transaction, a component action that does not end the turn, such as an agent) has an `action_id` of its own: it is a part of the turn that follows when that turn is of the same seat; else it is a turn of its own, "Transaction". A part that the seat ended (the fact of the end of its turn) is a turn of its own. The turn keeps the id of its first part. Its action is its tactical or strategic action or its pass, else its first component action; another component action in it is a line ("Component action · Carth of Golden Sands").
- The turns are numbered in their round from 1, in the order of the game.
- A phase that is no action phase is one entry, "Strategy phase · Round 3", with what the seats did in it: the picks, the scored objectives, the votes of each agenda, the cards that were played. The start of an action phase is an entry with no details: a divider between the turns.

### The scope

- The Log starts at the last turn of the viewer, and that turn is its first entry, so that the player sees what it did and then what the others did after it. Oldest first; the phases in between are their entries. The turn that is played now is last and marked live; a turn that ended with no fact of its end (a tactical action that ends after its movement) is not live once another seat has the turn (`active_player`). A turn of the viewer that is played now is not its last turn: the Log starts at the turn before it.
- A spectator, and a seat before its first turn, see the round in play.

### An entry

- A turn has its seat, its kind ("Tactical · Mecatol Rex · #18", "Strategic · Technology", "Component · Sabotage", "Pass"), its round and its number, a summary of at most three outcomes of its seat ("moved 3 ships · combat (lost 1, destroyed 1) · landed on Mecatol Rex", "researched Gravity Drive"), and its details in groups. A pass has no summary and no details.
- The groups of a tactical or a component action are its steps: Activation, Movement (the ships from each system to each system, by unit, and what was loaded), Space cannon, Space combat (the rolls of each round, the losses, a retreat), Invasion (the bombardment, the landings by planet, the ground combat), Production (what was produced where, and what paid for it), then Effects, Cards played and Transactions. A strategic action has Primary (what its seat did: the research, the structures, the tokens, the system of Diplomacy and the planets it readied, the speaker of Politics, the seats whose commodities Trade replenished, Warfare's recalled token and command sheet) and Secondaries, then Cards played and Transactions. Secondaries has a part for each seat that answered: its answer first ("Followed" or "Declined"), then what it did, in the order of the answers; the seats that declined are last. What a seat does after the end of the turn (a unit removed over its fleet supply) is still a part of it.
- The summary of a strategic action ends with who followed: "2 followed", "none followed". A purchase of a token with influence is its payment ("Spent influence: Jord") and the tokens it gains, with no line of its own; the tokens of a seat are one line ("Gained 2 tactic tokens, 1 fleet token").
- The status phase has a group "Command tokens" with the command sheet of each seat after its redistribution ("3 tactic · 3 fleet · 2 strategy"). The votes of each agenda come before the cards played.
- A line has its seat, a text in words, and the dice of a roll (the faces, the face that hits, the hits). Units of one line are counted: "Dreadnought I, 2 Carrier I".
- A round of ground combat is a fact of the host (`ground_combat_round`, with its planet): its rolls are "Ground combat · Mecatol Rex · round 2", and a further ground combat on the planet (`ground_combat_started`) counts from 1 again. The rounds of a space combat are not told by the host: a roll after a decision, or a second roll of the same seat at the same value (the engine rolls the units of one seat and one value together), starts the next round.
- Not there yet, because the host does not say it: the casualties that no seat chose (a forced loss, the anti-fighter barrage), who won a combat, a planet that changed hands, the outcome of an agenda, an exploration, and the revealed objectives. The Log shows what the facts say and adds nothing to it.
