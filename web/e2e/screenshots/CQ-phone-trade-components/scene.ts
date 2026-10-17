import { expect, type Locator, type Page } from "@playwright/test";
import type { ChoiceOption } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, withPlayer } from "../_shared/players";

/** Which side of the change a run is: `before` is taken on the commit without the fix (no asserts). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
export const checking = prefix !== "before";

export const PHONES = [
  { name: "390x844", width: 390, height: 844 },
  { name: "360x740", width: 360, height: 740 },
  // 390 wide with Safari's address and tool bars showing: the visible page is about 664 px tall.
  { name: "390x664", width: 390, height: 664 },
];

/** A 6-player game: five trade partners, the viewer (Sol) can open four, the fifth is not reachable. */
const PARTNERS = [
  { seat: "other_seat", faction: "hacan", tg: 3, comm: 6, notes: 4, contact: true },
  { seat: "seat_c", faction: "jolnar", tg: 1, comm: 4, notes: 5, contact: true },
  { seat: "seat_d", faction: "letnev", tg: 0, comm: 2, notes: 3, contact: true },
  { seat: "seat_f", faction: "naalu", tg: 4, comm: 3, notes: 4, contact: true },
  { seat: "seat_e", faction: "xxcha", tg: 2, comm: 4, notes: 5, contact: false },
];

export const players = [
  playerWithHand({ technologies: ["sr"], relics: ["codex", "stellarconverter"] }),
  ...PARTNERS.map((p) =>
    withPlayer(opponent, { id: p.seat, faction: p.faction, trade_goods: p.tg, commodities: p.comm }),
  ),
];

const component = (id: string, label: string): ChoiceOption => ({ id, label, kind: "component" });

/**
 * Component actions as the engine offers them (ids and labels as in leaders.rs, relics.rs,
 * technology.rs, faction_abilities.rs, exploration.rs and thunders_edge.rs): several leaders and
 * relics carry only a name or a raw id, no rules text.
 */
export const COMPONENTS: ChoiceOption[] = [
  component("faction|orbital_drop", "Orbital Drop: spend a strategy token to land 2 infantry"),
  component("component|tech|sr", "use Sling Relay"),
  component("faction|trade_convoys", "Trade Convoys: place it faceup in your play area"),
  component("component|leader|solhero", "Jace X, 4th Air Legion"),
  component("relic|codex", "use codex"),
  component("relic|stellarconverter", "use stellarconverter"),
  component("purge|Cultural", "purge 3 cultural relic fragments for a relic"),
  component("play_area:ed1:gd", "spend 6 resources to research Gravity Drive"),
];

export async function openTurnMenu(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await openMockedGame(page, {
    players,
    choice: {
      prompt: "action phase",
      options: [
        { id: "tactical", label: "take a tactical action", kind: "action" },
        { id: "strategic|pok3politics", label: "take the strategic action of 3. Politics", kind: "action" },
        ...COMPONENTS,
        ...PARTNERS.filter((p) => p.contact).map((p) => ({
          id: `component|trade|${p.faction}`,
          label: `open a transaction with ${p.faction}`,
          kind: "open_transaction",
        })),
        { id: "pass", label: "pass", kind: "action" },
      ],
      details: {
        kind: "turn_menu",
        closing: false,
        tokens: { tactic: 3, fleet: 3, strategy: 2 },
        strategy_cards: [{ card: "pok3politics", used: false, option: "strategic|pok3politics" }],
        partners: PARTNERS.map((p) => ({
          seat: p.seat,
          faction: p.faction,
          in_contact: p.contact,
          trade_goods: p.tg,
          commodities: p.comm,
          promissory_notes: p.notes,
          ...(p.contact ? {} : { reason: "Not a neighbour" }),
        })),
      },
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
}

export interface Placement {
  inViewport: boolean;
  covered: string | null;
}

/** Whether a target is fully inside the viewport and is the element hit at its centre (not covered). */
export async function placement(target: Locator): Promise<Placement> {
  return target.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const inViewport = r.top >= 0 && r.left >= 0 && r.bottom <= window.innerHeight && r.right <= window.innerWidth;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const covered = hit && (hit === el || el.contains(hit)) ? null : hit ? `${hit.tagName}.${hit.className}` : "nothing";
    return { inViewport, covered };
  });
}

/** Scrolls a target into its scroll container, then checks it can be seen and tapped. */
export async function expectReachable(target: Locator, what: string) {
  await target.scrollIntoViewIfNeeded();
  const p = await placement(target);
  console.log(`${what}: ${JSON.stringify(p)}`);
  if (!checking) return;
  expect(p.inViewport, `${what} is outside the viewport`).toBe(true);
  expect(p.covered, `${what} is covered`).toBeNull();
  const box = (await target.boundingBox())!;
  expect(box.height, `${what} tap target`).toBeGreaterThanOrEqual(44);
}
