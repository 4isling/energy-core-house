import type { TickReport } from "../types";

function fmtHour(hour: number): string {
  const h = Math.floor(hour);
  const m = Math.round((hour - h) * 60);
  return `${String(h).padStart(2, "0")}h${String(m).padStart(2, "0")}`;
}

function fmtEur(v: number): string {
  return v.toLocaleString("fr-FR", { maximumFractionDigits: 0 }) + " €";
}

export function Dashboard({ report }: { report: TickReport }) {
  const net = report.import_kw - report.export_kw;
  return (
    <div className="dashboard">
      <Stat label="Jour" value={`J${report.day} · ${fmtHour(report.hour)}`} />
      <Stat
        label="Budget"
        value={fmtEur(report.budget_eur)}
        warn={report.budget_eur < 0}
      />
      <Stat label="Revenu" value={`+${fmtEur(report.revenue_eur_day)}/j`} />
      <Stat label="Population" value={`👤 ${report.population}`} />
      {report.waiting > 0 && (
        <Stat label="En attente" value={`⏳ ${report.waiting}`} />
      )}
      <Stat label="Foyers" value={`${report.buildings.length}`} />
      <Stat label="CO₂ cumulé" value={`${report.co2_kg_total.toFixed(1)} kg`} />
      <Gauge label="Batterie (SoC)" pct={report.soc_pct} />
      <Gauge label="Confort du village" pct={report.avg_comfort_pct} />
      <Stat label="Demande" value={`${report.load_kw.toFixed(2)} kW`} />
      <Stat
        label="Pertes lignes"
        value={`🔌 ${report.loss_kw.toFixed(2)} kW`}
        warn={report.loss_kw > 0.2 * report.load_kw && report.loss_kw > 0.1}
      />
      <Stat
        label={net >= 0 ? "Import réseau" : "Export réseau"}
        value={`${Math.abs(net).toFixed(2)} kW`}
      />
      {report.blackout ? (
        <div className="stat blackout-alert">⚠️ BLACK-OUT</div>
      ) : (
        <div className="stat ok-badge">✓ Alimenté</div>
      )}
      {report.overcrowded && (
        <div className="stat blackout-alert">🏠 SURPOPULATION</div>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  warn,
}: {
  label: string;
  value: string;
  warn?: boolean;
}) {
  return (
    <div className={`stat${warn ? " warn" : ""}`}>
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
    </div>
  );
}

function Gauge({
  label,
  pct,
  invertColor,
}: {
  label: string;
  pct: number;
  invertColor?: boolean;
}) {
  const clamped = Math.max(0, Math.min(100, pct));
  // Confort : vert quand haut. SoC : vert quand haut aussi.
  const hue = invertColor ? clamped * 1.2 : clamped * 1.2;
  return (
    <div className="stat gauge">
      <span className="stat-label">{label}</span>
      <div className="gauge-track">
        <div
          className="gauge-fill"
          style={{
            width: `${clamped}%`,
            background: `hsl(${hue}, 70%, 45%)`,
          }}
        />
      </div>
      <span className="stat-value">{clamped.toFixed(0)} %</span>
    </div>
  );
}
