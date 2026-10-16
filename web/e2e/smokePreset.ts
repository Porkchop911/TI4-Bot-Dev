/** Start presets the server knows (crates/ti4-server/src/preset.rs). */
export const KNOWN_PRESETS = ["combat", "cards", "agenda", "relics", "invasion", "techs", "leaders", "endgame", "siege", "bombard", "capture", "explore", "notes", "world"] as const;

/** The preset without its `+rot`, `+short`, `+agenda` and `+fac[:alias...]` suffixes. */
export function basePreset(name: string | undefined): string {
  return (name ?? "").split("+")[0];
}

/**
 * Reads TI4_SMOKE_PRESET: unset or empty means a normal opening; an unknown name is an error.
 * Suffixes, in any order: "+rot" also rotates the factions (Jol-Nar and L1Z1X at small tables;
 * "leaders" always does), "+fac" seats factions from the engine's roster by seed ("+fac:naalu:mentak"
 * names the first seats; "world" always does), "+short" starts the game near its end, "+agenda" removes the custodians at the start so every round has an agenda phase.
 */
export function presetFromEnv(value: string | undefined): string | undefined {
  const name = value?.trim();
  if (!name) return undefined;
  const [base, ...suffixes] = name.split("+");
  const suffixOk = (s: string) => s === "rot" || s === "short" || s === "agenda" || /^fac(:[a-z0-9_]+)*$/.test(s);
  if (!(KNOWN_PRESETS as readonly string[]).includes(base) || !suffixes.every(suffixOk)) {
    throw new Error(`unknown TI4_SMOKE_PRESET "${name}" (known: ${KNOWN_PRESETS.join(", ")})`);
  }
  return name;
}

/** Strategy card sets the create form offers (`card_set::OFFERED`). */
export const KNOWN_CARD_SETS = ["te", "pok", "base_game_codex1"] as const;

/** Reads TI4_SMOKE_CARD_SET: unset or empty means the server default; an unknown name is an error. */
export function cardSetFromEnv(value: string | undefined): string | undefined {
  const name = value?.trim();
  if (!name) return undefined;
  if (!(KNOWN_CARD_SETS as readonly string[]).includes(name)) {
    throw new Error(`unknown TI4_SMOKE_CARD_SET "${name}" (known: ${KNOWN_CARD_SETS.join(", ")})`);
  }
  return name;
}

/** Reads TI4_SMOKE_MAP_TEMPLATE: unset or empty means the server's default map for the table size. */
export function mapTemplateFromEnv(value: string | undefined): string | undefined {
  const name = value?.trim();
  return name || undefined;
}

/** The POST /api/games body; `start_preset` is only sent when a preset was asked for. */
export function createGameBody(
  playerCount: number,
  seed: number,
  preset?: string,
  cardSet?: string,
  mapTemplate?: string,
) {
  return {
    player_count: playerCount,
    seed,
    nickname: "E2E Host",
    ...(preset ? { start_preset: preset } : {}),
    // Left out for the server default (te); the nightly sets pok on part of the runs.
    ...(cardSet ? { strategy_card_set: cardSet } : {}),
    // A named map (e.g. "3pInPersonHyperlanes", the only default-size map with frontier systems).
    ...(mapTemplate ? { map_template: mapTemplate } : {}),
  };
}

/**
 * What a run must have offered. `TI4_SMOKE_EXPECT` is a comma list; each item is a decision
 * subtype, `a|b|c` (at least one of them) or `a|b|c>=N` (at least N distinct of them), and the
 * word `preset` stands for the start preset's own list (`PRESET_EXPECT`). Unset means no check.
 */
export interface Expectation {
  subtypes: string[];
  atLeast: number;
}

/** The subtypes each preset exists to reach; asserted when `TI4_SMOKE_EXPECT=preset`. */
export const PRESET_EXPECT: Record<string, string> = {
  // Thunder's Edge cards dealt to hands: their pick prompts, and the "action card played" window.
  cards:
    "overrule_pick_strategy card|strategize1_pick_strategy card|exchange_program_answer|exchangeprogram_pick_player|exchangeprogram_pick_planet>=2,reaction_when_ACTION_CARD_PLAYED",
  // The agenda phase ran and somebody voted; a window or pick of the dealt agenda cards fired.
  agenda:
    "cast_vote,play_reaction_after_AGENDA_REVEALED|predict_agenda_outcome|bribery_pick_count|assassin_pick_player>=2",
  // Relic prompts: purge to move, explore with the Crown, Neuraloop, JR-XS455-O, Heart of Ixth.
  relics:
    "crown_of_emphidia_choose_planet|dominus_orb_purge_to_move|neuraloop_choose_relic_to_purge|titan_prototype_choose_builder|stellar_converter_choose_target|heart_ixth_die_adjust>=3",
  // An invasion of a defended colony: landing and ground combat rounds. The two PDS's space cannon
  // usually costs the invader a sustain or a ship, but that is dice, so it is not asserted.
  // (assign_ground_casualty is never asked: the server's timing path assigns ground hits itself,
  // and bombardment_target is coexistence-only.)
  invasion: "commit_ground_forces,fight_ground_combat_round",
  // Every leader usable from the start with rotated factions: agent, hero and commander prompts.
  // The game ended: `game_over` is a pseudo-subtype the harness adds when it sees that status.
  endgame: "game_over",
  leaders: "leader_hacanagent_branch|leader_xxchaagent_ready_planet|leader_l1z1xagent_copy_planet|leader_jolnarhero_swap|leader_l1z1xhero_destination|leader_hacanhero_free_production|leader_xxchahero_te_place|legendary_arms_vault|legendary_end_of_turn|legendary_place>=3",
  // Garrisoned PDS colonies: the Assault Cannon loss, a combat window of the dealt cards (barrage,
  // space cannon hits, hits to assign, Courageous) and a hit assignment. Barrage and cannon rolls
  // are dice, so only their windows are asserted, and only one of them.
  siege:
    "assault_cannon_destroy,reaction_when_ANTI_FIGHTER_BARRAGE_STARTED|reaction_when_SPACE_CANNON_HITS|reaction_when_HITS_TO_ASSIGN|courageous_to_the_end_assign_casualty,assign_casualty|sustain_damage",
  // Bombardment on a planet shared by two defenders asks whose units take the hits.
  bombard: "bombardment_target,commit_ground_forces,fight_ground_combat_round",
  // The Cabal captures from reinforcements (Vortex, the agent's Stillness of Stars).
  capture: "vortex_target|stillness_target",
  // The invasion setup with every seat owning the prompt-bearing technologies.
  techs:
    "quantum_datahub_swap|spatial_conduit_link|nullification_field_end_turn|chaos_mapping_choose_system|bio_stims_ready|psychoarchaeology_exhaust_specialty|transit_diodes_redeploy|supercharge|scanlink_explore>=5",
  // Exploration: a planet or frontier card that asks a question (the subtype is "<card name>_choose_reward").
  explore:
    "merchant_station_choose_reward|abandoned_warehouses_choose_reward|functioning_base_choose_reward|local_fabricators_choose_reward|mercenary_outfit_choose_reward|core_mine_choose_reward|expedition_choose_reward|volatile_fuel_source_choose_reward|ion_storm_choose_reward",
  // Promissory notes in foreign hands: the trade desk offers them, Political Secret asks, notes are given.
  notes: "propose_transaction,ps|give_note|commander_give_note|reaction_after_STRATEGY_PHASE_ENDED",
  // Roster factions at war from round one (their own decisions differ per table; the fights do not).
  world: "fight_ground_combat_round",
};

export function parseExpect(value: string | undefined, preset?: string): Expectation[] {
  const text = value?.trim();
  if (!text) return [];
  return text
    .split(",")
    .flatMap((item) =>
      item.trim() === "preset" ? (PRESET_EXPECT[basePreset(preset)] ?? "").split(",") : [item],
    )
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => {
      const match = /^(.*?)(?:>=(\d+))?$/.exec(item);
      return {
        subtypes: (match?.[1] ?? item).split("|").map((s) => s.trim()),
        atLeast: Number(match?.[2] ?? 1),
      };
    });
}

/** The expectations that `seen` (report.subtypes) does not meet, as readable text. */
export function missingExpected(seen: Record<string, number>, expected: Expectation[]): string[] {
  return expected.flatMap(({ subtypes, atLeast }) => {
    const got = subtypes.filter((s) => (seen[s] ?? 0) > 0).length;
    return got >= atLeast ? [] : [`${subtypes.join("|")} (need ${atLeast}, saw ${got})`];
  });
}
