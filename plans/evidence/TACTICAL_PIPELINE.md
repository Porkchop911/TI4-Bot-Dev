# Evidence: Tactical Pipeline Implementation

## Summary
Implemented the tactical pipeline with TacticalManager supporting fleet movement, combat resolution, bombardment, infantry landing, and unit production.

## Commands Run

### Build
```bash
cargo build
```
**Result:** SUCCESS - All 10 crates compile

### Tests
```bash
cargo test -p ti4-engine
```
**Result:** 13/13 tests PASS
- Game loop: 6 tests (start, transitions, agenda, round completion, game over)
- Tactical: 7 tests (activation, deactivation, movement validation, combat score, casualties, bombardment, landing)

## Files Created/Modified

### ti4-engine/src/tactical.rs (NEW)
- TacticalManager struct with activate/deactivate
- move_fleet() - validates distance, fuel, capacity
- resolve_combat() - calculates scores, casualties, determines winner
- bombard() - calculates damage, applies to planet influence
- land_infantry() - lands infantry, tracks via invasion tokens
- produce() - validates production capacity
- Helper methods: calculate_distance, get_max_movement, calculate_fuel_cost, get_capacity
- calculate_combat_score, calculate_casualties, calculate_bombardment_damage
- Result types: MovementResult, CombatResult, BombardmentResult, LandingResult, ProductionResult

## Combat Score Table
| Unit Type | Combat Value |
|-----------|-------------|
| fighter | 1 |
| cruiser | 2 |
| destroyer | 2 |
| carrier | 1 |
| dreadnought | 3 |
| infantry | 1 |
| pds | 2 |
| spacedock | 2 |

## Bombardment Damage
| Unit Type | Damage |
|-----------|--------|
| cruiser | 2 |
| destroyer | 2 |
| dreadnought | 3 |

## Test Coverage
- Player activation/deactivation
- Movement requires activation
- Combat score calculation (3 cruisers = 6)
- Casualty distribution (priority by unit type)
- Bombardment damage calculation (2 cruisers + 1 dreadnought = 7)
- Infantry landing with garrison tracking

## Next Steps
1. Implement strategy phase with card selection
2. Implement command token distribution
3. Implement player activation order
4. Implement agenda voting and resolution
5. Implement objective scoring
6. Implement technology research
7. Implement exploration mechanics
