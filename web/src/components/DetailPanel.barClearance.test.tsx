import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { DetailPanel } from "./DetailPanel.tsx";

const css = readFileSync(resolve(__dirname, "../index.css"), "utf8");
const root = () => document.documentElement.style.getPropertyValue("--bar-clear");

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("DetailPanel clears the confirm bar", () => {
  it("publishes how far the activation bar reaches up from the bottom edge, and drops it with the panel", async () => {
    const bar = document.createElement("aside");
    bar.className = "system-activation-bar";
    bar.getBoundingClientRect = () => ({ top: window.innerHeight - 90, height: 66, bottom: window.innerHeight - 24, left: 0, right: 0, width: 400, x: 0, y: 0, toJSON: () => ({}) });
    document.body.appendChild(bar);
    const { unmount } = render(
      <DetailPanel title="System A #1" onClose={() => undefined}>
        <p>a</p>
      </DetailPanel>,
    );
    expect(root()).toBe("102px");
    // A bar that grows (an error line) moves the value when something in the page changes.
    bar.getBoundingClientRect = () => ({ top: window.innerHeight - 150, height: 126, bottom: window.innerHeight - 24, left: 0, right: 0, width: 400, x: 0, y: 0, toJSON: () => ({}) });
    document.body.appendChild(document.createElement("div"));
    await waitFor(() => expect(root()).toBe("162px"));
    unmount();
    expect(root()).toBe("");
  });

  it("is 0 without a bar, and the panel's bottom inset never goes below its old value", () => {
    render(
      <DetailPanel title="System A #1" onClose={() => undefined}>
        <p>a</p>
      </DetailPanel>,
    );
    expect(screen.getByTestId("detail-panel")).toBeInTheDocument();
    expect(root()).toBe("0px");
    expect(css).toMatch(/bottom: max\(250px, var\(--bar-clear, 0px\)\) !important/);
    expect(css).toMatch(/bottom: max\(85px, var\(--bar-clear, 0px\)\) !important/);
  });
});
