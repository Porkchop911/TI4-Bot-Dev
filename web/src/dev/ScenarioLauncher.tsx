import React, { useEffect, useState } from "react";
import "./ScenarioLauncher.css";

interface ScenarioSummary {
  id: string;
  title: string;
  category: string;
  description: string;
  player_count: number;
  human_faction: string;
  opponent_factions: string[];
}

interface LaunchScenarioResponse {
  game_id: string;
  player_session: string;
  player_id: string;
  scenario_id: string;
}

export const ScenarioLauncher: React.FC = () => {
  const [scenarios, setScenarios] = useState<ScenarioSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [seed, setSeed] = useState<string>("");
  const [launchingId, setLaunchingId] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/dev/scenarios")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}: ${res.statusText}`);
        return res.json();
      })
      .then((data: ScenarioSummary[]) => {
        setScenarios(data);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  }, []);

  const handleLaunch = async (scenarioId: string) => {
    setLaunchingId(scenarioId);
    setError(null);
    try {
      const parsedSeed = seed.trim() ? Number(seed.trim()) : undefined;
      const res = await fetch("/api/dev/scenarios/launch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          scenario_id: scenarioId,
          seed: Number.isFinite(parsedSeed) ? parsedSeed : undefined,
        }),
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `Failed to launch scenario (HTTP ${res.status})`);
      }

      const launched: LaunchScenarioResponse = await res.json();
      sessionStorage.setItem(`ti4.player-session:${launched.game_id}`, launched.player_session);
      window.location.href = `/games/${encodeURIComponent(launched.game_id)}`;
    } catch (err: any) {
      setError(err.message ?? "Unknown error launching scenario");
      setLaunchingId(null);
    }
  };

  return (
    <div className="scenario-launcher" data-testid="scenario-launcher">
      <header className="scenario-launcher__header panel">
        <div className="scenario-launcher__header-top">
          <div>
            <h1>
              Live Dev Scenarios{" "}
              <small>Development only · Authoritative engine & WebSockets · Solo vs Bots</small>
            </h1>
          </div>
          <div>
            <a href="/dev/decisions" className="button button--secondary">
              &larr; Synthetic Decision Gallery
            </a>
          </div>
        </div>
        <p>
          Simulate the complete flow against the authoritative Rust game engine. Actions advance the
          real game loop, roll dice, and trigger bot opponent responses in real-time.
        </p>
        <div className="scenario-launcher__config">
          <label>
            <span>RNG Seed (optional):</span>
            <input
              type="number"
              placeholder="Random"
              value={seed}
              onChange={(e) => setSeed(e.target.value)}
            />
          </label>
        </div>
      </header>

      {error && (
        <div className="session-error" role="alert" style={{ marginBottom: 16 }}>
          {error}
        </div>
      )}

      {loading ? (
        <p>Loading available scenarios from server…</p>
      ) : (
        <main className="scenario-launcher__grid">
          {scenarios.map((sc) => {
            const isLaunching = launchingId === sc.id;
            const badgeClass =
              sc.category.toLowerCase() === "combat"
                ? "scenario-card__badge scenario-card__badge--combat"
                : "scenario-card__badge scenario-card__badge--tactical";

            return (
              <div key={sc.id} className="scenario-card">
                <div className="scenario-card__header">
                  <h3>{sc.title}</h3>
                  <span className={badgeClass}>{sc.category}</span>
                </div>
                <p className="scenario-card__desc">{sc.description}</p>
                <div className="scenario-card__meta">
                  <div className="scenario-card__meta-item">
                    <strong>Your Faction:</strong>
                    <span>{sc.human_faction}</span>
                  </div>
                  <div className="scenario-card__meta-item">
                    <strong>Bot Opponents:</strong>
                    <span>{sc.opponent_factions.join(", ")}</span>
                  </div>
                  <div className="scenario-card__meta-item">
                    <strong>Player Count:</strong>
                    <span>
                      {sc.player_count} players (1 Human, {sc.player_count - 1} Bots)
                    </span>
                  </div>
                </div>
                <div className="scenario-card__action">
                  <button
                    type="button"
                    className="button button--primary"
                    disabled={launchingId !== null}
                    onClick={() => handleLaunch(sc.id)}
                  >
                    {isLaunching ? "Launching Scenario…" : "Launch Scenario"}
                  </button>
                </div>
              </div>
            );
          })}
        </main>
      )}
    </div>
  );
};
