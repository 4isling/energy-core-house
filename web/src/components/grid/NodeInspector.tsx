import type { GridNodeView, NodeReport } from "../../gridTypes";
import { TIER_EMOJI } from "../../gridTypes";

function fmtEur(v: number): string {
  return v.toLocaleString("fr-FR", { maximumFractionDigits: 0 }) + " €";
}

/** Inspecteur d'un nœud : stats du pas, actions joueur (actifs partagés,
 * îlotage) et liste des enfants pour le drill-down. Une **maison** est en
 * lecture seule (ce sont les NPC qui décident) ; on l'influence par le tarif. */
export function NodeInspector({
  node,
  report,
  children,
  childReports,
  onDrill,
  onIsland,
  onBuildSolar,
  onBuildWind,
  onBuildBattery,
}: {
  node: GridNodeView;
  report: NodeReport | undefined;
  children: GridNodeView[];
  childReports: NodeReport[];
  onDrill: (id: number) => void;
  onIsland: (id: number, islanded: boolean) => void;
  onBuildSolar: (id: number, kwc: number) => void;
  onBuildWind: (id: number) => void;
  onBuildBattery: (id: number, kwh: number) => void;
}) {
  const isHousehold = node.tier === "Maison";
  const isDistrict = node.tier === "Quartier";

  return (
    <div className="node-inspector">
      <header className="ni-head">
        <h3>
          {TIER_EMOJI[node.tier]} {node.name}
          {node.islanded && <span className="badge islanded"> ⛔ Îloté</span>}
          {report?.blackout && <span className="badge blackout-alert"> ⚠️ Black-out</span>}
        </h3>
        <span className="ni-balance" title="Portefeuille du nœud">
          {fmtEur(node.balance_eur)}
        </span>
      </header>

      {/* Flux du pas */}
      <div className="dashboard">
        <Stat label="Charge" value={`${node.load_kw.toFixed(2)} kW`} />
        <Stat label="Solaire" value={`${(report?.solar_kw ?? 0).toFixed(1)} kW`} />
        <Stat label="Éolien" value={`${(report?.wind_kw ?? 0).toFixed(1)} kW`} />
        <Stat label="Thermique" value={`${(report?.thermal_kw ?? 0).toFixed(1)} kW`} />
        <Stat label="Import" value={`${(report?.import_kw ?? 0).toFixed(2)} kW`} />
        <Stat label="Export" value={`${(report?.export_kw ?? 0).toFixed(2)} kW`} />
        <Stat label="P2P voisins" value={`${(report?.p2p_kw ?? 0).toFixed(2)} kW`} />
        <Stat label="Non fourni" value={`${(report?.unmet_kw ?? 0).toFixed(2)} kW`} warn={(report?.unmet_kw ?? 0) > 0.01} />
        <Gauge label="Batterie" pct={report?.soc_pct ?? 0} />
      </div>

      {/* Auto-production installée */}
      <p className="ni-assets">
        ☀️ {node.solar_kwc.toFixed(1)} kWc · 🔋 {node.battery_kwh.toFixed(0)} kWh ·
        🌬️ {node.wind_count} · 🛢️ {node.thermal_count}
      </p>

      {/* Maison : lecture seule + détails NPC ; sinon : actions joueur. */}
      {isHousehold ? (
        <div className="ni-household">
          <p>
            Autonomie souhaitée :{" "}
            <strong>{Math.round(node.autonomy_pref * 100)} %</strong> · Revenu :{" "}
            {fmtEur(node.income_eur_per_day)}/j
          </p>
          <p className="muted">
            Ce foyer est piloté par ses habitants (NPC) : il investit lui-même
            selon le tarif. Influence-le indirectement via le tarif national.
          </p>
        </div>
      ) : (
        <div className="ni-actions">
          <h4>Actifs partagés {isDistrict ? "du quartier" : "du national"}</h4>
          <div className="action-row">
            <button onClick={() => onBuildSolar(node.id, 50)}>☀️ +50 kWc solaire</button>
            <button onClick={() => onBuildBattery(node.id, 100)}>🔋 +100 kWh batterie</button>
            <button onClick={() => onBuildWind(node.id)}>🌬️ +1 éolienne</button>
          </div>
          {node.has_uplink && (
            <label className="island-toggle">
              <input
                type="checkbox"
                checked={node.islanded}
                onChange={(e) => onIsland(node.id, e.target.checked)}
              />
              Îloter ce nœud (le déconnecter de son parent)
            </label>
          )}
        </div>
      )}

      {/* Enfants : drill-down */}
      {children.length > 0 && (
        <div className="ni-children">
          <h4>
            {isDistrict ? "Maisons du quartier" : "Quartiers raccordés"} ({children.length})
          </h4>
          <div className="child-grid">
            {children.map((c) => {
              const r = childReports[c.id];
              return (
                <button key={c.id} className="child-card" onClick={() => onDrill(c.id)}>
                  <span className="cc-name">
                    {TIER_EMOJI[c.tier]} {c.name}
                  </span>
                  <span className="cc-line">{c.load_kw.toFixed(1)} kW charge</span>
                  <span className="cc-flags">
                    {c.islanded && <span className="badge islanded">⛔</span>}
                    {r?.blackout && <span className="badge blackout-alert">⚠️</span>}
                    {c.solar_kwc > 0 && <span className="badge ok-badge">☀️</span>}
                    {c.battery_kwh > 0 && <span className="badge ok-badge">🔋</span>}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}
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

function Gauge({ label, pct }: { label: string; pct: number }) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div className="stat gauge">
      <span className="stat-label">{label}</span>
      <div className="gauge-track">
        <div className="gauge-fill" style={{ width: `${clamped}%`, background: `hsl(${clamped * 1.2}, 70%, 45%)` }} />
      </div>
      <span className="stat-value">{clamped.toFixed(0)} %</span>
    </div>
  );
}
