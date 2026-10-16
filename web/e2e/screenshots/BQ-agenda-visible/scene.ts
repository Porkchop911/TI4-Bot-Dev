import type { Browser, BrowserContext, Page } from "@playwright/test";
import type { MockChoice, MockGameOptions } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { DEVICES, type DeviceName } from "../BN-phone-detection/scene";

export { DEVICES, type DeviceName };

/** Which side of the change a run is. The `before` shots are taken on the commit that precedes it. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const AGENDA = {
  id: "minister_of_peace",
  name: "Minister of Peace",
  type: "Law",
  target: "For/Against",
  text1:
    "For: The elected player may, once per agenda phase, ready the planet Mecatol Rex when another player activates a system adjacent to it.",
  text2: "Against: Discard this card.",
};

export const ELECT = {
  id: "minister_of_commerce",
  name: "Minister of Commerce",
  type: "Law",
  target: "Elect Player",
  text1: "At the start of the status phase, the elected player gains 1 trade good for each planet they control.",
  text2: "",
};

export const SUBSTATES = [
  "reveal-reaction",
  "vote-cast",
  "vote-planets",
  "predict",
  "others-voting",
  "tiebreak",
  "elect-vote",
] as const;
export type Substate = (typeof SUBSTATES)[number];

const planets = [
  ["mecatol_rex", "Mecatol Rex", 6],
  ["jord", "Jord", 2],
  ["lodor", "Lodor", 3],
  ["arc_prime", "Arc Prime", 4],
  ["wren_terra", "Wren Terra", 2],
  ["abyz", "Abyz", 2],
  ["fria", "Fria", 1],
  ["mellon", "Mellon", 2],
] as const;

const forAgainst = (extra: Partial<MockChoice> = {}): MockChoice => ({
  prompt: "vote for which outcome",
  context: { subtype: "cast_vote" },
  details: { agenda: AGENDA },
  options: [
    { id: "FOR", label: "FOR", kind: "vote", payload: { current_votes: 3 } },
    { id: "AGAINST", label: "AGAINST", kind: "vote", payload: { current_votes: 1 } },
    { id: "decline", label: "Abstain", kind: "decline" },
  ],
  ...extra,
});

/** The decision the engine asks in each agenda sub-state (real shapes: the agenda is in details.agenda). */
export function substateChoice(state: Substate): MockChoice {
  switch (state) {
    case "reveal-reaction":
      return {
        prompt: "reaction_when_AGENDA_REVEALED",
        context: {
          subtype: "reaction_when_AGENDA_REVEALED",
          optional: true,
          trigger: { kind: "agenda_revealed", event_type: "AGENDA_REVEALED", event_id: 12, relation: "when", agenda: AGENDA.id },
        } as never,
        options: [
          { id: "reaction:hacan:AGENDA_REVEALED:when", label: "Play Sabotage", kind: "ability", payload: { card: "sabo1", card_name: "Sabotage" } },
          { id: "decline", label: "Pass", kind: "decline" },
        ],
      };
    case "vote-cast":
      return forAgainst();
    case "vote-planets":
      return {
        prompt: "exhaust a planet to vote FOR",
        context: { subtype: "vote_exhaust_planet" },
        details: { agenda: AGENDA },
        options: [
          ...planets.map(([id, name, votes]) => ({
            id,
            label: `exhaust ${name} for ${votes} votes`,
            kind: "vote_planet",
            payload: { votes, planet_name: name },
          })),
          { id: "decline", label: "Done", kind: "decline" },
        ],
      };
    case "predict":
      return {
        prompt: "Imperial Rider: predict the agenda outcome",
        context: { subtype: "predict_agenda_outcome", source: { Rule: "8" } },
        options: [
          { id: "FOR", label: "predict FOR", kind: "prediction" },
          { id: "AGAINST", label: "predict AGAINST", kind: "prediction" },
        ],
      };
    case "others-voting":
      return forAgainst({ player: opponent.id });
    case "tiebreak":
      return {
        prompt: "speaker breaks the tie",
        context: { subtype: "vote_tiebreak" },
        details: { agenda: AGENDA },
        options: [
          { id: "FOR", label: "FOR", kind: "tiebreak" },
          { id: "AGAINST", label: "AGAINST", kind: "tiebreak" },
        ],
      };
    case "elect-vote":
      return {
        prompt: "vote for which outcome",
        context: { subtype: "cast_vote" },
        details: { agenda: ELECT },
        options: [
          { id: "p1", label: "p1", kind: "vote", payload: { current_votes: 0 } },
          { id: "p2", label: "p2", kind: "vote", payload: { current_votes: 2 } },
          { id: "decline", label: "Abstain", kind: "decline" },
        ],
      };
  }
}

export const substateAgenda = (state: Substate) => (state === "elect-vote" ? ELECT : AGENDA);

export function substateOptions(state: Substate): MockGameOptions {
  return {
    phase: "agenda",
    round: 3,
    players: [playerWithHand({ technologies: ["amd", "st"] }), opponent],
    choice: substateChoice(state),
  };
}

export async function newDevicePage(browser: Browser, device: DeviceName, baseURL?: string): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ ...DEVICES[device], colorScheme: "dark", baseURL });
  const page = await context.newPage();
  return { context, page };
}

export interface Visibility {
  /** The agenda name is on screen, inside the viewport. */
  inViewport: boolean;
  /** The element at its centre is the text itself (nothing paints over it). */
  uncovered: boolean;
  testId: string | null;
}

/**
 * Looks for the agenda name among the visible text and reports whether it is inside the viewport
 * and whether nothing covers it (elementFromPoint at the centre of its box).
 */
export async function agendaVisibility(page: Page, name: string): Promise<Visibility> {
  return page.evaluate((needle) => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const results: Array<{ inViewport: boolean; uncovered: boolean; testId: string | null }> = [];
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!node.nodeValue?.includes(needle)) continue;
      const el = node.parentElement;
      if (!el) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const box = range.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      const style = getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") continue;
      // The text must be inside the viewport and not cut off by its own box (ellipsis / line clamp).
      const own = el.getBoundingClientRect();
      const clipped = el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1;
      const inViewport =
        own.top >= 0 && own.left >= 0 && own.bottom <= innerHeight && own.right <= innerWidth + 1 && !clipped;
      const x = Math.min(Math.max(box.left + Math.min(box.width / 2, 40), 0), innerWidth - 1);
      const y = Math.min(Math.max(box.top + box.height / 2, 0), innerHeight - 1);
      const top = document.elementFromPoint(x, y);
      const uncovered = !!top && (top === el || el.contains(top) || top.contains(el));
      results.push({ inViewport, uncovered, testId: el.closest("[data-testid]")?.getAttribute("data-testid") ?? null });
    }
    return results.find((r) => r.inViewport && r.uncovered) ?? results[0] ?? { inViewport: false, uncovered: false, testId: null };
  }, name);
}

/** Pushes the next decision to the open game, like the server does after an answer. */
export async function advanceTo(game: import("../_shared/mockGame").MockedGame, state: Substate, version: number) {
  const options = { ...substateOptions(state), version };
  const { buildSnapshot } = await import("../_shared/mockGame");
  const { events: _events, ...update } = buildSnapshot(options);
  game.send({ ...update, type: "state_update" });
}

/** Pushes a state without a pending decision (resolution, between agendas), or another phase. */
export async function advanceIdle(game: import("../_shared/mockGame").MockedGame, version: number, phase = "agenda") {
  const { buildSnapshot } = await import("../_shared/mockGame");
  const { events: _events, ...update } = buildSnapshot({
    phase,
    round: 3,
    players: [playerWithHand({ technologies: ["amd", "st"] }), opponent],
    choice: null,
    version,
  });
  game.send({ ...update, type: "state_update" });
}
