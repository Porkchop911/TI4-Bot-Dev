# web2: decisions, reaction windows, action picker

Part of `web2/AGENTS.md`: the Patterns and the rules of Screen size and Text there apply here too.

## Action picker and shortcuts

- There is no separate turn bar. The action picker is the content of the action panel when the viewer has the turn and no action is open.
- The picker has fixed groups in a fixed order: Tactical, Strategic, Component, Trade, Pass. A row that cannot be used stays in its place and shows the reason.
- The picker is one list. The whole row is the control: key, group, name, and the state at the right edge. Rows have no buttons of their own.
- A choice in the picker sends nothing. A tactical action opens as the private draft. A strategic or component action is staged. The main button of the footer sends it.
- After the viewer's action, the panel shows what uses no action, and the footer has "End turn".
- A shortcut key is data: `ActionButtonView.key`. The button shows the key. Enter is the main button of the footer. Esc goes one level back.
- The picker is static: it has the same rows in every turn of a game. Only the number of strategy cards changes it, with the player count. Action cards have one row ("Play an action card"), other components have one row. A second list asks which one, with the keys 1, 2, 3.
- A strategy card has its number as key.
- Browser back is not used for game choices. Esc goes one level back; a draft has undo and redo. Browser back was tried for the picker (2026-10-07) and removed: it could not apply to the other screens.
- Shortcuts do not act in a text field or while a dialog, a menu or a rule text is open. A control that the player reached with Tab keeps Enter.

## Decisions

A decision is any choice that a card, an ability or a rule asks for. Every decision uses the same parts.

- A decision appears where its cause is: as blocks in the open action. The panel does not switch to another screen, and there is no dialog.
- The source is on top: the card, ability or rule that asks, as printed (the `source` block). The heading says what the player must do.
- There are three shapes of choice. The thing that is chosen decides the shape:
  - on the board, when it is on the board (a system, a planet, a place). The panel shows the choice and the limits, not a list of candidates;
  - in the player table, when it is a player. The table already shows the values of each player;
  - in a list, for everything else (answers, rewards, cards). Rows are the controls, with the keys 1, 2, 3 and the effect in game values at the right edge.
- An amount is a counter row. A limit is a gauge above the rows.
- A choice is staged, then sent. A staged choice shows "Selected" and "Not sent". The main button of the footer sends it. Esc clears the choice first, then leaves the action.
- A thing that cannot be chosen stays in view and shows the reason.
- Draft and History use the same blocks.

## Reaction windows

The owner asked on 2026-10-13 for the printed text of every reaction and for a setting of every reaction.

- A reaction window is the question of the action panel, with the title "Reaction". The heading states the trigger in game values ("After Hacan’s tactical action in system 18"), never an id of the engine. An event with no wording of its own is said in its words, with the seat and the system (`select/reaction.ts`, `triggerText`).
- The window lists what the viewer can use now, one row each: an action card, a leader, a faction ability, a technology, a promissory note, a unit, a breakthrough. A row has the printed name, the kind at the right, the printed timing and text, and the keys 1, 2, 3. No window has code of its own: the engine says what every option stands for (`reaction` of the update, `ti4-host/src/view/reaction.rs`, from the id of the option).
- The main button of the footer is Pass. A row stages its reaction; the main button then says "Use", and Esc clears it. Enter always does what the main button says. The decline is no row.
- **The setting of a reaction** is in its row of the window, at the right edge, and on its card in a sheet (pattern 1: the window is where a reaction is muted; the sheet is where it is set back, because a muted window does not open). It is one list (`OfferSelect`): "Offered", "Only on my turns", "Never". It applies to the windows after it and leaves a toast. The window that is open stays open and is answered by the player: the list only sets, it never passes (owner, 2026-10-13). Exception to "a choice is staged, then sent": it is a setting of the seat and no decision of the game.
- "Only on my turns" is of the action phase: the reaction is passed while another seat is the active player. Outside the action phase nobody has the turn, the reaction is offered, and the list of a window there has "Offered" and "Never" only.
- The sheets: the Cards sheet for an action card of the hand; the sheet of the viewer for a leader, a faction ability with a timing and a technology that is used. A reaction that is not always offered and has no card there (a promissory note, a unit, a breakthrough) is a row of "Other reactions" in the sheet of the viewer. The sheet of another seat has no setting.
- A window that does not open because of the setting leaves a toast that names what was not offered.
- The setting is applied where the game runs, by `ti4_host::view::reaction_modes::NeverOffer`: the engine of a local game and the server use the same wrapper. A reaction is set by its key (`ReactionOffer.key`): the printed name of an action card, so every copy is covered, or the kind and the id of the card of an ability ("leader:sardakkagent"), so an ability that hangs on several events has one setting. The page only tells the transport (`Transport.setReactionMode`) and shows what the transport says (`modes`, `passed` of its events). A window that offers only what the seat muted is declined without asking; a window with something else is asked without it. Only a window that the seat may decline is touched. In a local game that decline is an answer like any other, so the saved game has it, and a saved game is played again the same whatever is set now. An undo takes it back together with the answer of the player before it. The modes are kept next to the saved game (`ti4.save.<id>.cards`; a list of the names of the Never cards is read too), and by the server (`reaction_modes.json`).
- The engine says whose turn it is with the question (`details.active` of the choice, display only), so the filter needs no state.
- Not there yet: an ability that asks its own question outside a window of the timing resolver (some "you may" abilities) is a decision in the list, with no setting; the mock still has the two modes of a card.
