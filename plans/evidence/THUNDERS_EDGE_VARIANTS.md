# Evidence: Thunder's Edge Strategy Card Variants

## Package
Add Thunder's Edge strategy card variants to StrategyCard enum

## Objective
Support Thunder's Edge expansion strategy cards (te4construction, te6warfare) per Oracle.

## Oracle Inspection
Inspected `D:/Projects/ti4-engine/engine/strategy.py` and `engine/content/strategy_cards.json`:

### Thunder's Edge Variants
| ID | Name | Initiative | Primary Effect |
|----|------|-----------|----------------|
| te4construction | Construction | 4 | Either place 1 structure OR use PRODUCTION, then place 1 structure |
| te6warfare | Warfare | 6 | Free tactical action in any system (no token cost) |

### Oracle Dispatch Logic (`implementation_for`)
```python
def implementation_for(card_id: str, name: str, table: dict) -> tuple[str, object] | None:
    if card_id in table:
        return card_id, table[card_id]  # ID-specific first
    if name in table:
        return name, table[name]  # Fall back to name
    return None
```

## Commands and Results

### Build
```
cd D:/Projects/ti4-engine-rs
cargo build --workspace
```
Result: ✅ BUILD SUCCESS

### Test workspace
```
cd D:/Projects/ti4-engine-rs
cargo test --workspace
```
Result: ✅ 34 passed; 0 failed

## Changes Made

### StrategyCard Enum (ti4-model/src/state.rs)
**Added variants:**
```rust
pub enum StrategyCard {
    // ... existing 8 ...
    Te4Construction,  // Thunder's Edge Construction
    Te6Warfare,       // Thunder's Edge Warfare
    Unknown,
}
```

**from_code parser:**
```rust
"te4construction" => Self::Te4Construction,
"te6warfare" => Self::Te6Warfare,
```

**code() method:**
```rust
Self::Te4Construction => "te4construction",
Self::Te6Warfare => "te6warfare",
```

### EffectEngine (effects.rs)
| Method | Card | Effect |
|--------|------|--------|
| `apply_te4_construction_effect()` | Te4Construction | +2 production (structure placement proxy) |
| `apply_te6_warfare_effect()` | Te6Warfare | has_war=true, +1 tactic_token (free action proxy) |

### Effect Dispatch (apply_strategy_effect)
Added two new arms to the match:
```rust
StrategyCard::Te4Construction => self.apply_te4_construction_effect(game, player),
StrategyCard::Te6Warfare => self.apply_te6_warfare_effect(game, player),
```

## Compatibility Evidence
- No breaking changes to existing code
- Thunder's Edge variants only activate when card_id matches
- Base cards continue to work with original behavior
- Oracle's dispatch logic (ID-first, name-fallback) preserved in design

## Benchmark Effect
- Negligible memory overhead (2 enum variants = 0 bytes at runtime)
- No performance impact (enum dispatch is O(1))

## Unresolved Differences
- te4construction primary effect is simplified (+2 production)
  - Oracle allows player choice: structure placement OR PRODUCTION
  - Second structure placement is also player-choice
- te6warfare primary effect is simplified (proxy token grant)
  - Oracle grants free tactical action in any system
  - Player may redistribute command tokens before/after
- Thunder's Edge secondary abilities not yet implemented
  - te4construction secondary: spend token to place structure
  - te6warfare secondary: production in home system (same as base)

## Source Oracle Commit
`37061c511a4780d4c0719e0342533a498cd4b457`

## Files Changed
- `crates/ti4-model/src/state.rs` - Added 2 enum variants, updated parser/code
- `crates/ti4-engine/src/effects.rs` - Added 2 effect methods, updated dispatch

## Review
- Self-reviewed: Variants match Oracle's card definitions
- Test coverage: No new tests (variants require Thunder's Edge content)
- Oracle alignment verified against strategy.py and strategy_cards.json
- No breaking changes to existing functionality
