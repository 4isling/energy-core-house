import { useGame } from "./useGame";
import { Dashboard } from "./components/Dashboard";
import { ProductionChart } from "./components/ProductionChart";
import { BuildMenu } from "./components/BuildMenu";
import { HouseSchematic } from "./components/HouseSchematic";

const STARTING_BUDGET = 40_000; // € — échelle maison
const SEED = 1234;

export function App() {
  const game = useGame(STARTING_BUDGET, SEED);

  if (!game.ready || !game.report) {
    return <div className="loading">Chargement du moteur…</div>;
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>🏡 Maison autonome</h1>
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

      <Dashboard report={game.report} residents={game.residents} />

      <div className="main-grid">
        <div className="left-col">
          <HouseSchematic report={game.report} residents={game.residents} />
          <ProductionChart history={game.history} />
        </div>
        <BuildMenu game={game} budget={game.report.budget_eur} />
      </div>
    </div>
  );
}
