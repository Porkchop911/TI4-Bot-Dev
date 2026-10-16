import type { Browser } from "@playwright/test";
import { measureMap, type OverlayMetrics } from "./measure";
import { extras, openExtra } from "./extras";
import { minimize, openCase, SIZES, type SizeName } from "./scene";

/** One row of the audit: an overlay, how it is reached, and what it does to the map. */
export interface AuditScene {
  overlay: string;
  trigger: string;
  /** A dev-gallery decision (title) or an extras scene (id). */
  gallery?: string;
  extra?: string;
  /** Keep the action pane expanded (the row is the pane itself). */
  paneOpen?: boolean;
  /** Test id of the control that closes it when that control lies outside the overlay (the Players / Events buttons, a dialog's Cancel). */
  closeWith?: string;
  /** Test id of a further control that shrinks it once minimized (the prepare banner's fold). */
  thenFold?: string;
}

export const AUDIT: AuditScene[] = [
  { overlay: "System activation bar", trigger: "decision: choose a system to activate", gallery: "system activation" },
  { overlay: "Planet selection bar", trigger: "decision: pick a planet", gallery: "planet selection" },
  { overlay: "Tactical movement tray", trigger: "decision: move ships", gallery: "tactical movement" },
  { overlay: "Cargo loading tray", trigger: "decision: load cargo", gallery: "tactical cargo" },
  { overlay: "Invasion landing tray", trigger: "decision: land ground forces", extra: "invasion-landing" },
  { overlay: "Ground combat / invasion overlay", trigger: "invasion result", extra: "ground-combat-result" },
  { overlay: "Payment drawer", trigger: "decision: pay resources", gallery: "payment" },
  { overlay: "Production builder", trigger: "decision: produce units", gallery: "production" },
  { overlay: "Space combat: hits / casualties", trigger: "decision: assign hits", gallery: "combat casualty" },
  { overlay: "Space combat: retreat", trigger: "decision: retreat", gallery: "combat retreat" },
  { overlay: "Space combat result", trigger: "a finished battle", extra: "combat-result" },
  { overlay: "Agenda ballot", trigger: "decision: vote", gallery: "agenda vote planets" },
  { overlay: "Trade desk", trigger: "decision: propose a trade", gallery: "transaction propose" },
  { overlay: "Reaction window", trigger: "decision: respond to an action card", gallery: "Reaction: Sabotage" },
  { overlay: "Objective scoring", trigger: "decision: score an objective", gallery: "objective scoring" },
  { overlay: "Strategy card draft / secondary", trigger: "decision: pick a strategy card", gallery: "strategy card draft" },
  { overlay: "Generic choice dialog", trigger: "decision: any other choice", gallery: "generic selection" },
  { overlay: "Remove-unit / over capacity", trigger: "decision: remove a unit", gallery: "Remove a unit: over capacity" },
  { overlay: "Command token redistribution", trigger: "decision: redistribute tokens", gallery: "Redistribute command tokens" },
  { overlay: "Technology research", trigger: "decision: research a technology", gallery: "technology research" },
  { overlay: "Turn action bar", trigger: "your turn menu", gallery: "Turn bar: your turn, one strategy card", paneOpen: true },
  { overlay: "Objectives modal", trigger: "header: Objectives", extra: "objectives" },
  { overlay: "Technologies modal", trigger: "header: Technologies", extra: "technologies" },
  { overlay: "Event log drawer", trigger: "Events button", extra: "event-log", closeWith: "event-log-mobile-toggle" },
  { overlay: "Player sheet drawer", trigger: "Players button", extra: "player-sheet", closeWith: "player-sheet-toggle" },
  { overlay: "System detail panel", trigger: "tap a system (decision open)", extra: "system-inspector" },
  { overlay: "Card detail panel", trigger: "tap a card in the player sheet", extra: "card-details" },
  { overlay: "System tooltip", trigger: "touch on a system", extra: "tile-tooltip" },
  { overlay: "Secondary prep chip", trigger: "another seat resolves a strategy card", extra: "prep-chip" },
  { overlay: "Secondary prepare banner", trigger: "chip, then the question", extra: "prep-banner", thenFold: "prep-fold" },
  { overlay: "Undo confirmation", trigger: "event log: Undo", extra: "undo-confirm", closeWith: "undo-confirm-cancel", thenFold: "event-log-mobile-toggle" },
  { overlay: "Error toast", trigger: "a refused history change", extra: "history-error" },
  { overlay: "Corner toast", trigger: "another seat acts", extra: "corner-toast" },
  { overlay: "Waiting banner / game over", trigger: "no decision; the game ends", extra: "game-over" },
];

export interface AuditRow extends AuditScene {
  size: SizeName;
  open: OverlayMetrics;
  minimized?: OverlayMetrics;
}

/** Opens every audit scene in a fresh context at `size` and measures the map before and after minimizing. */
export async function runAudit(browser: Browser, size: SizeName, only?: (scene: AuditScene) => boolean): Promise<AuditRow[]> {
  const rows: AuditRow[] = [];
  let exclude: number[] = [];
  let excludeChrome: number[] = [];
  // The always-present action pane is minimized in every scene except its own row, so that a row shows
  // what its overlay covers and not the pane (the pane's own minimize control is not part of this change).
  const fresh = async (paneOpen = false) => {
    const context = await browser.newContext({ viewport: SIZES[size], colorScheme: "dark" });
    if (!paneOpen) await context.addInitScript(() => window.localStorage.setItem("ti4_action_pane_minimized", "1"));
    return { context, page: await context.newPage() };
  };
  {
    // Nothing open: the map's own toolbar / legend, and whatever the page itself keeps over the map
    // (header, floating buttons, the minimized pane), are not counted as covered by an overlay.
    const { context, page } = await fresh();
    await openExtra(page, "waiting-other", size);
    const base = await measureMap(page);
    excludeChrome = base.chrome;
    exclude = [...new Set([...base.chrome, ...base.coveredIdx])];
    await context.close();
  }
  for (const scene of AUDIT) {
    if (only && !only(scene)) continue;
    const { context, page } = await fresh(scene.paneOpen);
    try {
      if (scene.gallery) await openCase(page, scene.gallery, size);
      else await openExtra(page, scene.extra!, size);
      const ex = scene.paneOpen ? excludeChrome : exclude;
      const open = await measureMap(page, ex);
      const row: AuditRow = { ...scene, size, open };
      if (scene.closeWith) row.open.minimizeControl = scene.closeWith;
      if (open.covered > 0.15 && (await minimize(page, row.open.minimizeControl))) {
        if (scene.thenFold) {
          const fold = page.getByTestId(scene.thenFold);
          if (await fold.count()) await fold.evaluate((el) => (el as HTMLElement).click());
          await page.waitForTimeout(250);
        }
        row.minimized = await measureMap(page, ex);
      }
      rows.push(row);
    } catch (error) {
      console.log(`AUDIT-ERR ${scene.overlay} ${size}: ${String(error).slice(0, 160)}`);
    } finally {
      await context.close();
    }
  }
  return rows;
}

export const extraIds = extras.map((e) => e.id);
