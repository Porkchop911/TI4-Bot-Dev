/**
 * Types for the ti4-advisor service endpoints.
 */

export interface BattleSideInput {
  player?: string;
  faction?: string;
  units?: Record<string, number> | [string, number][];
  damaged?: Record<string, number> | [string, number][];
  guns?: Record<string, number> | [string, number][];
}

export type BattleParticipant = string | BattleSideInput;

export interface BattleOddsRequest {
  state?: Record<string, unknown>;
  galaxy_layout?: unknown;
  system?: string;
  attacker: BattleParticipant;
  defender: BattleParticipant;
  simulations?: number;
  attacker_cannon?: boolean;
  in_progress?: boolean;
  seed?: number;
}

export interface BattleOddsResponse {
  simulations: number;
  attacker_win_rate: number;
  defender_win_rate: number;
  mutual_destruction_rate: number;
  unresolved_rate: number;
  average_rounds: number;
  attacker_expected_survivors: Record<string, number>;
  defender_expected_survivors: Record<string, number>;
  attacker_fielded: Record<string, number>;
  defender_fielded: Record<string, number>;
}
