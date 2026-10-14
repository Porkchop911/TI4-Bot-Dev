/** Validation of everything that comes from outside the running game: stored records and imported files. */
import {
  type EngineIdentity,
  type HostSave,
  LIMITS,
  RECORD_SCHEMA,
  type SavedGameRecord,
  StoreError,
} from "./types.ts";

export const EXPORT_KIND = "ti4-offline-game";
export const EXPORT_VERSION = 1;

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const isInt = (v: unknown, lo: number, hi: number): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi;
const isStr = (v: unknown, max: number): v is string => typeof v === "string" && v.length <= max;
/** Text that will be shown in the UI: no control characters. */
const clean = (s: string) => s.replace(/[\u0000-\u001f\u007f]/g, " ").trim();

function bad(message: string): never {
  throw new StoreError("invalid", message);
}

/** Byte size of the JSON text of `value`. */
export function jsonBytes(value: unknown): number {
  return new TextEncoder().encode(JSON.stringify(value)).length;
}

/** Checks the fields the store and the shell rely on; leaves the rest to the engine (which validates a resume itself). */
export function validateSave(v: unknown): HostSave {
  if (!isObj(v)) bad("save is not an object");
  if (v.format !== 1) bad(`unsupported save format ${String(v.format)}`);
  if (!isStr(v.game_id, LIMITS.maxIdChars)) bad("save.game_id");
  if (!isInt(v.seed, 0, Number.MAX_SAFE_INTEGER)) bad("save.seed");
  if (!isInt(v.players, 3, 6)) bad("save.players must be 3 to 6");
  if (!isInt(v.human_seat, 0, (v.players as number) - 1)) bad("save.human_seat");
  if (!isObj(v.setup)) bad("save.setup");
  if (!Array.isArray(v.answers) || v.answers.length > LIMITS.maxAnswers) bad("save.answers");
  for (const a of v.answers) if (!isStr(a, LIMITS.maxAnswerChars)) bad("save.answers entry");
  if (typeof v.finished !== "boolean") bad("save.finished");
  for (const k of ["redo", "never", "event_times", "event_versions"]) {
    const x = v[k];
    if (x !== undefined && (!Array.isArray(x) || x.length > LIMITS.maxAnswers * 50)) bad(`save.${k}`);
  }
  if (v.round != null && !isInt(v.round, 0, 10000)) bad("save.round");
  if (v.phase != null && !isStr(v.phase, 40)) bad("save.phase");
  if (v.engine !== undefined && !isObj(v.engine)) bad("save.engine");
  if (jsonBytes(v) > LIMITS.maxSaveBytes) throw new StoreError("too-large", "The saved game is too large.");
  return v as HostSave;
}

export function validateEngine(v: unknown): EngineIdentity {
  if (!isObj(v)) bad("engine");
  for (const k of ["build", "contentDigest", "wasmSha256"] as const) {
    if (!isStr(v[k], 200)) bad(`engine.${k}`);
  }
  return { build: v.build as string, contentDigest: v.contentDigest as string, wasmSha256: v.wasmSha256 as string };
}

/** Builds the stored record of a save. `now` is the update time. */
export function buildRecord(
  id: string,
  save: HostSave,
  meta: {
    nickname: string;
    humanFaction: string | null;
    engine: EngineIdentity;
    createdAt: number;
    resumeEstimateMs: number | null;
    imported: boolean;
  },
  now: number,
): SavedGameRecord {
  if (!isStr(id, LIMITS.maxIdChars) || id === "") bad("game id");
  const nickname = clean(meta.nickname).slice(0, LIMITS.maxNicknameChars);
  return {
    schema: RECORD_SCHEMA,
    id,
    nickname,
    createdAt: meta.createdAt,
    updatedAt: now,
    status: save.finished ? "finished" : "in_progress",
    summary: {
      playerCount: save.players,
      humanSeat: save.human_seat + 1,
      humanFaction: meta.humanFaction,
      seed: save.seed,
      round: save.round ?? null,
      phase: save.phase ?? null,
      answers: save.answers.length,
    },
    engine: meta.engine,
    resumeEstimateMs: meta.resumeEstimateMs,
    imported: meta.imported,
    sizeBytes: jsonBytes(save),
    save,
  };
}

/** A record read from the database. Throws StoreError("invalid") for anything unexpected (including newer schemas). */
export function validateRecord(v: unknown): SavedGameRecord {
  if (!isObj(v)) bad("record");
  if (v.schema !== RECORD_SCHEMA) bad(`record schema ${String(v.schema)}`);
  if (!isStr(v.id, LIMITS.maxIdChars) || v.id === "") bad("record.id");
  if (!isStr(v.nickname, LIMITS.maxNicknameChars)) bad("record.nickname");
  if (typeof v.createdAt !== "number" || typeof v.updatedAt !== "number") bad("record times");
  if (v.status !== "in_progress" && v.status !== "finished") bad("record.status");
  if (!isObj(v.summary)) bad("record.summary");
  const engine = validateEngine(v.engine);
  const save = validateSave(v.save);
  return { ...(v as unknown as SavedGameRecord), engine, save };
}

export interface ExportFile {
  kind: typeof EXPORT_KIND;
  version: typeof EXPORT_VERSION;
  exportedAt: number;
  game: {
    nickname: string;
    humanFaction: string | null;
    createdAt: number;
    engine: EngineIdentity;
    save: HostSave;
  };
}

export function buildExport(rec: SavedGameRecord, now: number): ExportFile {
  return {
    kind: EXPORT_KIND,
    version: EXPORT_VERSION,
    exportedAt: now,
    game: {
      nickname: rec.nickname,
      humanFaction: rec.summary.humanFaction,
      createdAt: rec.createdAt,
      engine: rec.engine,
      save: rec.save,
    },
  };
}

/** Parses the text of an import file. Throws StoreError for anything that is not exactly an export of this version. */
export function parseExport(text: string): ExportFile["game"] {
  if (text.length > LIMITS.maxImportBytes) throw new StoreError("too-large", "The file is too large.");
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    bad("The file is not valid JSON.");
  }
  if (!isObj(raw) || raw.kind !== EXPORT_KIND) bad("This is not an exported offline game.");
  if (raw.version !== EXPORT_VERSION) {
    bad(`Unsupported file version ${String(raw.version)} (this build reads version ${EXPORT_VERSION}).`);
  }
  const game = raw.game;
  if (!isObj(game)) bad("game");
  if (!isStr(game.nickname, 200)) bad("game.nickname");
  if (game.humanFaction !== null && !isStr(game.humanFaction, 80)) bad("game.humanFaction");
  const createdAt = game.createdAt;
  if (typeof createdAt !== "number" || !Number.isFinite(createdAt)) bad("game.createdAt");
  return {
    nickname: game.nickname as string,
    humanFaction: (game.humanFaction as string | null) ?? null,
    createdAt: createdAt as number,
    engine: validateEngine(game.engine),
    save: validateSave(game.save),
  };
}
