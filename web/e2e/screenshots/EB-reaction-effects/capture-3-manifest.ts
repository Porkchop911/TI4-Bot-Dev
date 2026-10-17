import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "@playwright/test";
import { artifactDir } from "../_shared/shot";
import { SCENES } from "./cards";
import { SIZES, prefix } from "./scene";
import { STRIP_SCENES } from "./strip-scenes";

// Writes manifest.json (before/after pairs per card, grouped by effect type, then the strip) from
// the same scene tables the captures use, so the page cannot drift from what was captured. The
// `before/` PNGs are the same captures run on the base commit with TI4_SHOT_PREFIX=before.
const esc = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

const STRIP_CARDS = new Set([
  "Flank Speed", "Lost Star Chart", "Morale Boost", "Nav Suite", "Solar Flare", "War Machine",
  "Extreme Duress", "Black Market Dealings", "Fighter Prototype", "Bunker", "Blitz", "Disable", "Rout",
  "Waylay", "Intercept", "Shields Holding", "Maneuvering Jets", "Hack Election", "Distinguished Councilor",
  "Political Stability",
]);
/** The cards of the audit's 26 reaction-only and 11 marker cards; the others are extras. */
const AUDIT_MARKERS = new Set([
  "Flank Speed", "Lost Star Chart", "Morale Boost", "Nav Suite", "Solar Flare", "War Machine", "Master Plan",
  "Puppets on a String", "Extreme Duress", "Crisis", "Black Market Dealings",
]);
const EXTRAS = new Set(["Scramble Frequency", "Veto"]);
const UNCOVERED = [
  "Ancient Burial Sites", "Assassinate Representative", "Bribery", "Confusing Legal Text", "Construction Rider",
  "Courageous to the End", "Crash Landing", "Decoy Operation", "Diplomacy Rider", "Diplomatic Pressure",
  "Experimental Battlestation", "Fire Team", "Forward Supply Base", "Ghost Squad", "Imperial Rider",
  "In the Silence of Space", "Insider Information", "Leadership Rider", "Lie in Wait", "Manipulate Investments",
  "Politics Rider", "Public Disgrace", "Reparations", "Rescue", "Reveal Prototype", "Sanction", "Skilled Retreat",
  "Summit", "Technology Rider", "Trade Rider", "Upgrade", "Warfare Rider",
];

test("manifest", async ({}, testInfo) => {
  test.skip(prefix !== "after", "the manifest is written by the after run");
  const seen = new Set<string>();
  const unique = SCENES.filter((scene) => !seen.has(scene.name) && seen.add(scene.name));
  const rows = unique
    .map((scene) => {
      const kind = AUDIT_MARKERS.has(scene.name) ? "marker card" : EXTRAS.has(scene.name) ? "extra" : "reaction-only";
      return `<tr><td>${esc(scene.name)}</td><td>${kind}</td><td class="ok">preview</td><td class="${STRIP_CARDS.has(scene.name) ? "ok" : "no"}">${
        STRIP_CARDS.has(scene.name) ? "strip" : "one-shot: gone before the next question"
      }</td></tr>`;
    })
    .join("");
  const intro = `
    <h2>Coverage</h2>
    <p>${unique.length} cards now say what playing them would do (the 26 reaction-only cards and the 11 marker cards of the audit, plus Scramble Frequency and Veto). ${STRIP_CARDS.size} cards leave a lingering effect that the active-effects strip shows while it is in force.</p>
    <table><thead><tr><th>Card</th><th>Audit class</th><th>Reaction window</th><th>Active-effects strip</th></tr></thead><tbody>${rows}</tbody></table>
    <h2>Not covered</h2>
    <p>These reaction cards have no preview because playing them asks their own question (a pick, a rider outcome, a retreat, a dice reroll), which their own pickers handle: ${UNCOVERED.map(esc).join(", ")}.</p>
    <p>Master Plan, Puppets on a String, Crisis, Coup d'Etat, Deadly Plot, Veto and Confounding Legal Text are one-shot: the engine consumes their flag before the next question is asked, so there is no lingering state to show; their preview in the window is the whole indication.</p>`;
  const groups = [...new Set(unique.map((scene) => scene.group))];
  const cardShots = SCENES.filter((scene) => groups.includes(scene.group)).map((scene) => ({
    group: scene.group,
    heading: scene.name + (scene.key.endsWith("-immune") ? " (immune ship)" : scene.key.endsWith("-none") ? " (nothing to repair)" : ""),
    caption: scene.note,
    images: SIZES.flatMap((size) => [
      { label: `Before, ${size.label}`, file: `before/before-${scene.key}-${size.label}.png` },
      { label: `After, ${size.label}`, file: `out/after-${scene.key}-${size.label}.png` },
    ]),
  }));
  const combatShots = ["direct-hit", "shields-holding"].map((key) => {
    const scene = SCENES.find((candidate) => candidate.key === key)!;
    return {
      group: "Inside the combat overlay",
      heading: `${scene.name}, during a space combat`,
      caption: "In a fight the window is shown in the combat overlay, which has its own offer rows; they carry the same preview.",
      images: SIZES.flatMap((size) => [
        { label: `Before, ${size.label}`, file: `before/before-combat-${key}-${size.label}.png` },
        { label: `After, ${size.label}`, file: `out/after-combat-${key}-${size.label}.png` },
      ]),
    };
  });
  const stripShots = STRIP_SCENES.map((scene) => ({
    group: "Active-effects strip",
    heading: scene.title,
    caption: scene.caption,
    images: SIZES.flatMap((size) => [
      { label: `Before, ${size.label}`, file: `before/before-strip-${scene.key}-${size.label}.png` },
      { label: `After, ${size.label}`, file: `out/after-strip-${scene.key}-${size.label}.png` },
    ]),
  }));
  const manifest = {
    title: "Card Effect Previews",
    description:
      "Reaction windows now state what playing the card would do right now (which card it cancels, which ship it destroys, how many hits it cancels, ...), drawn from engine-provided facts, and a strip under the top bar lists the lingering card effects in force with their seat and expiry. Each card is shown before (base commit, same mocked decision) and after, on desktop and a 390x844 phone. Fixtures are synthetic but shaped exactly like the engine's DecisionContext.previews and GameView.active_effects.",
    tag: "web/e2e/screenshots/EB-reaction-effects",
    intro,
    shots: [...cardShots, ...combatShots, ...stripShots],
  };
  writeFileSync(join(artifactDir(testInfo), "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
});
