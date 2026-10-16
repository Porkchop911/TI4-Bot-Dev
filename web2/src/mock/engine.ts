// @ts-nocheck
// The scripted rules engine of the click dummy.
// It is not type-checked on purpose: it is throwaway code that the server replaces. Typed code
// (selectors, the reducer) reads its state and calls its functions.
import {
  ANOMALY,
  DECISIONS,
  FACES,
  KEYS,
  LOSS_ORDER,
  MAP,
  MORALE,
  NEAR,
  ADJ,
  P,
  PAY,
  RETREATS,
  SCRIPTED,
  SEAT,
  SEATS,
  SIDES,
  SIM,
  SPECS,
  STEPS,
  STOCK,
  STRATEGY_SIM,
  SUPPLY,
  U,
  baseData,
  blankMine,
  clone,
  examples,
  LINES,
  lineById,
  linesOf,
  other,
  plural,
} from "./data";

/** What the dummy keeps outside a workspace: the seat that looks at the screen, and messages to show. */
export const env = { viewer: "sol", toasts: [] as string[], neverOffer: [] as string[] };

export const dig = (object, path) => path.split(".").reduce((value, key) => value?.[key], object);
export function put(object, path, value) {
  const keys = path.split(".");
  const last = keys.pop();
  keys.reduce((value, key) => (value[key] ||= {}), object)[last] = value;
}
// Live dice follow the fixed script above; odds use a seeded generator so they never jitter.
export function nextFace(source) {
  if (source.s === undefined) {
    return FACES[source.i++ % FACES.length];
  }
  source.s = (source.s + 0x6d2b79f5) | 0;
  let t = Math.imul(source.s ^ (source.s >>> 15), 1 | source.s);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return 1 + Math.floor((((t ^ (t >>> 14)) >>> 0) / 4294967296) * 10);
}
export const unitName = (type, count = 1) =>
  count === 1 ? U[type].name : U[type].plural || U[type].name + "s";
export const types = (force) => Object.keys(U).filter((type) => force[type]?.n);
export const force = (counts, damaged = {}) =>
  Object.fromEntries(
    Object.keys(U)
      .filter((type) => counts[type] > 0)
      .map((type) => [type, { n: counts[type], dmg: damaged[type] || 0 }]),
  );
export const size = (group, test = () => true) =>
  types(group).reduce((sum, type) => sum + (test(type) ? group[type].n : 0), 0);
export const ships = (group) => size(group, (type) => U[type].ship);
export const capacityOf = (group) =>
  types(group).reduce((sum, type) => sum + group[type].n * (U[type].capacity || 0), 0);
export const absorb = (group) =>
  types(group).reduce(
    (sum, type) => sum + group[type].n + (U[type].sustain ? group[type].n - group[type].dmg : 0),
    0,
  );
export const list = (group) =>
  types(group)
    .map(
      (type) =>
        `${group[type].n} ${unitName(type, group[type].n)}${group[type].dmg ? ` (${group[type].n > 1 ? group[type].dmg + " " : ""}damaged)` : ""}`,
    )
    .join(" · ") || "None";
export function add(group, type, count) {
  (group[type] ||= { n: 0, dmg: 0 }).n += count;
}
export function drop(group, type) {
  const unit = group[type];
  if (unit.dmg === unit.n) {
    unit.dmg--;
  }
  unit.n--;
}
export function roll(source, group, spec, mod = 0) {
  const result = { dice: {}, targets: {}, hits: 0 };
  for (const type of types(group)) {
    const rule = spec(type);
    if (!rule) {
      continue;
    }
    result.targets[type] = rule.target;
    result.dice[type] = Array.from({ length: group[type].n * (rule.dice || 1) }, () => {
      const face = nextFace(source);
      const hit = face + mod >= rule.target;
      result.hits += hit;
      return { face, hit };
    });
  }
  return result;
}
export function record(key, label, forces) {
  return {
    key,
    label,
    start: clone(forces),
    dice: { att: {}, def: {} },
    targets: { att: {}, def: {} },
    hits: { att: 0, def: 0 },
    lost: { att: {}, def: {} },
    dmg: { att: {}, def: {} },
    mod: {},
    notes: [],
  };
}
export function setRoll(rec, side, result) {
  rec.dice[side] = result.dice;
  rec.targets[side] = result.targets;
  rec.hits[side] = result.hits;
}
// The same record without dice: what will roll, at which value.
export function preview(label, forces, specs, boost = {}) {
  const rec = record("next", label, forces);
  rec.pre = true;
  rec.count = { att: {}, def: {} };
  for (const side of SIDES) {
    if (boost[side]) {
      rec.mod[side] = MORALE;
    }
    for (const type of types(forces[side])) {
      const rule = specs[side]?.(type);
      if (!rule) {
        continue;
      }
      rec.targets[side][type] = rule.target;
      rec.count[side][type] = forces[side][type].n * (rule.dice || 1);
    }
  }
  return rec;
}
// Default casualty policy for the scripted opponent: sustain first, then lose the cheapest units.
export function autoAssign(group, hits, only) {
  const left = clone(group);
  const actions = [];
  if (!only) {
    for (const type of types(left)) {
      while (hits > actions.length && U[type].sustain && left[type].n > left[type].dmg) {
        left[type].dmg++;
        actions.push({ type, kind: "sustain" });
      }
    }
  }
  for (const type of LOSS_ORDER) {
    while (hits > actions.length && left[type]?.n && (!only || only === type)) {
      const damaged = left[type].dmg > 0;
      left[type].n--;
      if (damaged) {
        left[type].dmg--;
      }
      actions.push({ type, kind: damaged ? "destroy-damaged" : "destroy" });
    }
  }
  return actions;
}
export function applyActions(group, actions, rec, side) {
  for (const action of actions) {
    const unit = group[action.type];
    if (action.kind === "sustain") {
      unit.dmg++;
      if (rec) {
        rec.dmg[side][action.type] = (rec.dmg[side][action.type] || 0) + 1;
      }
    } else {
      unit.n--;
      if (action.kind === "destroy-damaged") {
        unit.dmg--;
      }
      if (rec) {
        rec.lost[side][action.type] = (rec.lost[side][action.type] || 0) + 1;
      }
    }
  }
  return group;
}

// ---- Battles: one engine for space and ground combat ----------------------------------------
export const controls = (state, side) =>
  !state.auto && state.mode === "live" && env.viewer === SEAT[side];
export const alive = (battle, side) => size(battle.forces[side]);
export function newBattle(kind, att, def, extra = {}) {
  const forces = { att, def };
  return {
    kind,
    forces,
    initial: clone(forces),
    types: { att: types(att), def: types(def) },
    records: [],
    round: 1,
    stage: "pre",
    owed: { att: 0, def: 0 },
    staged: [],
    announced: {},
    boost: {},
    // The window at the start of a round: the staged card, and the round that was answered.
    start: { pick: null, asked: 0 },
    i: 0,
    ...extra,
  };
}
export function barrage(battle) {
  const shots = SIDES.map((side) =>
    roll(
      battle.source || battle,
      battle.forces[side],
      (type) => battle.forces[other(side)].fighter?.n && SPECS.afb(type),
    ),
  );
  if (!shots.some((shot) => Object.keys(shot.dice).length)) {
    return;
  }
  const rec = record("afb", "Barrage", battle.forces);
  SIDES.forEach((side, index) => {
    setRoll(rec, side, shots[index]);
  });
  for (const side of SIDES) {
    applyActions(
      battle.forces[other(side)],
      autoAssign(battle.forces[other(side)], rec.hits[side], "fighter"),
      rec,
      other(side),
    );
  }
  rec.hint = "Anti-fighter barrage hits destroy opposing fighters automatically.";
  battle.records.push(rec);
}
/** The side of the viewer while their window at the start of a combat round is open, or null. */
export function startWindow(state, battle) {
  const side = SIDES.find((name) => controls(state, name));
  return battle?.stage === "pre" &&
    battle.kind === "space" &&
    side &&
    battle.start.asked !== battle.round &&
    state.hands[SEAT[side]].includes("Morale Boost") &&
    !env.neverOffer.includes("Morale Boost")
    ? side
    : null;
}
export function battleRoll(state, battle) {
  const rec = record(`r${battle.round}`, `Round ${battle.round}`, battle.forces);
  for (const side of SIDES) {
    if (battle.boost[side]) {
      rec.mod[side] = MORALE;
    }
    setRoll(
      rec,
      side,
      roll(
        battle.source || battle,
        battle.forces[side],
        SPECS[battle.kind === "ground" ? "ground" : "combat"],
        battle.boost[side] ? 1 : 0,
      ),
    );
  }
  battle.boost = {};
  battle.records.push(rec);
  battle.owed = {
    att: Math.min(rec.hits.def, absorb(battle.forces.att)),
    def: Math.min(rec.hits.att, absorb(battle.forces.def)),
  };
  battle.stage = "hits";
  battleContinue(state, battle);
}
// Resolves everything the scripted side owes, and stops whenever the viewer has a decision.
export function battleContinue(state, battle) {
  const rec = battle.records.at(-1);
  const order = SIDES.slice().sort((a, b) => controls(state, a) - controls(state, b));
  for (const side of order) {
    if (!battle.owed[side]) {
      continue;
    }
    battle.side = side;
    const group = battle.forces[side],
      kinds = types(group);
    // A loss with only one legal outcome resolves without a prompt.
    const forced =
      battle.owed[side] >= absorb(group) || (kinds.length === 1 && !U[kinds[0]].sustain);
    if (controls(state, side) && !forced) {
      battle.stage = "assign";
      battle.staged = [];
      return;
    }
    if (battle.pause && side === "def" && !controls(state, side)) {
      battle.pause = false;
      battle.stage = "waiting";
      return;
    }
    const actions = autoAssign(battle.forces[side], battle.owed[side]);
    applyActions(battle.forces[side], actions, rec, side);
    battle.owed[side] = 0;
    const sustained = actions.find((action) => action.kind === "sustain");
    const foe = other(side);
    if (
      sustained &&
      battle.kind === "space" &&
      controls(state, foe) &&
      state.hands[SEAT[foe]].includes("Direct Hit")
    ) {
      // The viewer set this card to "Never offer": the window does not open, and they are told.
      if (env.neverOffer.includes("Direct Hit")) {
        env.toasts.push("Direct Hit was not offered: it is set to Never offer.");
      } else {
        battle.stage = "reaction";
        battle.reaction = { side: foe, target: side, type: sustained.type, pick: null };
        return;
      }
    }
  }
  endRound(state, battle);
}
export function endRound(state, battle) {
  for (const side of SIDES) {
    if (!battle.announced[side] || !alive(battle, side) || battle.retreat) {
      continue;
    }
    if (controls(state, side) && !battle.destination) {
      battle.stage = "retreat";
      battle.side = side;
      battle.pick = null;
      return;
    }
    battle.retreat = {
      side,
      to: sysLabel(battle.destination || RETREATS[side][0]),
      units: clone(battle.forces[side]),
    };
    for (const type of types(battle.forces[side])) {
      battle.forces[side][type] = { n: 0, dmg: 0 };
    }
    battle.records.at(-1).notes.push(`${P[SEAT[side]].name} retreated to ${battle.retreat.to}.`);
  }
  if (battle.retreat || !alive(battle, "att") || !alive(battle, "def")) {
    return finishBattle(state, battle);
  }
  battle.round++;
  battle.stage = "pre";
  // Scripted opponent: an outnumbered defender announces a retreat.
  if (
    battle.kind === "space" &&
    !battle.sim &&
    !controls(state, "def") &&
    !battle.announced.att &&
    alive(battle, "def") < alive(battle, "att")
  ) {
    battle.announced.def = true;
  }
  if (battle.kind === "ground" && !battle.sim) {
    battleRoll(state, battle);
  }
}
export function finishBattle(state, battle) {
  const att = alive(battle, "att"),
    def = alive(battle, "def");
  battle.stage = "done";
  battle.winner = battle.retreat
    ? other(battle.retreat.side)
    : att && !def
      ? "att"
      : def && !att
        ? "def"
        : null;
  if (battle.sim) {
    return;
  }
  if (battle.kind === "space") {
    afterCombat(state);
  } else {
    afterGround(state, battle);
  }
}
export const oddsCache = new Map();
export function odds(kind, att, def) {
  const key = JSON.stringify([kind, att, def]);
  if (!oddsCache.has(key)) {
    const runs = 400,
      source = { s: 20261006 },
      total = { att: 0, def: 0, none: 0, rounds: 0, attLeft: 0, defLeft: 0 };
    for (let run = 0; run < runs; run++) {
      const battle = newBattle(kind, clone(att), clone(def), { sim: true, source });
      if (kind === "space") {
        barrage(battle);
      }
      if (!alive(battle, "att") || !alive(battle, "def")) {
        finishBattle(null, battle);
      }
      while (battle.stage === "pre" && battle.round < 40) {
        battleRoll({ auto: true }, battle);
      }
      total[battle.winner || "none"]++;
      total.rounds += battle.round;
      total.attLeft += alive(battle, "att");
      total.defLeft += alive(battle, "def");
    }
    oddsCache.set(key, {
      runs,
      att: Math.round((total.att / runs) * 100),
      def: Math.round((total.def / runs) * 100),
      rounds: (total.rounds / runs).toFixed(1),
      attLeft: (total.attLeft / runs).toFixed(1),
      defLeft: (total.defLeft / runs).toFixed(1),
    });
  }
  return oddsCache.get(key);
}

// ---- The action pipeline: the same transitions drive Draft and Live -------------------------
export const landing = (data, planet, type) => data[`${planet}:${type}`] || 0;
export const landed = (state, planet, data = state.data.invasion) =>
  force({ infantry: landing(data, planet, "infantry"), mech: landing(data, planet, "mech") });
export const hostile = (planet) => planet.owner === "hacan";
export const shielded = (planet) => planet.structures.includes("pds");
// ---- Map and movement rules -----------------------------------------------------------------
export const activeId = (state) => state.data.activation.system;
export const garrison = (state) =>
  ["hostile", "cannon"].includes(state.route) ? { dreadnought: 1, destroyer: 1, fighter: 1 } : null;
// Ships of the other players in a system: { seat: { type: count } }.
export const others = (state, id) =>
  id === activeId(state)
    ? size(state.enemyFleet)
      ? {
          hacan: Object.fromEntries(
            types(state.enemyFleet).map((type) => [type, state.enemyFleet[type].n]),
          ),
        }
      : {}
    : id === "27"
      ? garrison(state)
        ? { hacan: garrison(state) }
        : {}
      : MAP[id].fleets || {};
export const planetsOf = (state, id) =>
  id === activeId(state) && state.kind === "tactical" ? state.planets : MAP[id].planets || [];
export const sysLabel = (id) => `${MAP[id].name} · #${id}`;
export const adjacent = (id) => ADJ[id];
export const neighbours = (id) => NEAR[id];
export const rifts = (path) => path.slice(0, -1).filter((id) => MAP[id].anomaly === "rift");
export const cargoAt = (state, id) =>
  linesOf(state).filter((line) => line.origin === id && !U[line.type].ship);
export function blocker(state, id) {
  const anomaly = MAP[id].anomaly;
  if (anomaly === "asteroid" || anomaly === "supernova") {
    return `${ANOMALY[anomaly].label} #${id} blocks the route`;
  }
  if (anomaly === "nebula") {
    return `Nebula #${id} cannot be moved through`;
  }
  const seat = Object.keys(others(state, id))[0];
  return seat ? `${SEATS[seat].faction} ships in #${id} block the route` : "";
}
// Every route from origin to the active system within the move value. `free` ignores blockers, to name them.
export function routes(state, origin, move, free = false) {
  const active = activeId(state),
    found = [];
  if (origin === active || (MAP[origin].token && !free)) {
    return found;
  }
  const walk = (path, allowance) => {
    const here = path.at(-1);
    if (here === active) {
      return found.push(path);
    }
    if (MAP[here].anomaly === "rift") {
      allowance++;
    }
    if (allowance - (path.length - 1) <= 0) {
      return;
    }
    for (const next of neighbours(here)) {
      if (!path.includes(next) && (next === active || free || !blocker(state, next))) {
        walk([...path, next], allowance);
      }
    }
  };
  walk([origin], MAP[origin].anomaly === "nebula" ? 1 : move);
  return found.sort((a, b) => a.length - b.length || rifts(a).length - rifts(b).length);
}
export function hops(state, origin) {
  let ring = [origin],
    count = 0;
  const seen = new Set(ring);
  while (ring.length && !seen.has(activeId(state))) {
    ring = ring.flatMap((id) => neighbours(id)).filter((id) => !seen.has(id) && seen.add(id));
    count++;
  }
  return count;
}
export function whyNot(state, line, base) {
  if (state.stale && line.id === "j-cruiser") {
    return `No longer at ${MAP[line.origin].name}`;
  }
  if (MAP[line.origin].token) {
    return "Your command token is here";
  }
  const open = routes(state, line.origin, base, true)[0];
  if (open) {
    return blocker(
      state,
      open.slice(1, -1).find((id) => blocker(state, id)),
    );
  }
  return `${MAP[line.origin].anomaly === "nebula" ? `Move ${base} → 1 in the nebula` : `Move ${base}`} · ${plural(hops(state, line.origin), "system")} away`;
}
export const moveCache = new Map();
// How many of the player's ships can move to a system, before it is activated.
export function reach(state, id) {
  const probe = { ...state, data: { ...state.data, activation: { system: id } } };
  const lines = linesOf(state).filter(
    (line) => U[line.type].ship && moveInfo(probe, line).routes.length,
  );
  return {
    ships: lines.reduce((sum, line) => sum + line.n, 0),
    systems: new Set(lines.map((line) => line.origin)).size,
  };
}
// What the ships of one line can do. `plain` is the path with their own move value, where a
// gravity rift on the way adds one. `boost` is the path with Gravity Drive, with the fewest rifts.
// `gd`: the ships cannot move without Gravity Drive. `helps`: Gravity Drive spares them a rift.
export function moveInfo(state, line) {
  const key = [
    activeId(state),
    state.route,
    !!size(state.enemyFleet),
    state.stale,
    state.scene,
    line.id,
  ].join("|");
  if (!moveCache.has(key)) {
    const base = line.move || U[line.type].move,
      gone = state.stale && line.id === "j-cruiser";
    const found = gone ? [] : routes(state, line.origin, base);
    const further = gone ? [] : routes(state, line.origin, base + 1);
    const load = (path) =>
      U[line.type].capacity
        ? path
            .slice(1, -1)
            .filter((id) => !MAP[id].token)
            .reduce((sum, id) => sum + cargoAt(state, id).length, 0)
        : 0;
    // The game chooses the path of a ship. There is one, and the player cannot change it.
    const via = (path) => (line.via && path.includes(line.via) ? 0 : 1);
    const plain = found
      .filter((path) => path.length === found[0].length)
      .sort((a, b) => via(a) - via(b) || rifts(a).length - rifts(b).length || load(b) - load(a))
      .slice(0, 1);
    const boost = [...further]
      .sort(
        (a, b) =>
          rifts(a).length - rifts(b).length ||
          a.length - b.length ||
          via(a) - via(b) ||
          load(b) - load(a),
      )
      .slice(0, 1);
    const fewest = (paths) => Math.min(...paths.map((path) => rifts(path).length));
    const gd = !plain.length && boost.length > 0;
    moveCache.set(key, {
      base,
      gd,
      helps: plain.length > 0 && boost.length > 0 && fewest(further) < fewest(found),
      capped: MAP[line.origin].anomaly === "nebula",
      plain,
      boost,
      routes: plain.length ? plain : boost,
      reason: plain.length || boost.length ? "" : whyNot(state, line, base),
    });
  }
  return moveCache.get(key);
}
// The path of the ships of a line when nothing is staged.
export function chosen(state, _data, line) {
  return moveInfo(state, line).routes[0];
}
// Movement data, for each ship: { 'lineId#i': 1 } when ship i of the line moves,
// { 'lineId#i>cargoLineId': count } for what is in its hold, and { '@gd:lineId#i': 1 } for the
// one ship of the action that has Gravity Drive. An example may name a line only: see expandMovement.
export const shipKeys = (line) =>
  Array.from({ length: line.n }, (_, index) => `${line.id}#${index}`);
export const shipLine = (shipKey) => lineById[shipKey.split("#")[0]];
export const stagedEntries = (data) =>
  Object.keys(data)
    .filter((key) => key[0] !== "@" && data[key] > 0)
    .map((key) => {
      const [unit, second] = key.split(">"),
        ship = shipLine(unit),
        line = second ? lineById[second] : ship;
      return (
        line &&
        ship &&
        unit.includes("#") && {
          key,
          line,
          // The ship that carries: its key, and its line for a unit in a hold.
          unit,
          ship: second ? ship : null,
          carrier: ship.origin,
          count: data[key],
        }
      );
    })
    .filter(Boolean);
// The ship that has Gravity Drive: it moves one ship in an action.
export const boostKey = (data) =>
  Object.keys(data)
    .find((key) => key.startsWith("@gd:") && data[key] > 0)
    ?.slice(4);
export const hasBoost = (data, shipKey) => boostKey(data) === shipKey;
// Another ship that has Gravity Drive.
export function boosted(_state, data, except) {
  const key = boostKey(data);
  return key && key !== except ? key : undefined;
}
export function setBoost(state, data, shipKey) {
  for (const key of Object.keys(data)) {
    if (key.startsWith("@gd:")) {
      delete data[key];
    }
  }
  if (shipKey) {
    data[`@gd:${shipKey}`] = 1;
  }
  // A ship that cannot move without Gravity Drive stays when another ship takes it.
  for (const entry of stagedEntries(data)) {
    if (!entry.ship && entry.unit !== shipKey && moveInfo(state, entry.line).gd) {
      data[entry.key] = 0;
      trimHold(data, entry.unit);
    }
  }
}
// The path of one ship: with Gravity Drive the path with the fewest rifts.
export function shipPath(state, data, shipKey) {
  const info = moveInfo(state, shipLine(shipKey));
  return hasBoost(data, shipKey) ? (info.boost[0] ?? info.plain[0]) : info.routes[0];
}
// The systems between the start and the active system where a ship can load.
export function pickupSites(state, data, shipKey) {
  const path = U[shipLine(shipKey).type].capacity ? shipPath(state, data, shipKey) : null;
  return (path || []).slice(1, -1).filter((id) => cargoAt(state, id).length);
}
// What is in the hold of one ship, and the room it has.
export const holdEntries = (data, shipKey) =>
  stagedEntries(data).filter((entry) => entry.ship && entry.unit === shipKey);
export const holdRoom = (data, shipKey) =>
  data[shipKey] ? U[shipLine(shipKey).type].capacity || 0 : 0;
export const holdLoad = (data, shipKey) =>
  holdEntries(data, shipKey).reduce((sum, entry) => sum + entry.count, 0);
// What one ship can load: the units where it starts, then those on its way.
// A system with the player's command token gives nothing; it is listed with that reason.
export function loadable(state, data, shipKey) {
  const ship = shipLine(shipKey);
  if (!U[ship.type].capacity) {
    return [];
  }
  return [ship.origin, ...pickupSites(state, data, shipKey)].flatMap((site) =>
    cargoAt(state, site).map((line) => ({
      line,
      key: `${shipKey}>${line.id}`,
      pickup: site !== ship.origin,
      token: site !== ship.origin && !!MAP[site].token,
    })),
  );
}
export function available(state, data, key) {
  const [unit, second] = key.split(">"),
    ship = shipLine(unit);
  if (!second) {
    const info = moveInfo(state, ship);
    return !info.routes.length ? 0 : info.gd && boosted(state, data, unit) ? 0 : 1;
  }
  const line = lineById[second];
  if (
    !data[unit] ||
    !loadable(state, data, unit).some((cargo) => cargo.line === line && !cargo.token)
  ) {
    return 0;
  }
  return (
    line.n -
    stagedEntries(data)
      .filter((entry) => entry.line === line && entry.key !== key)
      .reduce((sum, entry) => sum + entry.count, 0)
  );
}
// An example names a line and a count: "j-carrier": 2 moves the first two carriers of Jord,
// "j-infantry": 3 loads three infantry on the ships that leave Jord, in the order of the lines,
// and "l-carrier>t-infantry": 2 loads a pickup on the ships of that line.
export function expandMovement(data) {
  const short = Object.keys(data).filter((key) => key[0] !== "@" && !key.includes("#"));
  const isShip = (key) => !key.includes(">") && lineById[key] && U[lineById[key].type].ship;
  for (const key of short.filter(isShip)) {
    shipKeys(lineById[key]).forEach((unit, index) => {
      data[unit] = index < data[key] ? 1 : 0;
    });
    delete data[key];
  }
  for (const key of short.filter((key) => !isShip(key))) {
    const [first, second] = key.split(">"),
      line = lineById[second || first];
    let left = data[key];
    delete data[key];
    if (!line) {
      continue;
    }
    const ships = LINES.filter((ship) =>
      second ? ship.id === first : U[ship.type].ship && ship.origin === line.origin,
    ).flatMap(shipKeys);
    for (const unit of ships) {
      const count = Math.min(left, holdRoom(data, unit) - holdLoad(data, unit));
      if (count > 0) {
        const slot = `${unit}>${line.id}`;
        data[slot] = (data[slot] || 0) + count;
        left -= count;
      }
    }
  }
  return data;
}
// A ship that cannot move without Gravity Drive takes it, when no ship has it.
export function autoBoost(state, data) {
  const needs = stagedEntries(data).find((entry) => !entry.ship && moveInfo(state, entry.line).gd);
  if (needs && !boostKey(data)) {
    data[`@gd:${needs.unit}`] = 1;
  }
}
// A ship stays: what it had loaded stays too, and it gives Gravity Drive back.
export function trimHold(data, shipKey) {
  if (data[shipKey]) {
    return;
  }
  for (const entry of holdEntries({ ...data, [shipKey]: 1 }, shipKey)) {
    data[entry.key] = 0;
  }
  delete data[`@gd:${shipKey}`];
}
// The usual target: the hold of the ship is full. With a source, of that kind of unit only;
// with none, ground forces before fighters.
export function fillHold(state, data, shipKey, source) {
  const order = (cargo) => (cargo.line.type === "fighter" ? 1 : 0);
  const cargoes = loadable(state, data, shipKey)
    .filter((cargo) => !source || cargo.line.id === source)
    .sort((a, b) => order(a) - order(b));
  for (const cargo of cargoes) {
    const room = holdRoom(data, shipKey) - holdLoad(data, shipKey);
    const count = data[cargo.key] || 0;
    const more = Math.min(room, available(state, data, cargo.key) - count);
    if (more > 0) {
      data[cargo.key] = count + more;
    }
  }
}
// Every ship that leaves a system, in the order of the lines.
export function fillOrigin(state, data, origin, source) {
  for (const entry of stagedEntries(data)) {
    if (!entry.ship && entry.carrier === origin) {
      fillHold(state, data, entry.unit, source);
    }
  }
}
export function originTotals(data, origin) {
  const total = { ships: 0, capacity: 0, cargo: 0, carriers: [] };
  for (const { line, carrier, count } of stagedEntries(data)) {
    if (origin && carrier !== origin) {
      continue;
    }
    const room = count * (U[line.type].capacity || 0);
    if (!U[line.type].ship) {
      total.cargo += count;
    } else {
      total.ships += count;
      total.capacity += room;
      if (room) {
        total.carriers.push(`${unitName(line.type)} ${room}`);
      }
    }
  }
  return total;
}
// One entry per ship and per gravity rift that ship leaves, with what is in its hold.
export const riftExits = (state, data) =>
  stagedEntries(data)
    .filter((entry) => !entry.ship)
    .flatMap(({ line, unit }) =>
      rifts(shipPath(state, data, unit) || []).map((rift) => ({
        line: line.id,
        ship: unit,
        type: line.type,
        rift,
        cargo: Object.fromEntries(
          holdEntries(data, unit).map((entry) => [entry.line.type, entry.count]),
        ),
      })),
    );
export function productionTotals(state, data) {
  const count = (type) => data.units[type] || 0;
  const built = Object.keys(STOCK).filter(count);
  const dock = state.planets.find(
    (planet) => planet.owner === "sol" && planet.structures.includes("dock"),
  );
  const newShips = force(
    Object.fromEntries(built.filter((type) => U[type].ship).map((type) => [type, count(type)])),
  );
  const inSpace = built
    .filter((type) => U[type].ground && data.place[type] === "space")
    .reduce((sum, type) => sum + count(type), 0);
  return {
    count: built.reduce((sum, type) => sum + count(type), 0),
    capacity: dock ? dock.res + 2 : 0,
    cost: built.reduce(
      (sum, type) =>
        sum + (U[type].per ? Math.ceil(count(type) / U[type].per) : count(type)) * U[type].cost,
      0,
    ),
    paid: payTotal(data.pay),
    ships: ships(state.fleet) + ships(newShips),
    cargo:
      (state.fleet.fighter?.n || 0) +
      state.ground.infantry +
      state.ground.mech +
      count("fighter") +
      inSpace,
    // Space Dock: up to 3 fighters in the system do not count against capacity.
    room: capacityOf(state.fleet) + capacityOf(newShips) + 3,
  };
}
export function problem(state, step, data) {
  if (step === 0) {
    if (!data.system) {
      return "Click a system on the board to activate it.";
    }
    if (MAP[data.system].token) {
      return `Your command token is already in ${MAP[data.system].name}. Choose a different system.`;
    }
    if (!SCRIPTED.includes(data.system)) {
      return "This demo scripts only Starpoint and Wellon. Choose one of them to continue.";
    }
  }
  if (step === 1) {
    const entries = stagedEntries(data);
    if (entries.some((entry) => entry.count > available(state, data, entry.key))) {
      return "A staged selection is no longer available. Remove the marked rows.";
    }
    const needy = entries.filter((entry) => !entry.ship && moveInfo(state, entry.line).gd);
    if (needy.some((entry) => !hasBoost(data, entry.unit))) {
      return "Gravity Drive moves one ship per action. Remove one of the boosted ships.";
    }
    for (const origin of new Set(entries.map((entry) => entry.carrier))) {
      const total = originTotals(data, origin);
      if (total.cargo > total.capacity) {
        return `Cargo carried from ${MAP[origin].name} exceeds its transport capacity by ${total.cargo - total.capacity}.`;
      }
    }
    // Every ship has its own hold.
    for (const unit of new Set(entries.filter((entry) => entry.ship).map((entry) => entry.unit))) {
      const ship = shipLine(unit);
      const loaded = holdLoad(data, unit);
      const room = holdRoom(data, unit);
      if (loaded > room) {
        return `The ${unitName(ship.type).toLowerCase()} from ${MAP[ship.origin].name} carries ${loaded}, and its hold has room for ${room}.`;
      }
    }
  }
  if (step === 3) {
    for (const type of ["infantry", "mech"]) {
      const used = state.planets.reduce((sum, planet) => sum + landing(data, planet.id, type), 0);
      if (used > state.ground[type]) {
        return `Landings use ${used} ${unitName(type, used)}; the fleet carries ${state.ground[type]}.`;
      }
    }
  }
  if (step === 4) {
    const total = productionTotals(state, data);
    if (total.count > total.capacity) {
      return `Production capacity exceeded. Build no more than ${total.capacity} units.`;
    }
    if (total.ships > SUPPLY) {
      return `Fleet supply allows ${Math.max(0, SUPPLY - ships(state.fleet))} more non-fighter ships here.`;
    }
    if (total.cargo > total.room) {
      return `Fighters and ground forces in space exceed capacity by ${total.cargo - total.room}.`;
    }
    if (total.paid < total.cost) {
      return `Stage ${plural(total.cost - total.paid, "more resource")} to pay for this build.`;
    }
  }
  return "";
}
export function setup(state) {
  const friendly = state.route === "friendly";
  if (state.data.activation.system === "19") {
    state.enemyFleet = {};
    state.planets = [
      { id: "wellon", name: "Wellon", res: 1, inf: 2, owner: null, units: {}, structures: [] },
    ];
  } else {
    state.enemyFleet = force(state.forces?.def || garrison(state) || {});
    state.planets = [
      {
        id: "starpoint",
        name: "Starpoint",
        res: 3,
        inf: 1,
        owner: "sol",
        units: force({ infantry: 2 }),
        structures: ["dock"],
      },
      {
        id: "newalbion",
        name: "New Albion",
        res: 1,
        inf: 1,
        owner: friendly ? "sol" : "hacan",
        units: force({ infantry: friendly ? 1 : 2 }),
        structures: state.route === "cannon" ? ["pds"] : [],
      },
    ];
  }
  Object.assign(state, {
    // The ships of the player that are in the active system before the movement.
    fleet: force(state.present || {}),
    ground: { infantry: 0, mech: 0 },
    removed: [],
    rift: null,
    cannon: null,
    battle: null,
    inv: null,
    boundary: null,
  });
}
export function trimCargo(state, removed) {
  const cargo = () => (state.fleet.fighter?.n || 0) + state.ground.infantry + state.ground.mech;
  while (cargo() > capacityOf(state.fleet)) {
    const type = state.fleet.fighter?.n ? "fighter" : state.ground.infantry ? "infantry" : "mech";
    if (type === "fighter") {
      state.fleet.fighter.n--;
    } else {
      state.ground[type]--;
    }
    removed.push(type);
  }
}
export function boundary(state, step, kind) {
  Object.assign(state, { frontier: step, blocker: "boundary", boundary: kind });
}
export function finish(state) {
  Object.assign(state, { frontier: null, blocker: null });
}
export const T = {
  0(state) {
    setup(state);
    state.frontier = 1;
    state.blocker = problem(state, 1, state.data.movement) ? "needs-review" : "decision";
  },
  1(state) {
    setup(state);
    const counts = { ...state.present },
      damaged = {};
    for (const { line, count } of stagedEntries(state.data.movement)) {
      counts[line.type] = (counts[line.type] || 0) + count;
      if (line.damaged) {
        damaged[line.type] = (damaged[line.type] || 0) + count;
      }
    }
    state.ground = { infantry: counts.infantry || 0, mech: counts.mech || 0 };
    state.fleet = force({ ...counts, infantry: 0, mech: 0 }, damaged);
    // An example can set both fleets, to show a battle that the scripted lines cannot build.
    if (state.forces) {
      state.fleet = force(state.forces.att);
    }
    const exits = riftExits(state, state.data.movement);
    if (exits.length && state.mode === "live") {
      const dice = { i: 4 };
      state.rift = exits.map((ship) => {
        const face = nextFace(dice),
          lost = face <= 3 && state.fleet[ship.type]?.n > 0;
        if (lost) {
          drop(state.fleet, ship.type);
          // What the ship carried is destroyed with it.
          for (const type of Object.keys(ship.cargo)) {
            for (let count = ship.cargo[type]; count > 0; count--) {
              if (type === "fighter") {
                drop(state.fleet, type);
              } else {
                state.ground[type]--;
              }
            }
          }
        }
        return { ...ship, face, lost };
      });
    }
    for (const type of LOSS_ORDER) {
      while (!state.forces && U[type].ship && ships(state.fleet) > SUPPLY && state.fleet[type]?.n) {
        drop(state.fleet, type);
        state.removed.push(type);
      }
    }
    trimCargo(state, state.removed);
    if (exits.length && state.mode === "draft") {
      return boundary(state, 1, "rift");
    }
    if (state.planets.some((planet) => hostile(planet) && shielded(planet)) && size(state.fleet)) {
      if (state.mode === "draft") {
        return boundary(state, 1, "cannon");
      }
      const rec = record("cannon", "Space cannon offense", {
        att: state.fleet,
        def: force({ pds: 1 }),
      });
      setRoll(rec, "def", roll({ i: 0 }, rec.start.def, SPECS.cannon));
      applyActions(state.fleet, autoAssign(state.fleet, rec.hits.def), rec, "att");
      state.cannon = rec;
    }
    toCombat(state);
  },
  3(state) {
    if (state.mode === "live") {
      for (const type of ["infantry", "mech"]) {
        state.ground[type] -= state.planets.reduce(
          (sum, planet) => sum + landing(state.data.invasion, planet.id, type),
          0,
        );
      }
    }
    resolveLandings(state);
  },
  4(state) {
    const data = state.data.production;
    if (state.mode === "live") {
      for (const type of Object.keys(STOCK)) {
        const count = data.units[type] || 0;
        if (!count) {
          continue;
        }
        if (!U[type].ground) {
          add(state.fleet, type, count);
        } else if (data.place[type] === "space") {
          state.ground[type] += count;
        } else {
          add(state.planets.find((planet) => planet.id === data.place[type]).units, type, count);
        }
      }
    }
    finish(state);
  },
};
export function toCombat(state) {
  if (!size(state.enemyFleet) || !size(state.fleet)) {
    state.skipped[2] = size(state.enemyFleet) ? "no ships moved in" : "no enemy ships";
    return toInvasion(state);
  }
  state.frontier = 2;
  if (state.mode === "draft") {
    return boundary(state, 2, "combat");
  }
  const battle = (state.battle = newBattle("space", clone(state.fleet), clone(state.enemyFleet)));
  state.blocker = "battle";
  barrage(battle);
  if (!alive(battle, "att") || !alive(battle, "def")) {
    finishBattle(state, battle);
  }
}
export function afterCombat(state) {
  const battle = state.battle;
  state.fleet = clone(battle.forces.att);
  state.enemyFleet = clone(battle.forces.def);
  battle.cargoLost = [];
  if (!size(state.fleet)) {
    state.ground = { infantry: 0, mech: 0 };
    state.skipped[3] = "no attacking forces left";
    state.skipped[4] = "not reached in this demo";
    return finish(state);
  }
  trimCargo(state, battle.cargoLost);
  toInvasion(state);
}
export function toInvasion(state) {
  if (!state.ground.infantry && !state.ground.mech) {
    state.skipped[3] = "no ground forces";
    return toProduction(state);
  }
  const bombers = force(
    Object.fromEntries(
      types(state.fleet)
        .filter((type) => U[type].bombard)
        .map((type) => [type, state.fleet[type].n]),
    ),
  );
  state.frontier = 3;
  state.inv = {
    records: {},
    battles: {},
    results: {},
    bombard: {},
    before: Object.fromEntries(state.planets.map((planet) => [planet.id, clone(planet)])),
  };
  for (const planet of state.planets.filter((planet) => hostile(planet) && size(planet.units))) {
    if (shielded(planet)) {
      state.inv.bombard[planet.id] = "Planetary Shield";
    } else if (!size(bombers)) {
      state.inv.bombard[planet.id] = "no Bombardment units";
    } else if (state.mode === "draft") {
      return boundary(state, 3, "bombard");
    } else {
      const rec = record("bombard", "Bombardment", { att: bombers, def: planet.units });
      setRoll(rec, "att", roll({ i: 2 }, bombers, SPECS.bombard));
      applyActions(planet.units, autoAssign(planet.units, rec.hits.att), rec, "def");
      state.inv.records[planet.id] = [rec];
    }
  }
  state.blocker = problem(state, 3, state.data.invasion) ? "needs-review" : "decision";
}
export function resolveLandings(state) {
  const inv = state.inv;
  for (const planet of state.planets) {
    const troops = landed(state, planet.id);
    if (inv.results[planet.id] || !size(troops)) {
      continue;
    }
    if (state.mode === "draft") {
      if (hostile(planet)) {
        return boundary(state, 3, shielded(planet) ? "cannon-defense" : "ground");
      }
      continue;
    }
    if (!hostile(planet)) {
      inv.results[planet.id] = planet.owner ? "reinforced" : "claimed";
      planet.owner = "sol";
      for (const type of types(troops)) {
        add(planet.units, type, troops[type].n);
      }
      continue;
    }
    if (inv.battles[planet.id]) {
      return;
    }
    if (shielded(planet)) {
      const rec = record("cannon", "Space cannon defense", { att: troops, def: force({ pds: 1 }) });
      setRoll(rec, "def", roll({ i: 5 }, rec.start.def, SPECS.cannon));
      applyActions(troops, autoAssign(troops, rec.hits.def), rec, "att");
      (inv.records[planet.id] ||= []).push(rec);
    }
    if (!size(troops)) {
      inv.results[planet.id] = "repelled";
      continue;
    }
    const battle = (inv.battles[planet.id] = newBattle("ground", troops, clone(planet.units), {
      planet: planet.id,
      i: 2,
      pause: !state.auto || state.hold,
    }));
    state.blocker = "battle";
    if (alive(battle, "def")) {
      battleRoll(state, battle);
    } else {
      finishBattle(state, battle);
    }
    return;
  }
  toProduction(state);
}
export function afterGround(state, battle) {
  const planet = state.planets.find((item) => item.id === battle.planet);
  const won = battle.winner === "att";
  state.inv.results[planet.id] = won ? "captured" : "held";
  planet.units = clone(battle.forces[won ? "att" : "def"]);
  if (won) {
    Object.assign(planet, { owner: "sol", structures: [] });
  }
  resolveLandings(state);
}
export function toProduction(state) {
  if (
    !state.planets.some((planet) => planet.owner === "sol" && planet.structures.includes("dock"))
  ) {
    state.skipped[4] = "no unit with Production";
    return finish(state);
  }
  state.frontier = 4;
  state.blocker = problem(state, 4, state.data.production) ? "needs-review" : "decision";
}
export function commitStep(state, step) {
  const kept = { ...state.done };
  for (let later = step; later < 5; later++) {
    delete state.done[later];
    if (later > step) {
      delete state.skipped[later];
    }
  }
  T[step](state);
  state.done[step] = true;
  // A draft keeps later recorded choices that still fit; the first one that does not needs review.
  while (
    state.mode === "draft" &&
    state.blocker === "decision" &&
    state.frontier > step &&
    kept[state.frontier]
  ) {
    const next = state.frontier;
    T[next](state);
    state.done[next] = true;
  }
}
export function blankState(example, workspace, route, data) {
  const state = {
    example,
    mode: workspace,
    route,
    data: { ...clone(data), movement: expandMovement(clone(data.movement)) },
    stale: false,
    frontier: 0,
    blocker: "decision",
    kind: "tactical",
    skipped: {},
    done: {},
    open: {},
    selected: 0,
    edit: null,
    view: {},
    inspect: "27",
    zoom: 1,
    hands: { sol: [...P.sol.hand], hacan: [...P.hacan.hand] },
    history: [],
    cursor: -1,
  };
  setup(state);
  return state;
}
export function makeState(example) {
  const config = examples[example];
  if (config.kind) {
    const flow = blankState(example, "live", "hostile", baseData);
    const data = clone(config.flow);
    if (config.kind === "strategic") {
      data.mine = { ...blankMine(data.number), ...data.mine };
    }
    return Object.assign(flow, {
      kind: config.kind,
      flow: data,
      tip: config.tip || "",
      selected: config.show || 0,
      inspect: null,
    });
  }
  const state = blankState(example, config.mode, config.route, {
    ...baseData,
    movement: { ...baseData.movement, ...config.movement },
  });
  const until = config.until ?? 5;
  state.scene = config.scene;
  autoBoost(state, state.data.movement);
  if (config.invasion) {
    state.data.invasion = { ...config.invasion };
  }
  Object.assign(state, {
    stale: !!config.stale,
    scene: config.scene,
    present: config.present,
    tip: config.tip,
    auto: true,
    hold: !!config.hold,
    forces: config.forces,
  });
  if (until > 0) {
    commitStep(state, 0);
  }
  if (until > 1 && state.frontier === 1 && state.blocker === "decision") {
    commitStep(state, 1);
  }
  if (until > 2 && state.battle?.stage === "pre") {
    state.battle.boost.att = true;
    state.hands.sol = state.hands.sol.filter((card) => card !== "Morale Boost");
    while (state.battle.stage === "pre" && state.battle.round < 40) {
      battleRoll(state, state.battle);
    }
  }
  if (until > 3 && state.frontier === 3 && state.blocker === "decision") {
    commitStep(state, 3);
  }
  if (until > 4 && state.frontier === 4 && state.blocker === "decision") {
    commitStep(state, 4);
  }
  Object.assign(state, {
    auto: false,
    hold: false,
    selected: config.show ?? state.frontier ?? 4,
    inspect: state.data.activation.system,
  });
  // The movement of a draft is staged on the map: nothing is open over it at the start.
  if (state.mode === "draft" && state.selected === 1) {
    state.inspect = null;
  }
  remember(state);
  return state;
}
export const systemName = (state) => MAP[state.data.activation.system].name;
export const isReached = (state, step) => state.frontier === null || step <= state.frontier;
export const editedData = (state, step) =>
  state.edit?.step === step ? state.edit.value : state.data[KEYS[step]];
export const validation = (state) =>
  state.edit ? problem(state, state.edit.step, state.edit.value) : "";
export function activeBattle(state) {
  const battle =
    state.frontier === 2
      ? state.battle
      : state.frontier === 3
        ? Object.values(state.inv?.battles || {}).find((item) => item.stage !== "done")
        : null;
  return battle && battle.stage !== "done" ? battle : null;
}
export function stepState(state, step) {
  if (state.skipped[step]) {
    return "skipped";
  }
  if (!isReached(state, step)) {
    return "future";
  }
  return state.frontier === step ? state.blocker : "done";
}
// Who the action is waiting for, and for what.
export function need(state) {
  if (state.frontier === null || state.blocker === "boundary") {
    return null;
  }
  const battle = activeBattle(state);
  if (startWindow(state, battle)) {
    return { seats: [env.viewer], what: `Start of round ${battle.round}` };
  }
  if (battle?.stage === "pre") {
    return { seats: ["sol", "hacan"], what: `Roll round ${battle.round}` };
  }
  if (battle?.stage === "reaction") {
    return { seats: [SEAT[battle.reaction.side]], what: "Direct Hit reaction" };
  }
  if (battle?.stage === "retreat") {
    return { seats: [SEAT[battle.side]], what: "Choose a retreat destination" };
  }
  if (battle) {
    return {
      seats: [SEAT[battle.side]],
      what: `Assign ${plural(battle.owed[battle.side], "hit")}`,
    };
  }
  return {
    seats: ["sol"],
    what: [
      "Choose a system",
      "Stage ships and cargo",
      "",
      "Commit ground forces",
      "Choose units and payment",
    ][state.frontier],
  };
}
export const mine = (state) => state.mode === "draft" || !!need(state)?.seats.includes(env.viewer);
export const waitingFor = (state) =>
  need(state)
    .seats.map((seat) => P[seat].name)
    .join(" and ");
export function beginEdit(state, step, show = true) {
  state.edit = { step, value: clone(state.data[KEYS[step]]), dirty: false };
  if (show) {
    state.selected = step;
  }
}
// The current step needs no click to start: its controls are open as soon as it is the viewer's.
export function normalize(state) {
  if (state.kind !== "tactical") {
    return;
  }
  const battle = activeBattle(state);
  if (battle?.stage === "waiting" && controls(state, battle.side)) {
    Object.assign(battle, { stage: "assign", staged: [] });
  }
  const decides = state.mode === "draft" || env.viewer === "sol";
  const draft = state.mode === "draft";
  // The movement of a draft is open whenever its step is in view: it has no button to open it.
  if (draft && state.selected === 1 && state.done[1] && !state.edit?.dirty) {
    if (state.edit?.step !== 1) {
      beginEdit(state, 1, false);
    }
  } else if (recorded(state) && state.selected !== 1) {
    state.edit = null;
  }
  if (!state.edit && decides && ["decision", "needs-review"].includes(state.blocker)) {
    beginEdit(state, state.frontier, false);
  }
  if (draft && state.edit?.step === 1 && state.frontier === 1 && state.blocker === "decision") {
    recordMovement(state);
  }
}
// The open movement of a draft with nothing that is not recorded: it holds nothing back.
export const recorded = (state) =>
  state.mode === "draft" && state.edit?.step === 1 && !state.edit.dirty && !!state.done[1];
// An edit that must be committed or dropped before the draft can go on.
export const pendingEdit = (state) => !!state.edit && !recorded(state);
// A draft records every change of the movement at once, and the later steps are checked again.
// A movement that is not valid stays staged, with its error.
export function recordMovement(state) {
  if (state.mode !== "draft" || state.edit?.step !== 1 || validation(state)) {
    return false;
  }
  const { selected, inspect } = state;
  state.data.movement = clone(state.edit.value);
  state.edit = null;
  commitStep(state, 1);
  beginEdit(state, 1, false);
  Object.assign(state, { selected, inspect });
  return true;
}
export function remember(state) {
  normalize(state);
  if (state.mode !== "draft") {
    return;
  }
  const { history, cursor, ...rest } = state;
  state.history = [...history.slice(0, cursor + 1), JSON.stringify(rest)];
  state.cursor = state.history.length - 1;
}
export function travel(state, cursor) {
  const history = state.history;
  for (const key of Object.keys(state)) {
    delete state[key];
  }
  Object.assign(state, JSON.parse(history[cursor]), { history, cursor });
}
export function applyReadiness(state) {
  if (env.viewer !== "sol") {
    return "Only the acting player can apply";
  }
  if (state.edit?.dirty || (pendingEdit(state) && state.edit.step !== state.frontier)) {
    return `Commit or cancel the ${STEPS[state.edit.step].toLowerCase()} edits first`;
  }
  if (state.blocker === "needs-review") {
    return "Fix the changed selection first";
  }
  if (state.frontier !== null && state.blocker !== "boundary") {
    return `Finish ${STEPS[state.frontier].toLowerCase()} first`;
  }
  return "Apply the recorded choices to the live game";
}
export const canApply = (state) => state.frontier === null || state.blocker === "boundary";
export const isTactical = (state) => state.kind === "tactical";
export const actor = (state) => state.flow?.owner || "sol";
/**
 * The demo holds the private side of one seat only: the hand, the drafts and the staged choices
 * of Jamie. Any other viewer sees the public side and has no controls outside a battle.
 */
export const own = () => env.viewer === "sol";
export function flowNeed(state) {
  const flow = state.flow;
  if (!own() && state.kind !== "strategic") {
    const open = state.kind === "picker" || state.kind === "waiting" || flow.stage !== "done";
    return open
      ? { mine: false, seat: state.kind === "waiting" ? flow.owner : "sol", text: "their turn" }
      : null;
  }
  if (state.kind === "picker") {
    return { mine: true, text: "Choose an action" };
  }
  if (state.kind === "waiting") {
    return { mine: false, seat: flow.owner, text: "their turn" };
  }
  if (state.kind === "decision") {
    return flow.stage === "done" ? null : { mine: true, text: DECISIONS[flow.decision].ask };
  }
  if (state.kind === "component" && flow.stage === "reaction") {
    return { mine: true, text: `Reaction to ${flow.card}` };
  }
  if (state.kind === "component") {
    return flow.stage === "done"
      ? null
      : { mine: true, text: flow.stage === "ready" ? `Play ${flow.card}` : "Choose a planet" };
  }
  if (flow.stage === "done") {
    return null;
  }
  const seat = flow.stage === "primary" ? flow.owner : flow.order[flow.turn];
  return {
    mine: seat === "sol" && own(),
    seat,
    text: `${flow.card} ${flow.stage === "primary" ? "primary" : "secondary"}`,
  };
}
/** The row of the viewer in the seat order of a strategy card: the primary, or their secondary. */
export const flowOwnRow = (state) =>
  !own()
    ? null
    : state.flow.owner === "sol"
      ? "primary"
      : state.flow.order.includes("sol")
        ? "sol"
        : null;
/** True while the viewer has a decision or a draft in this strategy card. */
export const flowOpen = (state) => {
  const flow = state.flow;
  if (!own()) {
    return false;
  }
  return flow.owner === "sol"
    ? flow.stage === "primary"
    : flow.order.includes("sol") && !flow.mine.resolved && flow.stage !== "done";
};
export const flowEditing = (state) => flowOpen(state) && !state.flow.mine.ready;
// The viewer's own action, before its first commit. Nothing is sent yet, so it can be dropped.
export const flowStaged = (state) =>
  own() &&
  state.flow?.owner === "sol" &&
  ((state.kind === "strategic" && state.flow.stage === "primary" && !state.flow.sent) ||
    (state.kind === "component" && state.flow.stage !== "done") ||
    (state.kind === "decision" && state.flow.card && state.flow.stage !== "done"));
// A decision: the ships of the viewer in the system that is over the limit, and the limit.
export function unitsOver(state) {
  const system = "1";
  const lines = linesOf(state).filter(
    (line) => line.origin === system && U[line.type].ship && line.type !== "fighter",
  );
  const total = lines.reduce((sum, line) => sum + line.n, 0);
  const removed = lines.reduce((sum, line) => sum + (state.flow.counts[line.id] || 0), 0);
  return { system, lines, limit: total - 1, left: total - removed, removed };
}
// The staged choice of a decision is complete: the main button can send it.
export function decisionReady(state) {
  const flow = state.flow;
  if (DECISIONS[flow.decision].shape !== "units") {
    return flow.chosen !== null;
  }
  const units = unitsOver(state);
  return units.left <= units.limit;
}
export const decisionStaged = (state) =>
  state.kind === "decision" &&
  state.flow.stage !== "done" &&
  (state.flow.chosen !== null || Object.values(state.flow.counts).some(Boolean));
// The viewer's action is complete and the turn is still theirs.
export const turnClosing = (state) =>
  !state.past &&
  env.viewer === "sol" &&
  (state.kind === "tactical"
    ? state.mode === "live" && state.frontier === null
    : (["strategic", "component"].includes(state.kind) ||
        (state.kind === "decision" && state.flow.card)) &&
      state.flow.owner === "sol" &&
      state.flow.stage === "done");
// The card that the viewer staged in an open reaction window.
export const reactionPick = (state) =>
  state.kind === "tactical"
    ? activeBattle(state)?.stage === "reaction"
      ? activeBattle(state).reaction.pick || null
      : startWindow(state, activeBattle(state))
        ? activeBattle(state).start.pick
        : null
    : state.flow?.stage === "reaction"
      ? state.flow.reaction.pick
      : null;
/** The trade goods of the viewer: the source of a payment that is not on the board. */
export const GOODS = PAY.find((source) => !source.system);
/** What one source pays now. A planet pays its value; trade goods pay one each, as many as staged. */
export const payValue = (pay, source, key = "res") =>
  source.system
    ? pay[source.id]
      ? key === "inf"
        ? (source.inf ?? source.res)
        : source.res
      : 0
    : Math.min(source.res, +pay[source.id] || 0);
export const payTotal = (pay, key = "res") =>
  PAY.reduce((sum, source) => sum + payValue(pay, source, key), 0);
// A payment with the least waste. Planets first: trade goods pay only what the planets leave.
export function autoPay(cost, key) {
  if (cost <= 0) {
    return {};
  }
  const planets = PAY.filter((source) => source.system);
  const value = (source) => (key === "inf" ? (source.inf ?? source.res) : source.res);
  let best = null;
  for (let mask = 0; mask < 1 << planets.length; mask++) {
    const chosen = planets.filter((_, index) => mask & (1 << index));
    const paid = chosen.reduce((sum, source) => sum + value(source), 0);
    const goods = Math.max(0, cost - paid);
    if (goods > (GOODS?.res ?? 0)) {
      continue;
    }
    const score = (paid + goods - cost) * 1000 + goods * 100 + chosen.length;
    if (!best || score < best.score) {
      best = { score, chosen, goods };
    }
  }
  if (!best) {
    return {};
  }
  const pay = Object.fromEntries(best.chosen.map((source) => [source.id, true]));
  if (best.goods) {
    pay[GOODS.id] = best.goods;
  }
  return pay;
}
export const flowFree = (state) => (state.flow.owner === "sol" ? 3 : 0);
export const flowPaid = (state) => payTotal(state.flow.mine.pay, "inf");
export function flowProblem(state) {
  const mine = state.flow.mine,
    total = flowFree(state) + mine.buy,
    placed = mine.pools.t + mine.pools.f + mine.pools.s;
  if (flowPaid(state) < mine.buy * 3) {
    return `Stage ${mine.buy * 3 - flowPaid(state)} more influence to buy ${plural(mine.buy, "token")}.`;
  }
  if (placed !== total) {
    return `Place ${plural(total, "token")} in your pools. ${placed} placed.`;
  }
  return "";
}
export function resolveMine(state) {
  const flow = state.flow;
  flow.results.sol =
    flow.mine.outcome ??
    (flow.mine.buy
      ? `Followed · bought ${plural(flow.mine.buy, "token")} for ${flow.mine.buy * 3} influence`
      : "Passed");
  flow.mine.resolved = true;
  flow.turn++;
}
// A drafted secondary resolves by itself when its seat is reached.
export function settle(state) {
  const flow = state.flow;
  if (flow.stage === "secondary" && flow.order[flow.turn] === "sol" && flow.mine.ready) {
    resolveMine(state);
    env.toasts.push("Your drafted secondary resolved.");
  }
  if (flow.stage === "secondary" && flow.turn >= flow.order.length) {
    flow.stage = "done";
  }
  if (flow.stage !== "primary") {
    state.selected = 1;
  }
}
export function flowAdvance(state) {
  const flow = state.flow;
  if (flow.stage === "primary") {
    Object.assign(flow, {
      stage: "secondary",
      primaryResult: `${SEATS[flow.owner].name} ${STRATEGY_SIM[flow.number]?.primary ?? "gained 3 command tokens and bought 1 for 3 influence."}`,
    });
  } else if (flow.stage === "secondary" && flow.order[flow.turn] === "sol") {
    // Another viewer simulates Jamie: the draft resolves as it stands.
    resolveMine(state);
  } else if (flow.stage === "secondary") {
    const sim = SIM[flow.order[flow.turn]];
    flow.results[flow.order[flow.turn]] =
      STRATEGY_SIM[flow.number] && sim.startsWith("Followed")
        ? `Followed · ${STRATEGY_SIM[flow.number].follow}`
        : sim;
    flow.turn++;
  }
  settle(state);
}
export function simulate(state) {
  const battle = activeBattle(state);
  if (battle?.stage === "pre") {
    battleRoll(state, battle);
  } else if (battle?.stage === "waiting") {
    battle.stage = "hits";
    battleContinue(state, battle);
  } else if (!battle && state.blocker === "decision") {
    state.edit = null;
    commitStep(state, state.frontier);
  }
}
