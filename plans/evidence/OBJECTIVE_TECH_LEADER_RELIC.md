# Evidence: Objective scoring, technology research, leader abilities, and relic handling

## Package
Objective scoring, technology research, leader abilities, and relic handling

## Objective
Implement GameState methods for:
1. Public objective scoring (vp calculation based on control tokens and completed objectives)
2. Secret objective completion checking
3. Technology research with commodity cost
4. Leader activation/fatigue lifecycle
5. Relic awarding and querying

## Commands and Results

### Build
```
cd D:/Projects/ti4-engine-rs
cargo build --workspace
```
Result: ✅ BUILD SUCCESS

### Test ti4-engine
```
cd D:/Projects/ti4-engine-rs
cargo test -p ti4-engine
```
Result: ✅ 23 passed; 0 failed

### Test workspace
```
cd D:/Projects/ti4-engine-rs
cargo test --workspace
```
Result: ✅ 25 passed; 0 failed (23 ti4-engine + 2 ti4-model)

## New Methods (GameState)

### score_objective
- Input: `player: &PlayerId`, `objective: &ObjectiveState`
- Output: `i32` (VP score)
- Logic: Sums control tokens + completed objectives as VP proxy
- Records objective completion in player state

### check_secret_objective
- Input: `player: &PlayerId`, `secret: &SecretObjectiveState`
- Output: `bool`
- Logic: Checks if player has technologies/control tokens/fleet (simplified)

### research_technology
- Input: `player: &PlayerId`, `tech: TechnologyId`, `cost: i32`
- Output: `std::result::Result<bool, String>`
- Logic: Checks commodity >= cost, deducts commodity, adds tech, increments tech level

### get_tech_level
- Input: `player: &PlayerId`, `tech: &TechnologyId`
- Output: `i32`
- Logic: Returns tech level from player's tech_levels map (default 0)

### activate_leader
- Input: `player: &PlayerId`, `leader: LeaderState`
- Output: `std::result::Result<(), String>`
- Logic: Checks leader not fatigued, sets active_leader

### fatigue_leader
- Input: `player: &PlayerId`, `leader_id: LeaderId`
- Logic: Adds leader_id to player's leader_fatigue list

### refresh_leaders
- Input: `player: &PlayerId`
- Logic: Clears player's leader_fatigue list

### award_relic
- Input: `player: &PlayerId`, `relic: RelicState`
- Logic: Adds relic to player's relics list (no duplicates)

### has_relic
- Input: `player: &PlayerId`, `relic_id: &RelicId`
- Output: `bool`
- Logic: Checks if player has relic with matching id

## Tests Added

### test_objective_scoring
- Verifies VP scoring with 0, 1, and 2 control tokens
- Confirms completed objective adds to score
- Result: ✅ PASS

### test_technology_research
- Verifies successful research with sufficient commodity
- Verifies failed research with insufficient commodity
- Verifies tech level tracking
- Result: ✅ PASS

### test_leader_activation
- Verifies successful leader activation
- Verifies fatigue prevents activation
- Verifies refresh clears fatigue
- Result: ✅ PASS

### test_relic_handling
- Verifies relic awarding
- Verifies duplicate prevention
- Verifies cross-player relic isolation
- Result: ✅ PASS

### test_secret_objective_check
- Verifies false when no technologies/control/fleet
- Verifies true when technologies present
- Result: ✅ PASS

## Compatibility Evidence
- No changes to existing game flow or public APIs
- New methods are additive to GameState
- Model types (ObjectiveState, SecretObjectiveState, LeaderState, RelicState) unchanged
- All existing 20 tests continue to pass

## Benchmark Effect
- No performance benchmarks added (new methods are O(1) or O(n) with small n)
- Technology research: O(1) hash set insert + commodity check
- Leader activation: O(n) fatigue list scan (n = small number of leaders)
- Relic check: O(n) list scan (n = small number of relics)

## Unresolved Differences
- Secret objective check is simplified (no condition-specific logic yet)
- Objective scoring uses control token count as VP proxy (not full condition evaluation)
- Leader ability effects not yet implemented (activation only)

## Source Oracle Commit
`37061c511a4780d4c0719e0342533a498cd4b457`

## Files Changed
- `crates/ti4-model/src/state.rs` - Added 8 new GameState methods (~120 lines)
- `crates/ti4-engine/src/game.rs` - Added 5 new tests (~80 lines)

## Review
- Self-reviewed: All methods follow existing patterns
- Test coverage: 5 new tests, all passing
- No breaking changes to existing code
