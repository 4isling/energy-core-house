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
import type { NodeReport } from "../../gridTypes";

const round = (v: number) => Math.round(v * 100) / 100;

/** Historique d'un nœud : production empilée (solaire/éolien/thermique) vs la
 * charge, avec import/export/P2P en lignes. La série est extraite de l'historique
 * global (un tableau de rapports par pas) pour le `nodeId` sélectionné. */
export function NodeChart({
  history,
  nodeId,
}: {
  history: NodeReport[][];
  nodeId: number;
}) {
  const data = history
    .map((tickReports, i) => {
      const r = tickReports[nodeId];
      if (!r) return null;
      return {
        t: i,
        Solaire: round(r.solar_kw),
        Éolien: round(r.wind_kw),
        Thermique: round(r.thermal_kw),
        Charge: round(r.load_kw),
        Import: round(r.import_kw),
        Export: round(r.export_kw),
        P2P: round(r.p2p_kw),
      };
    })
    .filter((x): x is NonNullable<typeof x> => x !== null);

  return (
    <div className="chart-card">
      <h2>Historique du nœud (kW)</h2>
      <ResponsiveContainer width="100%" height={220}>
        <AreaChart data={data} margin={{ top: 8, right: 12, bottom: 0, left: -8 }}>
          <CartesianGrid strokeDasharray="3 3" opacity={0.2} />
          <XAxis dataKey="t" tick={false} />
          <YAxis width={40} />
          <Tooltip />
          <Legend />
          <Area type="monotone" dataKey="Solaire" stackId="prod" stroke="#f5a623" fill="#f5a623" />
          <Area type="monotone" dataKey="Éolien" stackId="prod" stroke="#4a90d9" fill="#4a90d9" />
          <Area type="monotone" dataKey="Thermique" stackId="prod" stroke="#9b59b6" fill="#9b59b6" />
          <Line type="monotone" dataKey="Charge" stroke="#e74c3c" strokeWidth={2} dot={false} />
          <Line type="monotone" dataKey="Import" stroke="#e67e22" strokeWidth={1} dot={false} />
          <Line type="monotone" dataKey="Export" stroke="#2ecc71" strokeWidth={1} dot={false} />
          <Line type="monotone" dataKey="P2P" stroke="#3aa6a6" strokeWidth={1} strokeDasharray="4 2" dot={false} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
