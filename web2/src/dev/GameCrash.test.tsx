import { render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { readCrash } from "../session/savedGame";
import { GameCrash } from "./GameCrash";

function Broken(): never {
  throw new Error("boom");
}

it("shows the error of a render in place of a blank page, with Export, and keeps it next to the game", async () => {
  // React reports the error it caught on the console.
  vi.spyOn(console, "error").mockImplementation(() => {});
  render(
    <GameCrash locate={() => Promise.resolve("g")}>
      <Broken />
    </GameCrash>,
  );
  expect(screen.getByRole("alert")).toHaveTextContent("The game stopped with an error: boom");
  expect(screen.getByRole("button", { name: "Export" })).toBeVisible();
  await waitFor(() => expect(readCrash(localStorage, "g")?.message).toBe("boom"));
  expect(readCrash(localStorage, "g")?.source).toBe("render");
  expect(readCrash(localStorage, "g")?.componentStack).toContain("Broken");
});
