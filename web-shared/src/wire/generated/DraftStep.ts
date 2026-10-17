// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ChosenOption } from "./ChosenOption.ts";
import type { PlanStep } from "./PlanStep.ts";

/**
 * One entry of a draft script: an answer by its id, or a step of the action ([`PlanStep`]) as
 * the page sends it to the live game. The steps that follow each other are one plan
 * ([`plan_answer`]); an answer by id between them is for what the game asks there (an ability
 * of the planner before the production).
 */
export type DraftStep = ChosenOption | PlanStep;
