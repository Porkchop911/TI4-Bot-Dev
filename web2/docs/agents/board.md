# web2: board, payment, exploration

Part of `web2/AGENTS.md`: the Patterns and the rules of Screen size and Text there apply here too.

## Board

- Payment and planet choices are made on the map only, and the controls of a payment are there too. The footer of the panel names what pays.
- Planets that the open task can use must stand out. The other board content is dimmed during the task.
- The activation has three levels (owner, 2026-10-10). A system stands out when the action does something there: ships of the player can move to it, the player has a production value there, or the player's SPACE CANNON fires at the ships of another player there. A system that cannot be activated (the player's command token) is dimmed. Every other system is neither: a click still chooses it. The facts come from the engine (`BoardTaskView.highlight`).
- While the viewer is at the turn menu, the inspector of a system has "Activate this system": it opens the tactical action as the private draft with the system staged. A system with the command token of the viewer says so instead, with no button.
- System names are flavor. The map does not show them. The inspector shows the name when the player clicks a system.
- **A click on a system shows what is in it** (owner, 2026-10-14), on every screen: the inspector, or the sheet of the task at that system (the fleet of a movement, the build of a production, the landings), which has the same details (`SystemDetails`: the facts, the ships in space by seat, and each planet with `res/inf`, `PlanetTraits`, the owner and the forces). In the activation the click also chooses the system; closing the inspector does not take the choice back, and the inspector closes when the activation is sent or recorded, so the movement has the whole map.
- A system can have up to three planets. The tile layout must support this.
- A planet that can pay shows resources and influence (`res/inf`). The value that pays is large. The player must see what the payment gives up.
- The system that an open window is about (a reaction window, an ability that asks in the middle of an action) has a wide ring in the colour of the window while it is open (`BoardView.marked`; which system: `docs/agents/decisions.md`, "Reaction windows"). Its tooltip and accessible name say it.
- The inner ring of a system shows who has ships there, in the colour of that seat. A red dashed ring shows that two or more players have ships there.
- Seat symbols are drawn shapes of one size (`SeatShape` in `src/game/context.tsx`). Do not use font glyphs for them: their sizes are not equal.

- The map opens on the view "Standard", the first of the map views: `res/inf` of each planet. The view "Owner" has the symbol of the owner in each planet.
- An exhausted planet has a dashed outline in every map view, also while the open task can use it; its tooltip says "exhausted". The inspector has the note "Exhausted".
- The board has no log line and no log button. The log is a reference sheet in the toolbar.
- The map view "Exploration" shows the type of each planet as a letter in the colour of the type (C, H, I), "?" while the planet is still explored when a player takes it, and "+" when something is attached. It is not a control. A planet of the open choice keeps its `res/inf` in every map view.
- Every map view shows the board after the staged movement: the ships and ground forces that stay, and the fleet that arrives. The player must see what a move leaves behind.

## Payment

- Every control of an open payment is on the board, in one bar on top of the map: how the payment stands ("Paying 4 / 6 resources", then "2 more" red, "✓ Paid" green or "⚠ 1 wasted" orange), the trade goods, "Auto-pay" and "Clear payment".
- Trade goods are not on the map. The bar has a counter for them, in steps of one, and "Pay rest": it sets the trade goods to what the planets leave, so the payment is exact. It is disabled when the trade goods cannot do that.
- Commodities that the engine lets pay as trade goods (the Keleres agent, `commodities` of the payment) have a counter of their own in the bar, next to the trade goods. It is not there otherwise.
- A planet pays with the face that the engine names: its own value, or the other one (`face`: Archon's Gift, Freelancers). The value that pays is the large one on the map, and the payment line names the face ("Mecatol Rex 6 influence").
- Auto-pay stages the payment with the least waste, with planets before commodities and trade goods. The player can change it; nothing is sent.
- The panel has no payment block while a payment is open. The footer has one line for it, left of the note: what pays and the total ("Payment Jord 4 + Vefut 2 · 6 / 6"). It uses no vertical space.
- A recorded or planned payment is a read-only line in the content. The board has no bar for it.

## Exploration

The owner chose on 2026-10-10 where it shows: in the landing sheet and the system inspector, in a map view of its own, and the decks as a reference sheet.

- Every screen that lists a planet has the same line for it (`PlanetTraits`): the type as icon and word, "Unexplored" while a player who takes the planet explores it, and each attachment with its name and what it changes ("Dyson Sphere +2 / +1"). The printed text of an attachment is behind "?". The landing sheet and the system inspector have it.
- The type is the control of its deck: it opens the reference sheet "Exploration" at that deck. The sheet is in the toolbar too, as the other reference sheets.
- The sheet has the four decks. A deck says how many cards are left and who has its relic fragments. A card is one row for all cards of that name: the printed text, "left / total", and who drew one, when and where. A card with none left is dimmed; "0 / 1" is its sign, with no "–" before it (owner, 2026-10-11).
- Exception to "rule text is behind hover": the deck sheet shows the printed text of each card, as a list of cards does. The reason: the player opens the sheet to read what the deck can still give.
- `res/inf` is the value of a planet now, from the engine: the printed value with its attachments and the laws on it (`PlanetView.resources`/`influence`, sent when they differ). The attachment line says what each adds.
- A decision that an exploration card asks has the card on top, as printed (the `source` block), with the deck and the planet.
- The facts come from the engine: which planets are explored, what the decks hold, what was drawn (`exploration` of the update). The names and texts are the catalogue of the engine (`Transport.catalog`); without it a card is shown by its id.
- The relic fragments and the relics of a player are in the sheet of that player.
