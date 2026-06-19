import { useState } from "react";
import type { GridNodeView, GridSummary } from "../../gridTypes";

function fmtEur(v: number): string {
  return v.toLocaleString("fr-FR", { maximumFractionDigits: 0 }) + " €";
}

/** Panneau du national : indicateurs de la « spirale de la mort » + réglage du
 * tarif national. Augmenter le tarif améliore le revenu par kWh mais rend
 * l'autoproduction des foyers plus rentable → ils décrochent → le revenu chute. */
export function SpiralPanel({
  summary,
  districts,
  onSetTariff,
}: {
  summary: GridSummary | null;
  districts: GridNodeView[]; // quartiers (enfants du national), pour lire le tarif courant
  onSetTariff: (importPrice: number, exportPrice: number) => void;
}) {
  // Tarif courant lu sur le premier quartier raccordé (sinon valeurs par défaut).
  const ref = districts[0];
  const [imp, setImp] = useState((ref?.import_price_eur_kwh ?? 0.2).toFixed(2));
  const [exp, setExp] = useState((ref?.export_price_eur_kwh ?? 0.1).toFixed(2));

  const dep = summary ? Math.round(summary.dependency_rate * 100) : 0;
  // Spirale : la dépendance basse + marge négative = le réseau s'effondre.
  const margin = summary?.national_margin_eur ?? 0;

  return (
    <div className="spiral-panel">
      <h3>🏛️ Réseau national</h3>
      <div className="dashboard">
        <Stat label="Trésorerie" value={fmtEur(summary?.national_balance_eur ?? 0)} warn={(summary?.national_balance_eur ?? 0) < 0} />
        <Stat label="Marge / pas" value={`${margin >= 0 ? "+" : ""}${fmtEur(margin)}`} warn={margin < 0} />
        <Stat label="Maisons" value={`${summary?.households ?? 0}`} />
        <Stat
          label="Décrochées"
          value={`${summary?.self_producing_households ?? 0}`}
          warn={(summary?.self_producing_households ?? 0) > 0}
        />
      </div>

      <div className="spiral-gauge">
        <span className="stat-label">Dépendance au réseau</span>
        <div className="gauge-track">
          <div
            className="gauge-fill"
            style={{ width: `${dep}%`, background: `hsl(${dep * 1.2}, 70%, 45%)` }}
          />
        </div>
        <span className="stat-value">{dep} %</span>
      </div>
      <p className="spiral-hint">
        {dep < 30
          ? "⚠️ Les foyers s'autonomisent : le revenu du réseau s'effondre (spirale de la mort)."
          : "Les foyers dépendent encore du réseau. Monter le tarif rapporte… mais pousse au décrochage."}
      </p>

      <div className="tariff-control">
        <h4>Tarif national (€/kWh)</h4>
        <label>
          Import
          <input type="number" step="0.01" min="0" value={imp} onChange={(e) => setImp(e.target.value)} />
        </label>
        <label>
          Export
          <input type="number" step="0.01" min="0" value={exp} onChange={(e) => setExp(e.target.value)} />
        </label>
        <button
          onClick={() => onSetTariff(parseFloat(imp) || 0, parseFloat(exp) || 0)}
        >
          Appliquer le tarif
        </button>
      </div>
    </div>
  );
}

function Stat({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div className={`stat${warn ? " warn" : ""}`}>
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
    </div>
  );
}
