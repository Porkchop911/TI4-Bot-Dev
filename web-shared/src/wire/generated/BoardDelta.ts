// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActiveSystemChange } from "./ActiveSystemChange.ts";
import type { CombatChange } from "./CombatChange.ts";
import type { GroundStepsChange } from "./GroundStepsChange.ts";
import type { InvasionChange } from "./InvasionChange.ts";
import type { PlanetChange } from "./PlanetChange.ts";
import type { PoolsChange } from "./PoolsChange.ts";
import type { SpaceRoundsChange } from "./SpaceRoundsChange.ts";
import type { SystemChange } from "./SystemChange.ts";
import type { TokensChange } from "./TokensChange.ts";
import type { UnitChange } from "./UnitChange.ts";

/**
 * What one step changed of the public board. Each part is there only when it changed.
 */
export type BoardDelta = { systems?: Array<SystemChange>, units?: Array<UnitChange>, planets?: Array<PlanetChange>, tokens?: Array<TokensChange>, pools?: Array<PoolsChange>, active_system?: ActiveSystemChange, combat?: CombatChange, invasion?: InvasionChange, space_rounds?: SpaceRoundsChange, ground_steps?: GroundStepsChange, };
