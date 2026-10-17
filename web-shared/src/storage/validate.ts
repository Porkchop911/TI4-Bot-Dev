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
export const EXPORT_VERSION = 2;

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const isInt = (v: unknown, lo: number, hi: number): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi;
const isStr = (v: unknown, max: number): v is string => typeof v === "string" && v.length <= max;
/** Text that will be shown in the UI: no control characters. */
// biome-ignore lint/suspicious/noControlCharactersInRegex: it removes the control characters
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
  if (!isObj(v)) {
    bad("save is not an object");
  }
  if (v.format !== 2) {
    bad(`unsupported save format ${String(v.format)}`);
  }
  if (!isInt(v.seed, 0, Number.MAX_SAFE_INTEGER)) {
    bad("save.seed");
  }
  if (!isInt(v.players, 2, 8)) {
    bad("save.players must be 2 to 8");
  }
  if (!isInt(v.humans, 1, 2 ** (v.players as number) - 1)) {
    bad("save.humans");
  }
  if (!isObj(v.game)) {
    bad("save.game");
  }
  if (!Array.isArray(v.decisions) || v.decisions.length > LIMITS.maxDecisions) {
    bad("save.decisions");
  }
  for (const d of v.decisions) {
    if (!isStr(d, LIMITS.maxAnswerChars)) {
      bad("save.decisions entry");
    }
  }
  if (!Array.isArray(v.answers) || v.answers.length > LIMITS.maxAnswers) {
    bad("save.answers");
  }
  for (const a of v.answers) {
    if (!isStr(a, LIMITS.maxAnswerChars)) {
      bad("save.answers entry");
    }
  }
  if (typeof v.finished !== "boolean") {
    bad("save.finished");
  }
  for (const k of ["redo", "never", "event_times", "event_versions"]) {
    const x = v[k];
    if (x !== undefined && (!Array.isArray(x) || x.length > LIMITS.maxAnswers * 50)) {
      bad(`save.${k}`);
    }
  }
  if (v.round != null && !isInt(v.round, 0, 10000)) {
    bad("save.round");
  }
  if (v.phase != null && !isStr(v.phase, 40)) {
    bad("save.phase");
  }
  if (v.engine !== undefined && !isObj(v.engine)) {
    bad("save.engine");
  }
  if (jsonBytes(v) > LIMITS.maxSaveBytes) {
    throw new StoreError("too-large", "The saved game is too large.");
  }
  return v as HostSave;
}

export function validateEngine(v: unknown): EngineIdentity {
  if (!isObj(v)) {
    bad("engine");
  }
  for (const k of ["build", "contentDigest", "wasmSha256"] as const) {
    if (!isStr(v[k], 200)) {
      bad(`engine.${k}`);
    }
  }
  return {
    build: v.build as string,
    contentDigest: v.contentDigest as string,
    wasmSha256: v.wasmSha256 as string,
  };
}

/** Which parts of two engine identities differ. */
export function engineDifferences(
  saved: EngineIdentity,
  current: EngineIdentity,
): Array<keyof EngineIdentity> {
  const out: Array<keyof EngineIdentity> = [];
  if (saved.build !== current.build) {
    out.push("build");
  }
  if (saved.contentDigest !== current.contentDigest) {
    out.push("contentDigest");
  }
  if (saved.wasmSha256 !== current.wasmSha256) {
    out.push("wasmSha256");
  }
  return out;
}

/** The faction of each seat: `players` entries, each a short string or null. */
export function validateFactions(v: unknown, players: number): Array<string | null> {
  if (!Array.isArray(v) || v.length !== players) {
    bad("factions");
  }
  for (const f of v) {
    if (f !== null && !isStr(f, LIMITS.maxFactionChars)) {
      bad("factions entry");
    }
  }
  return v as Array<string | null>;
}

const validSlot = (v: unknown): v is string => isStr(v, LIMITS.maxSlotChars) && v !== "";

/** Builds the stored record of a save. `now` is the update time. */
export function buildRecord(
  id: string,
  save: HostSave,
  meta: {
    nickname: string;
    factions: Array<string | null>;
    engine: EngineIdentity;
    slot?: string;
    createdAt: number;
    resumeEstimateMs: number | null;
    imported: boolean;
  },
  now: number,
): SavedGameRecord {
  if (!isStr(id, LIMITS.maxIdChars) || id === "") {
    bad("game id");
  }
  if (meta.slot !== undefined && !validSlot(meta.slot)) {
    bad("game slot");
  }
  const nickname = clean(meta.nickname).slice(0, LIMITS.maxNicknameChars);
  return {
    schema: RECORD_SCHEMA,
    id,
    ...(meta.slot !== undefined ? { slot: meta.slot } : {}),
    nickname,
    createdAt: meta.createdAt,
    updatedAt: now,
    status: save.finished ? "finished" : "in_progress",
    summary: {
      playerCount: save.players,
      humans: save.humans,
      factions: validateFactions(meta.factions, save.players),
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
  if (!isObj(v)) {
    bad("record");
  }
  if (v.schema !== RECORD_SCHEMA) {
    bad(`record schema ${String(v.schema)}`);
  }
  if (!isStr(v.id, LIMITS.maxIdChars) || v.id === "") {
    bad("record.id");
  }
  if (v.slot !== undefined && !validSlot(v.slot)) {
    bad("record.slot");
  }
  if (!isStr(v.nickname, LIMITS.maxNicknameChars)) {
    bad("record.nickname");
  }
  if (typeof v.createdAt !== "number" || typeof v.updatedAt !== "number") {
    bad("record times");
  }
  if (v.status !== "in_progress" && v.status !== "finished") {
    bad("record.status");
  }
  const summary = v.summary;
  if (!isObj(summary)) {
    bad("record.summary");
  }
  const engine = validateEngine(v.engine);
  const save = validateSave(v.save);
  if (summary.playerCount !== save.players || summary.humans !== save.humans) {
    bad("record.summary seats");
  }
  validateFactions(summary.factions, save.players);
  return { ...(v as unknown as SavedGameRecord), engine, save };
}

export interface ExportFile {
  kind: typeof EXPORT_KIND;
  version: typeof EXPORT_VERSION;
  exportedAt: number;
  game: {
    nickname: string;
    /** The faction of each seat, null while it is not known. */
    factions: Array<string | null>;
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
      factions: rec.summary.factions,
      createdAt: rec.createdAt,
      engine: rec.engine,
      save: rec.save,
    },
  };
}

/** Parses the text of an import file. Throws StoreError for anything that is not exactly an export of this version. */
export function parseExport(text: string): ExportFile["game"] {
  if (text.length > LIMITS.maxImportBytes) {
    throw new StoreError("too-large", "The file is too large.");
  }
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    bad("The file is not valid JSON.");
  }
  if (!isObj(raw) || raw.kind !== EXPORT_KIND) {
    bad("This is not an exported offline game.");
  }
  if (raw.version !== EXPORT_VERSION) {
    bad(
      `Unsupported file version ${String(raw.version)} (this build reads version ${EXPORT_VERSION}).`,
    );
  }
  const game = raw.game;
  if (!isObj(game)) {
    bad("game");
  }
  if (!isStr(game.nickname, 200)) {
    bad("game.nickname");
  }
  const createdAt = game.createdAt;
  if (typeof createdAt !== "number" || !Number.isFinite(createdAt)) {
    bad("game.createdAt");
  }
  const save = validateSave(game.save);
  return {
    nickname: game.nickname as string,
    factions: validateFactions(game.factions, save.players),
    createdAt: createdAt as number,
    engine: validateEngine(game.engine),
    save,
  };
}
