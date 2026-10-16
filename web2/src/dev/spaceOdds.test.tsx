import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { SpaceOdds } from "../game/combat/SpaceOdds";
import { GameProvider } from "../game/context";
import battleUpdate from "../session/fixtures/update-battle.json";
import { setCount } from "../session/movementDraft";
import { type KeptOdds, oddsRequest } from "../session/movementOdds";
import { selectMovement } from "../session/select/movement";
import { selectSeats } from "../session/select/players";
import type { MovementFacts, SessionUpdate, SpaceOddsResponse } from "../session/wire";

// The odds of a space combat over an update of the real engine: seat a (Sol) activates the
// system of seat c and moves its war sun in (see `session/liveOdds.test.tsx`). The test is here
// because `dev` is the layer that may import both the session and the components.
const update = battleUpdate as unknown as SessionUpdate;
const facts = update.tactical as MovementFacts;
const warSun = setCount(facts, {}, "ship:01|1", 1);
const key = oddsRequest(facts, warSun)!.key;

const answer: SpaceOddsResponse = {
  simulations: 2000,
  attacker_win_rate: 0.72,
  defender_win_rate: 0.25,
  mutual_destruction_rate: 0.02,
  unresolved_rate: 0.01,
  average_rounds: 2,
  attacker_expected_survivors: { warsun: 0.96 },
  defender_expected_survivors: { cruiser: 0.4, destroyer: 0.04 },
  attacker_fielded: { warsun: 1 },
  defender_fielded: { cruiser: 2, destroyer: 1 },
};

function show(kept: KeptOdds | null) {
  const view = selectMovement(update, facts, warSun, [], kept).odds!;
  render(
    <GameProvider seats={selectSeats(update)} dispatch={() => {}}>
      <SpaceOdds view={view} />
    </GameProvider>,
  );
  return screen.getByRole("region", { name: "Space combat" });
}

const text = (element: HTMLElement) => element.textContent?.replace(/\s+/g, " ").trim();

it("has Win, Draw and Loss, and the survivors of each side by kind", () => {
  const odds = show({ key, response: answer });
  expect(odds).toHaveAttribute("aria-busy", "false");
  expect(text(odds)).toContain("Win 72% · Draw 3% · Loss 25%");
  const attacker = within(odds).getByLabelText(/^Survivors of Sol/);
  expect(text(attacker)).toBe("1.0 / 1");
  expect(within(attacker).getByTitle("War sun")).toBeInTheDocument();
  const defender = within(odds).getByLabelText(/^Survivors of (?!Sol)/);
  expect(text(defender)).toBe("0.4 / 2 0.0 / 1");
  expect(within(defender).getByTitle("Cruisers")).toBeInTheDocument();
  expect(within(defender).getByTitle("Destroyer")).toBeInTheDocument();
  expect(within(odds).queryByTitle("Computing")).toBeNull();
});

it("says what the odds count behind a hint", () => {
  const odds = show({ key, response: answer });
  fireEvent.click(within(odds).getByRole("button", { name: "About the odds" }));
  const note = screen.getByRole("note");
  expect(note).toHaveTextContent("Simulated · 2000 rollouts.");
  expect(note).toHaveTextContent("Left out: action cards");
});

it("dims the numbers of another movement while the answer for this one is on its way", () => {
  const odds = show({ key: "another movement", response: answer });
  expect(odds).toHaveAttribute("aria-busy", "true");
  expect(within(odds).getByTitle("Computing")).toBeInTheDocument();
  const numbers = within(odds).getByText("72%").closest(".opacity-50");
  expect(numbers).not.toBeNull();
  // The ships are those of this movement.
  expect(text(within(odds).getByLabelText(/^Survivors of Sol/))).toBe("1.0 / 1");
});

it("has no numbers before the first answer", () => {
  const odds = show(null);
  expect(text(odds)).toContain("Win … · Draw … · Loss …");
  expect(text(within(odds).getByLabelText(/^Survivors of Sol/))).toBe("… / 1");
});
