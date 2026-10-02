- [x] technology tooltips and colors
- [x] overlays for map (ressource/influence/space units/ground combat units)
- [x] main game stats (total ressources/influence, remaining ressources/influence)
- faction rules tooltips
- test all strategy cards + secondaries
- [x] test scoring objectives
- test winning the game
- test agenda phase

### 1. Already in the Protocol Wire View (High-Value / Immediate Wins)

These fields already arrive from the server in PlayerView and TableView, but have no visual component:
  • Leaders (Agents, Commanders, Heroes)
      • Engine / Protocol: PlayerView.leaders: Record<string, string> tracks status (locked, unlocked, exhausted, purged).
      • Web UI: Completely unrendered. There is no leader tray or indicator on PlayerSheet.tsx showing who has unlocked their commander, purged their hero, or readied their agent.
  • Relics & Relic Exhaustion
      • Engine / Protocol: PlayerView.relics and exhausted_relics.
      • Web UI: Not displayed anywhere in PlayerSheet.tsx or any modal.
  • Galactic Laws in Play (Agendas Passed):
      • Engine / Protocol: TableView.laws: Record<string, string>.
      • Web UI: Not rendered anywhere. Players cannot see which laws (e.g., Minister of War, Wormhole Research) are active in the galaxy.
  • Unclaimed Strategy Cards & Accumulated Trade Goods:
      • Engine / Protocol: TableView.unclaimed_strategy_cards and TableView.strategy_card_goods.
      • Web UI: Neither the unpicked strategy cards nor the bonus trade goods placed on them are displayed.
  • Strategy Card Exhausted / Played State:
      • Engine / Protocol: PlayerView.exhausted_strategy_cards.
      • Web UI: Strategy card badges in PlayerSheet.tsx:340-367 render identically regardless of whether the card's primary ability has already been exhausted.
  
  ──────
  ### 2. Modeled in the Game Engine, but Missing from the Wire Protocol & UI
  
  These exist in ti4_model::state::GameState and Player, but are omitted from projection.rs and the UI:
  
  • Promissory Notes (Held & Face-Up in Play Area):
      • Engine: GameState.promissory_notes, GameState.promissory_faceup, and Support for the Throne holders.
      • Web UI: Promissory notes can be selected in TradeDeskModal.tsx, but there is no persistent view showing:
          1. A player's private held promissory notes.
          2. Face-up notes in play areas (crucially, who holds whose Support for the Throne or Alliance note).
  • Relic Fragments:
      • Engine: Player.relic_fragments: BTreeMap<String, i32> (Cultural, Hazardous, Industrial, Unknown).
      • Web UI: Completely invisible (players cannot see their fragment counts towards forging relics).
  • Captured Units (Vuil'raith Cabal / Vortex):
      • Engine: Player.captured_units: Vec<(PlayerId, UnitTypeId)>.
      • Web UI: Cabal players cannot see what plastic they have captured or available to consume.
  • Dynamic Board Tokens (Frontier Tokens, Ion Storm, Creuss Wormholes, Custodians):
      • Engine: GameState.frontier_tokens, GameState.ion_storm, GameState.wormhole_tokens, GameState.custodians_removed.
      • Web UI: BoardTile.tsx only displays static printed tile data. Empty systems do not indicate whether a frontier token is present or explored, and dynamic tokens (Creuss alpha/beta, Ion storm) are
      missing.
  • Component Supply / Reinforcements Limit:
      • Engine: ti4_engine::supply.
      • Web UI: Players cannot see remaining unit plastic in their supply when building or planning fleets.
  • Faction Reference & Faction Sheet:
      • Currently, the UI only displays the faction name string. Faction abilities, faction-specific units, and faction promissory notes have no overview panel.
  
  ──────
  ### Suggested Priorities After the Tech Overview
  
  1. [x] Scored Public Objectives Grid: Marking which players scored each revealed public objective on PlayerSheet.tsx (the backend data is already in TableView.scored_objectives).
  2. Leaders & Relics Tray: Adding a collapsible section or badges to PlayerSheet.tsx using PlayerView.leaders and PlayerView.relics.
  3. Face-Up Promissory Notes (Support for the Throne / Alliance): Projecting public promissory notes in TableView / PlayerView so table alliances and VP sources are transparent.
  4. Active Laws Panel: Displaying TableView.laws near the public objectives.
