# H - tooltips

One capture script per tooltip: `capture-NN-*.ts`, writing `out/NN-*.png`. Rerun with `cd web && npm run screenshots -- H`.

Custom tooltips are real hover states. Native `title` tooltips are drawn by `_shared/tooltip.ts` from the element's own title text, because Chrome draws them outside the page.

The web UI first appeared on this branch, so every tooltip below is absent from `main`; the last column is the commit that introduced it (found with `git log -S`).

| # | Tooltip | Source file (web/src) | Added in |
|---|---|---|---|
| 01 | Board tile hover | components/board/BoardTooltip.tsx | fa19591 add map overlays |
| 02 | Map zoom button | components/Board.tsx | 2c70290 add accessible primitives |
| 03 | Map overlay button | components/MapOverlayToolbar.tsx | fa19591 add map overlays |
| 04 | Victory point breakdown | components/PlayerSheet.tsx | adac4ef |
| 05 | Resources badge | components/PlayerSheet.tsx | b3de918 add additional player stats |
| 06 | Influence badge | components/PlayerSheet.tsx | b3de918 add additional player stats |
| 07 | Controlled systems | components/PlayerSheet.tsx | b3de918 add additional player stats |
| 08 | Controlled planets | components/PlayerSheet.tsx | b3de918 add additional player stats |
| 09 | Production capacity | components/PlayerSheet.tsx | b3de918 add additional player stats |
| 10 | Strategy card | components/PlayerSheet.tsx | 93b2d9f Add server and UI |
| 11 | Action card in hand | components/PlayerSheet.tsx | d0e156d fix tests |
| 12 | Secret objective | components/PlayerSheet.tsx | 93b2d9f Add server and UI |
| 13 | Turn sound | components/PlayerSheet.tsx | adac4ef |
| 14 | Event log: action card | components/EventLog.tsx | adac4ef |
| 15 | Event log: actor | components/EventLog.tsx | 5d88a56 make game event log hierarchical |
| 16 | Combat: roll results | components/SpaceCombatOverlay.tsx | 8c263ab improve space combat |
| 17 | Invasion: default target | components/InvasionLandingTray.tsx | adac4ef |
| 18 | Invasion: reset button | components/InvasionLandingTray.tsx | adac4ef |
| 19 | Combat: destroyed this step | components/SpaceCombatOverlay.tsx | ab32f71 fix some issues |
| 20 | Combat: action card chip | components/SpaceCombatOverlay.tsx | 8c263ab improve space combat |
| 21 | Combat: fleet supply gauge | components/SpaceCombatOverlay.tsx | 008a421 add space combat modal with dice hits |
| 22 | Combat: capacity gauge | components/SpaceCombatOverlay.tsx | 008a421 add space combat modal with dice hits |
| 23 | Reaction switch | components/PlayerSheet.tsx | 5a7f407 Add per-card reaction mode toggle |
| 24 | Combat odds: attacker win | components/SpaceCombatOverlay.tsx | 008a421 add space combat modal with dice hits |
| 25 | Combat odds: mutual destruction | components/SpaceCombatOverlay.tsx | 008a421 add space combat modal with dice hits |
| 26 | Combat odds: defender win | components/SpaceCombatOverlay.tsx | 008a421 add space combat modal with dice hits |
| 27 | Legendary planet | components/SystemInspector.tsx | 01e26a5 clean up ui interaction model |
| 28 | Lobby: remove player | components/Lobby.tsx | 6c3fa6b implement bot play in server |

The win-probability tooltips (24 to 26) need the odds service: `odds.ts` intercepts `POST /advisor/battle` with Playwright route interception and answers a fixed response. Shot 28 renders the real lobby as its host through `_shared/mockLobby.ts`.
