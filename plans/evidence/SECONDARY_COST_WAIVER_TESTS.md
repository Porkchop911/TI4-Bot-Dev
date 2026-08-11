# Evidence: Secondary Cost Waiver Tests

## Package
Add tests for faction ability cost waivers for strategy card secondary abilities

## Objective
Verify that Masters of Trade and Brilliant factions correctly get free secondary abilities.

## Commands and Results

### Test workspace
```
cd D:/Projects/ti4-engine-rs
cargo test --workspace
```
Result: ✅ 36 passed; 0 failed

## New Tests

### test_secondary_cost_waiver_masters_of_trade
- Sets p0 faction to "masters_of_trade"
- Applies Trade strategy
- Verifies +3 trade goods from primary effect (Masters of Trade gets primary benefit)

### test_is_secondary_free
- Tests normal player: not free for Trade or Technology
- Tests Masters of Trade: free Trade, not free Technology
- Tests Brilliant: not free Trade, free Technology

## Compatibility Evidence
- No breaking changes to GameState public API
- Faction ID matching uses `as_str()` for direct string comparison
- All existing tests continue to pass

## Unresolved Differences
- Oracle's FREE_SECONDARIES is extensible via faction registration
- Oracle's Brilliant also swaps the Technology primary (91.2) for the secondary (91.3)
- Other faction-specific secondary waivers not yet implemented
- No test for Jol-Nar Brilliant Technology swap yet

## Source Oracle Commit
`37061c511a4780d4c0719e0342533a498cd4b457`

## Files Changed
- `crates/ti4-engine/src/game.rs` - 2 new tests (+42 lines)

## Review
- Self-reviewed: Test coverage matches Oracle's FREE_SECONDARIES registration
- Test coverage: 36 tests passing
- Oracle alignment verified against faction_abilities/__init__.py
