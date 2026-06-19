import { useState } from "react";
import { useGrid } from "./useGrid";
import { LayerNav } from "./components/grid/LayerNav";
import { SpiralPanel } from "./components/grid/SpiralPanel";
import { NodeInspector } from "./components/grid/NodeInspector";

const SEED = 1234;
const N_DISTRICTS = 3;
const HOUSES_PER_DISTRICT = 4;

/** Vue « Réseau » (multi-couches) : navigation drill-down National ⇄ Quartier ⇄
 * Maison, panneau de spirale au national, gestion des actifs partagés au
 * quartier, inspecteur de maison. Coexiste avec le jeu mono-carte (App). */
export function GridApp() {
  const grid = useGrid(SEED, N_DISTRICTS, HOUSES_PER_DISTRICT);
  const [selectedId, setSelectedId] = useState(0);

  if (!grid.ready) {
    return <div className="loading">Chargement du réseau…</div>;
  }

  const node = grid.nodes[selectedId];
  const report = grid.reports[selectedId];
  const children = node ? node.children.map((id) => grid.nodes[id]).filter(Boolean) : [];
  const districts = grid.nodes.filter((n) => n.parent === grid.root);

  return (
    <div className="grid-app">
      <div className="grid-toolbar">
        <LayerNav nodes={grid.nodes} selectedId={selectedId} onSelect={setSelectedId} />
        <button onClick={grid.togglePause}>
          {grid.paused ? "▶︎ Reprendre" : "⏸ Pause"}
        </button>
      </div>

      <div className="grid-main">
        {/* Au national : panneau de spirale + tarif. Sinon, rien ici. */}
        {node?.id === grid.root && (
          <SpiralPanel
            summary={grid.summary}
            districts={districts}
            onSetTariff={grid.setNationalTariff}
          />
        )}

        {node && (
          <NodeInspector
            node={node}
            report={report}
            children={children}
            childReports={grid.reports}
            onDrill={setSelectedId}
            onIsland={grid.islandNode}
            onBuildSolar={grid.buildSolar}
            onBuildWind={grid.buildWind}
            onBuildBattery={grid.buildBattery}
          />
        )}
      </div>
    </div>
  );
}
