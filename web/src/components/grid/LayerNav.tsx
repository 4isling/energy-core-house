import type { GridNodeView } from "../../gridTypes";
import { TIER_EMOJI } from "../../gridTypes";

/** Fil d'Ariane : remonte la chaîne des parents du nœud sélectionné et permet de
 * naviguer National ⇄ Quartier ⇄ Maison par clic. */
export function LayerNav({
  nodes,
  selectedId,
  onSelect,
}: {
  nodes: GridNodeView[];
  selectedId: number;
  onSelect: (id: number) => void;
}) {
  // Construit le chemin racine → … → nœud sélectionné.
  const path: GridNodeView[] = [];
  let cur: GridNodeView | undefined = nodes[selectedId];
  while (cur) {
    path.unshift(cur);
    cur = cur.parent != null ? nodes[cur.parent] : undefined;
  }

  return (
    <nav className="layer-nav">
      {path.map((n, i) => (
        <span key={n.id} className="layer-crumb">
          <button
            className={i === path.length - 1 ? "crumb active" : "crumb"}
            onClick={() => onSelect(n.id)}
          >
            {TIER_EMOJI[n.tier] ?? ""} {n.name}
            {n.islanded ? " ⛔" : ""}
          </button>
          {i < path.length - 1 && <span className="crumb-sep">›</span>}
        </span>
      ))}
    </nav>
  );
}
