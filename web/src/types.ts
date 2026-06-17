// Reflète `TickReport` (src/sim.rs) renvoyé par `Game.tick()`.
export interface TickReport {
  hour: number;
  day: number;
  wind_kw: number;
  solar_kw: number;
  hydro_kw: number;
  thermal_kw: number;
  battery_kw: number; // + décharge, - charge
  import_kw: number;
  export_kw: number;
  load_kw: number;
  unmet_kw: number;
  blackout: boolean;
  soc_pct: number;
  co2_kg_step: number;
  cash_flow_eur: number;
  budget_eur: number;
  co2_kg_total: number;
  avg_comfort_pct: number;
}

// Reflète `Appliance` (src/appliance.rs).
export interface ApplianceView {
  id: number;
  kind: string;
  name: string;
  power_kw: number;
  on: boolean;
}

// Reflète `Resident` (src/resident.rs).
export interface ResidentView {
  name: string;
  profile: string;
  comfort: number;
}

// Codes acceptés par `Game.add_appliance` (cf. parse_appliance_kind, src/wasm.rs).
export const APPLIANCE_CATALOG: { code: string; label: string; power_kw: number }[] = [
  { code: "fridge", label: "Réfrigérateur", power_kw: 0.15 },
  { code: "lighting", label: "Éclairage", power_kw: 0.3 },
  { code: "heating", label: "Chauffage", power_kw: 2.5 },
  { code: "water_heater", label: "Ballon d'eau chaude", power_kw: 2.0 },
  { code: "washing_machine", label: "Lave-linge", power_kw: 2.0 },
  { code: "oven", label: "Four / cuisson", power_kw: 2.5 },
  { code: "ev_charger", label: "Recharge VE", power_kw: 7.0 },
];

// Codes acceptés par `Game.add_resident` (cf. parse_profile, src/wasm.rs).
export const RESIDENT_PROFILES: { code: string; label: string }[] = [
  { code: "worker", label: "Actif" },
  { code: "retiree", label: "Retraité" },
  { code: "teenager", label: "Ado" },
];
