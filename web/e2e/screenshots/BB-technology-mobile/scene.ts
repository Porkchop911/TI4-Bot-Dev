import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { openScene, technologyScript } from "../AS-secondary-exact-options/scene";
import { playerWithHand, opponent } from "../_shared/players";

/** Which side of the change a run is: `before` is taken on the commit without the fix. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  "360x740": { width: 360, height: 740 },
  "390x844": { width: 390, height: 844 },
  "412x915": { width: 412, height: 915 },
  "844x390": { width: 844, height: 390 },
} as const;
export type SizeName = keyof typeof SIZES;

const researchIds = ["amd", "nm", "st", "ps", "det", "pa", "sdn", "aida", "gd", "dxa", "gls", "md", "sr", "bs", "pi", "sar"];

/** The header's Technologies button, read-only for the viewer. */
export async function openHeader(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, { players: [playerWithHand({ technologies: ["amd", "st"] }), opponent] });
  // On a phone the button lives in the menu sheet (top-menu-button); desktop sizes have it in the header.
  const menu = page.getByTestId("top-menu-button");
  if (await menu.count()) await menu.click();
  await page.getByTestId("technology-modal-button").click();
  await page.getByTestId("technology-modal").waitFor();
}

/** The research_technology decision (own Technology primary) as a decision dialog. */
export async function openDecision(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    players: [playerWithHand({ technologies: ["amd", "st"] }), opponent],
    choice: {
      prompt: "Research a technology",
      context: { subtype: "research_technology", source: { StrategyCard: { card: "Technology", secondary: false } } },
      options: [
        ...researchIds.map((id) => ({ id, label: id, kind: "research" })),
        { id: "decline", label: "Decline", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("technology-modal").waitFor();
}

/** Secondary prepare mode against the engine's exact researchable list. */
export async function openPrepare(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await openScene(page, { name: "Technology", card: "pok7technology", script: technologyScript });
  await page.getByTestId("secondary-prep-chip").dispatchEvent("click"); // landscape: the event-log toggle lies over the chip
  await page.getByTestId("secondary-yes-btn").dispatchEvent("click");
  await page.getByTestId("technology-modal").waitFor();
  await page.waitForTimeout(300);
}

export interface Metrics {
  viewport: { w: number; h: number };
  docScrollW: number;
  modal: { x: number; y: number; w: number; h: number } | null;
  scrollBody: { clientW: number; scrollW: number; clientH: number; scrollH: number } | null;
  gridColumns: number;
  smallTargets: string[];
  smallText: number;
  smallClasses: string[];
  buttons: Record<string, { visible: boolean; covered: string | null; w: number; h: number } | null>;
}

/** Layout facts of the open Technology modal; `ids` are the buttons that must stay reachable. */
export async function measure(page: Page, ids: string[]): Promise<Metrics> {
  return page.evaluate((buttonIds) => {
    const vw = innerWidth;
    const vh = innerHeight;
    const rect = (el: Element) => el.getBoundingClientRect();
    const panel = document.querySelector('[data-testid="technology-modal"] .technology-modal-panel');
    const body = document.querySelector(".technology-modal__scroll-body") as HTMLElement | null;
    const buttons: Metrics["buttons"] = {};
    for (const id of buttonIds) {
      const el = document.querySelector(`[data-testid="${id}"]`);
      if (!el) {
        buttons[id] = null;
        continue;
      }
      const r = rect(el);
      const inView = r.top >= 0 && r.left >= 0 && r.bottom <= vh && r.right <= vw && r.width > 0;
      const top = inView ? document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) : null;
      const covered =
        inView && !(top === el || el.contains(top))
          ? (top?.getAttribute("data-testid") ?? top?.className?.toString() ?? top?.tagName ?? "?")
          : null;
      buttons[id] = { visible: inView, covered, w: Math.round(r.width), h: Math.round(r.height) };
    }
    const modal = document.querySelector('[data-testid="technology-modal"]');
    const small: string[] = [];
    let smallText = 0;
    const smallList = new Set<string>();
    if (modal) {
      for (const el of modal.querySelectorAll("button, .tech-skip-pill, .technology-card[data-selectable='true']")) {
        const r = rect(el);
        // Skip pills are wide and 34 px high on purpose (EX-tech-picker-text-space); the More link is a text link.
        if (el.classList.contains("technology-card__more")) continue;
        const minH = el.classList.contains("tech-skip-pill") ? 34 : 44;
        if (r.width && (r.height < minH || r.width < 44))
          small.push(`${el.getAttribute("data-testid") ?? el.className} ${Math.round(r.width)}x${Math.round(r.height)}`);
      }
      for (const el of modal.querySelectorAll("*")) {
        if (
          [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent?.trim()) &&
          parseFloat(getComputedStyle(el).fontSize) < 12
        )
          smallText++, smallList.add(`${el.className || el.tagName}:${(el.textContent ?? "").slice(0, 14)}:${getComputedStyle(el).fontSize}`);
      }
    }
    const row = document.querySelector(".technology-grid-row");
    const pr = panel ? rect(panel) : null;
    return {
      viewport: { w: vw, h: vh },
      docScrollW: document.documentElement.scrollWidth,
      modal: pr ? { x: Math.round(pr.x), y: Math.round(pr.y), w: Math.round(pr.width), h: Math.round(pr.height) } : null,
      scrollBody: body
        ? { clientW: body.clientWidth, scrollW: body.scrollWidth, clientH: body.clientHeight, scrollH: body.scrollHeight }
        : null,
      gridColumns: row ? getComputedStyle(row).gridTemplateColumns.split(" ").length : 0,
      smallTargets: small,
      smallText,
      smallClasses: [...smallList].slice(0, 8),
      buttons,
    };
  }, ids);
}
