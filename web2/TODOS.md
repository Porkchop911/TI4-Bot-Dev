web2 covers the action phase: the tactical action end to end, all eight strategy cards (primary and secondaries), the action picker with shortcuts, decisions on the board, in the player table and in a list, reaction windows, and payment on the map. The other phases, transactions and most reference sheets are still missing. The list below is what web/ has and web2 does not; it was checked against the code on 2026-10-08.

Missing features

Phases and flows with no web2 counterpart

- Strategy phase: no strategy card draft. web/ has the card grid with trade goods on each card.
- Status phase: no objective scoring window (score or decline), no command-token redistribution, no hand-limit discard.
- Agenda phase: no ballot, no voting with planets picked on the map, no tallies, riders (predict outcome) or speaker tiebreak.
- Transactions: no trade desk (you offer / you receive, net value, deal categories).
- Component actions: action cards only (a list, with a target or a decision for some). The row for technologies, leaders and planets is a disabled stub. web/ has legendary planet abilities and leaders/agents.

Generic decision UIs (web2 has the source block and the three shapes: board, player table, list; plus counters, gauges and the technology tree)

- Offer card: one scripted example (Merchant Station). web/ uses it for roughly 15 faction and commander decisions.
- Pickers: a unit picker is missing. Remove-unit has one scripted example, with the fleet supply limit only.
- Placement: structures exist (Construction). Units from reinforcements are missing.
- Exploration: the reward card choice.
- Reaction windows: Morale Boost and the retreat destination still use the old offer block inside the battle table, not the reaction window and the board.

Reference sheets (web2 renders all of these as plain bullet lists)

- Objectives: no cards with who scored what, and no VP breakdown per player (public / secret / other, summing to the total).
- Technology: no tree by colour with prerequisites, no cross-player comparison, no tech-skip toggle.
- Player sheet: no objectives list. (The action card hand is in the Cards sheet.)
- Log: no card names with hover text and no Copy replay button.

Board

- Planet state: PlanetMarkView has no exhausted/ready outside a payment task, and no traits, legendary marker or attachments. web/'s inspector shows all of these.
- Units: no unit icons or per-type breakdown on tiles, only ship count and strength. Ground forces are a single number.
- Command tokens: only the viewer's token is modelled (TileView.commandToken), not other players'.

Outside the game screen

- Map picker with preview (phase 4 of `plans/2026-10-12-LOBBY_PLAN.md`). The lobby, the nicknames, the seats and the bots are built.

Missing UX niceties

- Corner toasts: web/ toasts other players' public actions, merges bursts and has a mute toggle. web2 has one toast slot.
- Auto-resolved decisions: no toast when the engine settles a single-option decision for you.
- Turn sound: no beep with a remembered mute.
- Reaction auto-pass: no countdown with a pin to hold the window open.
- Hover tooltips: web/ has a board tooltip with a system summary and combat-odds tooltips. web2 has only SVG <title> text.
- Live undo/redo: web2's undo exists only inside the draft.
- Paused plan: no banner for "plan stopped at a reaction window, resume the rest". web2's draft stops at dice boundaries, but there is no live resume.
- Dev tooling: web/ has a decision gallery with about 1,500 lines of cases, a scenario launcher, and screenshot folders A–N with the artifact builder. web2 has 35 examples, a primitives gallery and e2e/shots.spec.ts. The capture-*.ts convention in the root CLAUDE.md exists only under web/.

Suggested order

1. Strategy phase, status phase and agenda phase.
2. Transactions.
3. The long tail of decisions: unit picker, placement from reinforcements, exploration.
4. Richer reference sheets and the toast layer.
