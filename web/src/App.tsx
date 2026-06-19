import { useCallback, useState } from "react";
import { useGame } from "./useGame";
import { Dashboard } from "./components/Dashboard";
import { ProductionChart } from "./components/ProductionChart";
import { BuildMenu } from "./components/BuildMenu";
import { MapView } from "./map/MapView";
import type { BuildTool, TileInfo } from "./types";

const STARTING_BUDGET = 250_000; // € — échelle village (foyers + réseau + hydro)
const SEED = 1234;

const PLACE_ERROR: Record<number, string> = {
  1: "Budget insuffisant",
  2: "Hors de la carte",
  3: "Tuile déjà occupée",
  4: "Terrain incompatible (hydro = rivière, pas de montagne)",
  5: "Élément inconnu",
};

export function App() {
  const game = useGame(STARTING_BUDGET, SEED);
  const [tool, setTool] = useState<BuildTool | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [hover, setHover] = useState<TileInfo | null>(null);

  const onPlace = useCallback(
    (x: number, y: number) => {
      if (!tool) {
        setFlash("Choisissez d'abord un élément à construire.");
        return;
      }
      const code = game.placeAt(tool, x, y);
      setFlash(code === 0 ? `${tool.label} posé en (${x}, ${y}).` : PLACE_ERROR[code] ?? "Échec");
    },
    [tool, game],
  );

  const onHoverTile = useCallback(
    (x: number, y: number) => setHover(game.tileInfo(x, y)),
    [game],
  );

  if (!game.ready || !game.report || !game.terrain) {
    return <div className="loading">Chargement du moteur…</div>;
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>🏘️ Village énergie</h1>
        <div className="header-controls">
          <button onClick={game.togglePause}>
            {game.paused ? "▶︎ Reprendre" : "⏸ Pause"}
          </button>
          <label className="grid-toggle">
            <input
              type="checkbox"
              checked={game.gridConnected}
              onChange={(e) => game.setGridConnected(e.target.checked)}
            />
            Raccordé au réseau
          </label>
        </div>
      </header>

      <Dashboard report={game.report} />

      <div className="main-grid">
        <div className="left-col">
          <div className="map-wrap">
            <MapView
              terrain={game.terrain}
              placements={game.placements}
              selectedTool={tool}
              onPlace={onPlace}
              onHoverTile={onHoverTile}
            />
            <div className="map-overlay">
              {flash && <span className="map-flash">{flash}</span>}
              {hover && (
                <span className="map-tile">
                  {hover.ground} ({hover.x},{hover.y}) · vent ×{hover.wind_factor.toFixed(2)} ·
                  soleil ×{hover.solar_factor.toFixed(2)}
                  {hover.is_water ? ` · débit ×${hover.water_factor.toFixed(2)}` : ""}
                  {hover.occupied ? " · occupée" : ""}
                </span>
              )}
            </div>
          </div>
          <ProductionChart history={game.history} />
        </div>
        <BuildMenu
          game={game}
          budget={game.report.budget_eur}
          selectedTool={tool}
          onSelectTool={setTool}
        />
      </div>
    </div>
  );
}
