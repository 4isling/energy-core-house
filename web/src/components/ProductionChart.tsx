import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { TickReport } from "../types";

// Construit les points du graphe à partir de l'historique glissant.
function toRows(history: TickReport[]) {
  return history.map((r, i) => ({
    t: i,
    Solaire: round(r.solar_kw),
    Éolien: round(r.wind_kw),
    Hydro: round(r.hydro_kw),
    Thermique: round(r.thermal_kw),
    Charge: round(r.load_kw),
  }));
}

const round = (v: number) => Math.round(v * 100) / 100;

export function ProductionChart({ history }: { history: TickReport[] }) {
  const data = toRows(history);
  return (
    <div className="chart-card">
      <h2>Production vs charge (kW)</h2>
      <ResponsiveContainer width="100%" height={240}>
        <AreaChart data={data} margin={{ top: 8, right: 12, bottom: 0, left: -8 }}>
          <CartesianGrid strokeDasharray="3 3" opacity={0.2} />
          <XAxis dataKey="t" tick={false} />
          <YAxis width={40} />
          <Tooltip />
          <Legend />
          <Area type="monotone" dataKey="Solaire" stackId="prod" stroke="#f5a623" fill="#f5a623" />
          <Area type="monotone" dataKey="Éolien" stackId="prod" stroke="#4a90d9" fill="#4a90d9" />
          <Area type="monotone" dataKey="Hydro" stackId="prod" stroke="#3aa6a6" fill="#3aa6a6" />
          <Area type="monotone" dataKey="Thermique" stackId="prod" stroke="#9b59b6" fill="#9b59b6" />
          <Line type="monotone" dataKey="Charge" stroke="#e74c3c" strokeWidth={2} dot={false} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
