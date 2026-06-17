import type { ResidentView, TickReport } from "../types";

// Épaisseur de trait proportionnelle à la puissance (kW), bornée.
function strokeFor(kw: number): number {
  return Math.max(0, Math.min(12, kw * 1.2));
}

interface FlowProps {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  kw: number;
  color: string;
}

function Flow({ x1, y1, x2, y2, kw, color }: FlowProps) {
  const w = strokeFor(kw);
  if (w <= 0.05) return null;
  return (
    <g>
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
    </g>
  );
}

export function HouseSchematic({
  report,
  residents,
}: {
  report: TickReport;
  residents: ResidentView[];
}) {
  const houseX = 320;
  const houseY = 170;
  return (
    <div className="house-card">
      <h2>Schéma de la maison</h2>
      <svg viewBox="0 0 560 320" className="house-svg">
        {/* Flux des sources vers la maison (épaisseur = kW) */}
        <Flow x1={90} y1={60} x2={houseX} y2={houseY} kw={report.solar_kw} color="#f5a623" />
        <Flow x1={90} y1={170} x2={houseX} y2={houseY} kw={report.wind_kw} color="#4a90d9" />
        <Flow
          x1={90}
          y1={280}
          x2={houseX}
          y2={houseY}
          kw={Math.max(0, report.battery_kw)}
          color="#2ecc71"
        />
        <Flow x1={530} y1={60} x2={houseX} y2={houseY} kw={report.import_kw} color="#95a5a6" />
        <Flow
          x1={houseX}
          y1={houseY}
          x2={530}
          y2={280}
          kw={report.export_kw}
          color="#16a085"
        />

        {/* Sources */}
        <Node x={90} y={60} icon="☀️" label={`${report.solar_kw.toFixed(1)} kW`} />
        <Node x={90} y={170} icon="🌬️" label={`${report.wind_kw.toFixed(1)} kW`} />
        <Node x={90} y={280} icon="🔋" label={`${report.soc_pct.toFixed(0)} %`} />
        <Node x={530} y={60} icon="🔌" label={`${report.import_kw.toFixed(1)} kW`} />
        <Node x={530} y={280} icon="📤" label={`${report.export_kw.toFixed(1)} kW`} />

        {/* La maison */}
        <g>
          <rect
            x={houseX - 55}
            y={houseY - 30}
            width={110}
            height={90}
            rx={6}
            fill={report.blackout ? "#3a2222" : "#22303a"}
            stroke={report.blackout ? "#e74c3c" : "#3aa6a6"}
            strokeWidth={3}
          />
          <polygon
            points={`${houseX - 65},${houseY - 30} ${houseX},${houseY - 70} ${houseX + 65},${houseY - 30}`}
            fill={report.blackout ? "#552b2b" : "#2b4a52"}
          />
          <text x={houseX} y={houseY + 15} textAnchor="middle" className="house-load">
            {report.load_kw.toFixed(2)} kW
          </text>
          {report.blackout && (
            <text x={houseX} y={houseY + 45} textAnchor="middle" className="house-blackout">
              BLACK-OUT
            </text>
          )}
        </g>

        {/* Avatars des habitants */}
        {residents.map((r, i) => (
          <g key={i} transform={`translate(${houseX - 45 + i * 26}, ${houseY + 80})`}>
            <title>
              {r.name} ({r.profile}) — confort {r.comfort.toFixed(0)} %
            </title>
            <text textAnchor="middle" fontSize={20} opacity={r.comfort < 40 ? 0.4 : 1}>
              {r.comfort < 40 ? "😣" : "🙂"}
            </text>
          </g>
        ))}
      </svg>
    </div>
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
