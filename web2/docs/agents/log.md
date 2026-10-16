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

## Read-only

- Nothing in the Log is a control (patterns 10 and 11): the Log is the same content with nothing open. Only what changes the view stays: a group that opens, a rule text behind "?", the rolls of a battle.
- This is done once, at the root of the Log, not per block: `ReadOnly` (`src/game/context.tsx`) puts the content in a `ReadOnlyScope` (`src/ui/readOnly.tsx`) and gives it a dispatch that does nothing. Inside it a `Button` is not there (an `ActionButton` that is on is its state, "✓ …"), a `Counter` is its count, a `Press` (a row, a token, a cell, the whole control that is its content) is its content with its state, a shortcut key (`Kbd`) is not shown, the setting of a reaction is its word, and the search of a system is not there.
- A new control in a block uses these primitives (`Button`, `Counter`, `Press`), never a bare `<button>` that dispatches, so that it is read-only in the Log by itself.

## The history

- The turns of the other seats since the viewer's last turn come with typed public facts of the host (not there yet). Until then the history is "No turns since your last turn".
