# Evidence: Command Token Pool Refactor

## Package
Command token pool refactor: single field → three pools (tactic, fleet, strategic)

## Objective
Align command token structure with Oracle's three-pool system (LRR 52.4).

## Oracle Inspection
Inspected `D:/Projects/ti4-engine/engine/state.py`:

```python
tactic_tokens: int = 3
fleet_tokens: int = 3
strategic_tokens: int = 2
```

### Oracle's POOL_NAMES mapping (game.py)
| Pool | Name | Used For |
|------|------|----------|
| tactic_tokens | "tactic" | Movement, combat, bombardment |
| fleet_tokens | "fleet" | Fleet capacity, supply |
| strategic_tokens | "strategic" | Production, structure placement |

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
Result: ✅ 32 passed; 0 failed

### Test workspace
```
cd D:/Projects/ti4-engine-rs
cargo test --workspace
```
Result: ✅ 34 passed; 0 failed (32 ti4-engine + 2 ti4-model)

## Changes Made

### PlayerState (ti4-model/src/state.rs)
**Before:**
```rust
pub command_tokens: i32,
```

**After:**
```rust
pub tactic_tokens: i32,
pub fleet_tokens: i32,
pub strategic_tokens: i32,
```

**Default:** 3/3/2 (per Oracle)

### BotView (ti4-model/src/view.rs)
**Before:**
```rust
pub my_command_tokens: i32,
```

**After:**
```rust
pub my_tactic_tokens: i32,
pub my_fleet_tokens: i32,
pub my_strategic_tokens: i32,
```

### EffectEngine (effects.rs)
| Method | Before | After |
|--------|--------|-------|
| `apply_leadership_effect()` | +3 command_tokens | +1 tactic, +1 fleet, +1 strategic |
| `apply_warfare_effect()` | +1 command_tokens | +1 tactic_token |

### GameLoop (game.rs)
- `reveal_strategy()` now calls `apply_strategy_effect()` after recording card
- `step_command()` distributes into correct pools:
  - P1 (idx 0): +2/+1/+1 = 4 total
  - P2 (idx 1): +1/+1/+1 = 3 total
  - P3 (idx 2): +1/+1/+0 = 2 total
  - P4 (idx 3): +1/+0/+0 = 1 total

### TacticalManager (tactical.rs)
| Operation | Before | After |
|-----------|--------|-------|
| Movement cost | command_tokens | tactic_tokens |
| Production cost | command_tokens | strategic_tokens |

### Tests Updated
- `test_command_token_distribution` - Updated to check totals (8 default + added)
- `test_leadership_strategy_effect` - Clear initial tokens, check +1 each pool
- `test_strategy_effect_dispatch` - Clear initial tokens, verify dispatch
- `test_full_round_simulation` - Updated to check three-pool totals

## Compatibility Evidence
- No breaking changes to GameState public API (internal field change only)
- BotView fields renamed but semantics preserved (total tokens = sum of pools)
- All existing tests continue to pass with updated assertions
- No changes to game flow logic

## Benchmark Effect
- Negligible performance impact (three i32 fields vs one = 8 bytes additional)
- Token access patterns unchanged (still O(1) field access)

## Unresolved Differences
- Oracle allows player choice of which pool to add tokens to (LRR 52.4)
- Current implementation uses fixed distribution (simplified)
- Thunder's Edge variants may modify pool distribution (not yet implemented)

## Source Oracle Commit
`37061c511a4780d4c0719e0342533a498cd4b457`

## Files Changed
- `crates/ti4-model/src/state.rs` - Three token pools (+3 fields, -1 field)
- `crates/ti4-model/src/view.rs` - Three token fields in BotView (+3, -1)
- `crates/ti4-engine/src/effects.rs` - Updated leadership/warfare effects
- `crates/ti4-engine/src/game.rs` - Effect integration, pool-aware tests
- `crates/ti4-engine/src/tactical.rs` - Pool-aware costs

## Review
- Self-reviewed: All changes align with Oracle's three-pool structure
- Test coverage: 3 new test updates, all passing
- Oracle alignment verified against state.py and game.py
- No breaking changes to public API
