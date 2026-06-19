import type { TickReport } from "../types";

// Épaisseur de trait proportionnelle à la puissance (kW), bornée.
function strokeFor(kw: number): number {
  return Math.max(0, Math.min(14, kw * 0.8));
}

function Flow({
  x1,
  y1,
  x2,
  y2,
  kw,
  color,
}: {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  kw: number;
  color: string;
}) {
  const w = strokeFor(kw);
  if (w <= 0.05) return null;
  return (
    <line
      x1={x1}
      y1={y1}
      x2={x2}
      y2={y2}
      stroke={color}
      strokeWidth={w}
      strokeLinecap="round"
      opacity={0.85}
      className="flow-line"
    />
  );
}

function Node({ x, y, icon, label }: { x: number; y: number; icon: string; label: string }) {
  return (
    <g transform={`translate(${x}, ${y})`}>
      <circle r={24} fill="#1b2530" stroke="#3a4a58" strokeWidth={2} />
      <text textAnchor="middle" y={6} fontSize={22}>
        {icon}
      </text>
      <text textAnchor="middle" y={42} className="node-label">
        {label}
      </text>
    </g>
  );
}

// Couleur du confort : rouge (bas) → vert (haut).
function comfortColor(pct: number): string {
  const c = Math.max(0, Math.min(100, pct));
  return `hsl(${c * 1.2}, 70%, 45%)`;
}

export function ColonyView({ report }: { report: TickReport }) {
  const hubX = 290;
  const hubY = 150;
  return (
    <div className="house-card">
      <h2>Micro-réseau du village</h2>
      <svg viewBox="0 0 560 300" className="house-svg">
        {/* Flux des sources partagées vers le hub du village (épaisseur = kW) */}
        <Flow x1={90} y1={55} x2={hubX} y2={hubY} kw={report.solar_kw} color="#f5a623" />
        <Flow x1={90} y1={150} x2={hubX} y2={hubY} kw={report.wind_kw} color="#4a90d9" />
        <Flow x1={90} y1={245} x2={hubX} y2={hubY} kw={Math.max(0, report.battery_kw)} color="#2ecc71" />
        <Flow x1={500} y1={55} x2={hubX} y2={hubY} kw={report.import_kw} color="#95a5a6" />
        <Flow x1={hubX} y1={hubY} x2={500} y2={245} kw={report.export_kw} color="#16a085" />

        {/* Sources mutualisées */}
        <Node x={90} y={55} icon="☀️" label={`${report.solar_kw.toFixed(1)} kW`} />
        <Node x={90} y={150} icon="🌬️" label={`${report.wind_kw.toFixed(1)} kW`} />
        <Node x={90} y={245} icon="🔋" label={`${report.soc_pct.toFixed(0)} %`} />
        <Node x={500} y={55} icon="🔌" label={`${report.import_kw.toFixed(1)} kW`} />
        <Node x={500} y={245} icon="📤" label={`${report.export_kw.toFixed(1)} kW`} />

        {/* Hub du village */}
        <g>
          <circle
            cx={hubX}
            cy={hubY}
            r={46}
            fill={report.blackout ? "#3a2222" : "#22303a"}
            stroke={report.blackout ? "#e74c3c" : "#3aa6a6"}
            strokeWidth={3}
          />
          <text x={hubX} y={hubY - 6} textAnchor="middle" fontSize={26}>
            🏘️
          </text>
          <text x={hubX} y={hubY + 20} textAnchor="middle" className="house-load">
            {report.load_kw.toFixed(1)} kW
          </text>
          {report.blackout && (
            <text x={hubX} y={hubY + 70} textAnchor="middle" className="house-blackout">
              BLACK-OUT
            </text>
          )}
        </g>
      </svg>

      {/* Grille des bâtiments */}
      <div className="building-grid">
        {report.buildings.map((b) => (
          <div key={b.id} className={`building-card${b.avg_comfort_pct < 40 ? " distress" : ""}`}>
            <div className="building-head">
              <span className="building-name">{b.name}</span>
              <span className="building-pop">👤 {b.resident_count}</span>
            </div>
            <div className="building-load">{b.load_kw.toFixed(2)} kW</div>
            <div className="comfort-track">
              <div
                className="comfort-fill"
                style={{
                  width: `${Math.max(0, Math.min(100, b.avg_comfort_pct))}%`,
                  background: comfortColor(b.avg_comfort_pct),
                }}
              />
            </div>
            <div className="building-comfort">
              {b.avg_comfort_pct < 40 ? "😣" : "🙂"} confort {b.avg_comfort_pct.toFixed(0)} %
            </div>
          </div>
        ))}
        {report.buildings.length === 0 && (
          <p className="muted">Aucun bâtiment — construisez un foyer pour démarrer le village.</p>
        )}
      </div>
    </div>
  );
}
