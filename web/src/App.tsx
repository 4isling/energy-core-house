import { useGame } from "./useGame";
import { Dashboard } from "./components/Dashboard";
import { ProductionChart } from "./components/ProductionChart";
import { BuildMenu } from "./components/BuildMenu";
import { ColonyView } from "./components/ColonyView";

const STARTING_BUDGET = 120_000; // € — échelle village (plusieurs foyers + réseau)
const SEED = 1234;

export function App() {
  const game = useGame(STARTING_BUDGET, SEED);

  if (!game.ready || !game.report) {
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
          <ColonyView report={game.report} />
          <ProductionChart history={game.history} />
        </div>
        <BuildMenu game={game} budget={game.report.budget_eur} />
      </div>
    </div>
  );
}
