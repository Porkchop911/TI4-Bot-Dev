// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CombatFact } from "./CombatFact.ts";
import type { Unavailable } from "./Unavailable.ts";

/**
 * One kind of unit of a production, also one that the seat cannot produce now (`unavailable`).
 */
export type BuildFact = { unit: string, 
/**
 * The printed cost of one purchase, before a discount.
 */
cost: number, 
/**
 * How many units one purchase makes: 2 for fighters and infantry.
 */
per: number, 
/**
 * How many units of this kind are left in the reinforcements. `None`: there is no limit.
 */
supply?: number, 
/**
 * The unit is a ship that counts against the fleet pool.
 */
fleet?: boolean, 
/**
 * The unit uses capacity while it is in a space area: a fighter, a ground force.
 */
cargo?: boolean, 
/**
 * The unit is a fighter: in the space area it counts as a fighter of
 * [`ti4_engine::fleet::FleetCounts`], else as a ground force that is carried.
 */
fighter?: boolean, 
/**
 * Where the unit can be placed: `space`, or a planet of the system.
 */
places: Array<string>, 
/**
 * What the unit hits on in a combat, and with how many dice. `None`: it does not fight.
 */
combat?: CombatFact, 
/**
 * The move value. 0: the unit does not move by itself.
 */
move_value: number, 
/**
 * What the unit carries.
 */
capacity: number, 
/**
 * The keyword abilities of the unit, as printed: "SUSTAIN DAMAGE", "BOMBARDMENT 5+",
 * "PRODUCTION +2".
 */
abilities?: Array<string>, 
/**
 * The special rule that is printed on the unit. `None`: it has none.
 */
rules?: string, 
/**
 * Why the seat cannot produce the unit here now. `None`: it can.
 */
unavailable?: Unavailable, };
