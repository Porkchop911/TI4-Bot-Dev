import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMapPreviewCache } from "../hooks/useMapCatalog.ts";
import { fixtureTemplates } from "../dev/mapPickerFixtures.ts";
import { OfflineSetupForm } from "./OfflineSetupForm.tsx";
import { effectiveChoice, mapLabel } from "./OfflineMapPicker.tsx";
import { createMockLauncher, MOCK_FACTIONS } from "./mockLauncher.ts";
import { aliasPlayers, buildSetup, DEFAULT_DRAFT, mapForPlayers } from "./setup.ts";
import { engineSetup } from "./local/launcher.ts";
import { createMapSource } from "./local/mapSource.ts";
import type { FromWorker, HostPort, ToWorker } from "@ti4/shared/engine/protocol.ts";
import type { LoadedEngine } from "./local/session.ts";
import type { OfflineSetup } from "./types.ts";

afterEach(() => {
  cleanup();
  clearMapPreviewCache();
});

const draft = { ...DEFAULT_DRAFT, nickname: "Ada" };

describe("map choice in the setup", () => {
  it("leaves the map out when untouched (the engine recommends one)", () => {
    const r = buildSetup(draft, false, MOCK_FACTIONS);
    expect(r.ok && "map" in r.setup).toBe(false);
  });
  it("passes a template or random through", () => {
    const t = buildSetup({ ...draft, map: { kind: "template", alias: "4pStandard" } }, false, MOCK_FACTIONS);
    expect(t.ok && t.setup.map).toEqual({ kind: "template", alias: "4pStandard" });
    const r = buildSetup({ ...draft, map: { kind: "random" } }, false, MOCK_FACTIONS);
    expect(r.ok && r.setup.map).toEqual({ kind: "random" });
  });
  it("drops a layout made for another table size", () => {
    expect(aliasPlayers("6pStandard")).toBe(6);
    expect(aliasPlayers("Odd")).toBeNull();
    expect(mapForPlayers({ kind: "template", alias: "6pStandard" }, 4)).toBeNull();
    expect(mapForPlayers({ kind: "template", alias: "4pStandard" }, 4)).not.toBeNull();
    expect(mapForPlayers({ kind: "random" }, 3)).toEqual({ kind: "random" });
    const r = buildSetup({ ...draft, playerCount: 4, map: { kind: "template", alias: "6pStandard" } }, false, MOCK_FACTIONS);
    expect(r.ok && "map" in r.setup).toBe(false);
  });
  it("reaches the engine's setup as Setup.map, and not at all when untouched", () => {
    const base: OfflineSetup = { playerCount: 4, humanSeat: null, humanFaction: null, nickname: "Ada", seed: 1 };
    expect(engineSetup(base, "off_x")).toEqual({ game_id: "off_x", seed: 1, players: 4 });
    expect(engineSetup({ ...base, map: { kind: "template", alias: "4pStandard" } }, "off_x")).toMatchObject({ map: "4pStandard" });
    expect(engineSetup({ ...base, map: { kind: "random" } }, "off_x")).toMatchObject({ map: "random" });
  });
  it("labels the choice", () => {
    const templates = fixtureTemplates(4);
    expect(effectiveChoice(null, templates)).toEqual({ kind: "template", alias: "4pStandard" });
    expect(mapLabel(null, templates)).toBe("Standard (recommended)");
    expect(mapLabel(null, null)).toBe("Recommended layout");
    expect(mapLabel({ kind: "random" }, templates)).toBe("Random map");
    expect(mapLabel({ kind: "template", alias: "4pBeMyNeighbor" }, templates)).toBe("Be my neighbor");
  });
});

describe("the form with the map picker", () => {
  const maps = createMockLauncher().maps!;
  const form = (onStart = vi.fn(), props: object = {}) =>
    render(<OfflineSetupForm factions={MOCK_FACTIONS} maps={maps} onStart={onStart} allowSeed={false} initial={{ nickname: "Ada" }} {...props} />);

  it("has no map row without a map source", () => {
    render(<OfflineSetupForm factions={MOCK_FACTIONS} onStart={vi.fn()} allowSeed={false} initial={{ nickname: "Ada" }} />);
    expect(screen.queryByTestId("offline-map-row")).toBeNull();
  });
  it("starts without a map when the picker is not used", () => {
    const onStart = vi.fn();
    form(onStart);
    expect(screen.getByTestId("offline-map-name").textContent).toBe("Recommended layout");
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(onStart.mock.calls[0][0]).not.toHaveProperty("map");
  });
  it("picks a layout and random, and previews it", async () => {
    const onStart = vi.fn();
    form(onStart);
    fireEvent.click(screen.getByTestId("offline-map-button"));
    await screen.findByTestId("map-card-4pBeMyNeighbor");
    expect(screen.getByTestId("map-card-4pStandard").getAttribute("aria-pressed")).toBe("true");
    await screen.findByTestId("map-preview-board");
    fireEvent.click(screen.getByTestId("map-card-4pHyperlanes"));
    await waitFor(() => expect(screen.getByTestId("map-card-4pHyperlanes").getAttribute("aria-pressed")).toBe("true"));
    fireEvent.click(screen.getByTestId("map-picker-done"));
    expect(screen.getByTestId("offline-map-name").textContent).toBe("Hyperlanes");
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(onStart.mock.calls[0][0].map).toEqual({ kind: "template", alias: "4pHyperlanes" });

    fireEvent.click(screen.getByTestId("offline-map-button"));
    fireEvent.click(await screen.findByTestId("map-card-random"));
    fireEvent.click(screen.getByTestId("map-picker-done"));
    expect(screen.getByTestId("offline-map-name").textContent).toBe("Random map");
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(onStart.mock.calls[1][0].map).toEqual({ kind: "random" });
  });
  it("forgets a layout when the table size changes", async () => {
    const onStart = vi.fn();
    form(onStart);
    fireEvent.click(screen.getByTestId("offline-map-button"));
    fireEvent.click(await screen.findByTestId("map-card-4pBeMyNeighbor"));
    fireEvent.click(screen.getByTestId("map-picker-done"));
    fireEvent.change(screen.getByTestId("offline-player-count"), { target: { value: "5" } });
    expect(screen.getByTestId("offline-map-name").textContent).toBe("Recommended layout");
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(onStart.mock.calls[0][0]).not.toHaveProperty("map");
  });
  it("shows the error with Retry, and the empty case", async () => {
    const failing = createMockLauncher({ mapsState: "error" }).maps!;
    const { unmount } = form(vi.fn(), { maps: failing });
    fireEvent.click(screen.getByTestId("offline-map-button"));
    await screen.findByTestId("map-picker-error");
    unmount();
    const none = createMockLauncher({ mapsState: "empty" }).maps!;
    form(vi.fn(), { maps: none });
    fireEvent.click(screen.getByTestId("offline-map-button"));
    await screen.findByTestId("map-picker-empty");
    expect(screen.getByTestId("map-card-random")).toBeTruthy();
  });
});

describe("the engine's map source", () => {
  const engine = {} as LoadedEngine;
  function fakePort() {
    const posted: ToWorker[] = [];
    let listener: (m: FromWorker) => void = () => undefined;
    let terminated = 0;
    const port: HostPort = {
      post: (m) => {
        posted.push(m);
        if (m.t !== "query") return;
        const request = m.request as { query: string };
        const value =
          request.query === "describe" ? { maps: { "4": [{ alias: "4pStandard" }] } } : { picture: request };
        queueMicrotask(() => listener({ t: "reply", id: m.id, value }));
      },
      listen: (l) => (listener = l),
      terminate: () => terminated++,
    };
    return { port, posted, terminated: () => terminated };
  }
  it("asks describe once, previews per choice, and ends the worker when idle", async () => {
    vi.useFakeTimers();
    try {
      const fake = fakePort();
      const opened = vi.fn(async () => fake.port);
      const source = createMapSource({ load: async () => engine, port: opened, idleMs: 1000 });
      expect(await source.catalog(4)).toEqual([{ alias: "4pStandard" }]);
      expect(await source.catalog(5)).toEqual([]);
      expect(fake.posted.filter((m) => m.t === "query")).toHaveLength(1);
      const picture = await source.preview({ kind: "random" }, 4, 2);
      expect(picture).toEqual({ picture: { query: "map_preview", choice: { kind: "random" }, player_count: 4, variant: 2 } });
      expect(opened).toHaveBeenCalledTimes(1);
      expect(fake.terminated()).toBe(0);
      await vi.advanceTimersByTimeAsync(1500);
      expect(fake.terminated()).toBe(1);
      await source.preview({ kind: "random" }, 4, 0);
      expect(opened).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
