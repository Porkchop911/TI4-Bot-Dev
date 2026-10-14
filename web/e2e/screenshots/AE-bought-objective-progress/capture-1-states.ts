import { test } from "@playwright/test";
import { capture, pay, SIZES, type Scenario } from "./scenario";

const scenarios: Scenario[] = [
  {
    // Enough ready planets and trade goods: the objective reads Ready, as before.
    name: "1-payable-now",
    mine: { monument: pay(8), sway_council: pay(8) },
    theirs: { monument: pay(8), sway_council: pay(8) },
  },
  {
    name: "2-partly-payable",
    mine: { monument: pay(5), sway_council: pay(3) },
    theirs: { monument: pay(6), sway_council: pay(2) },
  },
  {
    // Everything exhausted earlier in the round: capacity is 0, not "no progress made".
    name: "3-not-payable",
    mine: { monument: pay(0), sway_council: pay(0) },
    theirs: { monument: pay(0), sway_council: pay(1) },
  },
  {
    // Status phase scoring decision: the actor's cell drops the "pay at scoring" suffix.
    name: "4-scoring-window",
    mine: { monument: pay(8), sway_council: pay(3) },
    theirs: { monument: pay(4), sway_council: pay(2) },
    scoring: true,
  },
  {
    // Scored badge wins over progress.
    name: "5-scored",
    mine: { monument: pay(8), sway_council: pay(3) },
    theirs: { monument: pay(4), sway_council: pay(2) },
    scored: ["monument"],
  },
];

for (const size of Object.keys(SIZES) as (keyof typeof SIZES)[]) {
  for (const scenario of scenarios) {
    test(`bought objective progress: ${scenario.name} (${size})`, async ({ page }, testInfo) => {
      await capture(page, testInfo, size, scenario);
    });
  }
}
