import { render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { InvasionLandingTray } from "./InvasionLandingTray.tsx";
import { galleryBoard } from "../dev/galleryBoard.ts";
import { actor } from "../dev/decisionGalleryCases.ts";

const css = readFileSync(resolve(__dirname, "../index.css"), "utf8");

afterEach(() => {
  document.head.querySelectorAll("style[data-test-css]").forEach((style) => style.remove());
});

// Run s1-07-2324: Sardakk's hero skipped straight to Commit Ground Forces, so the board had no
// invasion and the standalone tray rendered inside `.app-shell__overlays` (pointer-events: none).
// Nothing re-enabled pointer events on it: Confirm landings could not be clicked and the game stalled.
it("takes pointer events back inside the overlay layer when rendered standalone", () => {
  const style = document.createElement("style");
  style.dataset.testCss = "";
  style.textContent = css;
  document.head.appendChild(style);
  const overlays = document.createElement("div");
  overlays.className = "app-shell__overlays";
  document.body.appendChild(overlays);
  render(
    <InvasionLandingTray
      choice={{
        actor,
        nonce: "landing-1",
        prompt: "commit ground forces in 18",
        context: { subtype: "commit_ground_forces", target: { System: "18" } },
        options: [
          {
            id: "commit|0|jord",
            label: "land infantry on jord",
            kind: "commit",
            payload: { planet: "jord", unit: "infantry", damaged: false },
          },
          { id: "done_committing", label: "commit no more ground forces", kind: "decline" },
        ],
      }}
      board={{ ...galleryBoard, invasion: undefined, active_system: "18" }}
      viewerSeat={actor}
      onSubmit={vi.fn()}
      onClose={vi.fn()}
    />,
    { container: overlays },
  );
  expect(getComputedStyle(overlays).pointerEvents).toBe("none");
  expect(getComputedStyle(screen.getByTestId("invasion-landing-tray")).pointerEvents).toBe("auto");
});
