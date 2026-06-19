// Types du réseau multi-couches (national / quartier / maison). Reflètent les
// structures Rust renvoyées par `GridGame` (src/wasm.rs + src/grid.rs).

// Reflète `NodeReport` (src/grid.rs) — un par nœud, renvoyé par `tick()`.
export interface NodeReport {
  id: number;
  tier_label: string; // "National" | "Quartier" | "Maison"
  name: string;
  wind_kw: number;
  solar_kw: number;
  hydro_kw: number;
  thermal_kw: number;
  battery_kw: number; // + décharge, - charge
  load_kw: number;
  import_kw: number; // reçu du parent
  export_kw: number; // livré au parent
  p2p_kw: number; // troqué avec les voisins
  unmet_kw: number; // déficit non fourni
  blackout: boolean;
  curtailed_kw: number; // surplus écrêté
  soc_pct: number;
  co2_kg_step: number;
  cash_flow_eur: number;
  balance_eur: number;
  islanded: boolean;
}

// Reflète `GridSummary` (src/grid.rs) — indicateurs de spirale d'un pas.
export interface GridSummary {
  national_margin_eur: number;
  national_balance_eur: number;
  dependency_rate: number; // 0..1
  total_load_kw: number;
  total_import_kw: number;
  households: number;
  self_producing_households: number;
}

// Reflète `GridNodeView` (src/wasm.rs) — vue synthétique d'un nœud.
export interface GridNodeView {
  id: number;
  tier: string; // "National" | "Quartier" | "Maison"
  name: string;
  parent: number | null;
  children: number[];
  load_kw: number;
  autonomy_pref: number;
  income_eur_per_day: number;
  balance_eur: number;
  fixed_cost_eur_per_day: number;
  islanded: boolean;
  solar_kwc: number;
  battery_kwh: number;
  wind_count: number;
  thermal_count: number;
  import_price_eur_kwh: number;
  export_price_eur_kwh: number;
  link_capacity_kw: number;
  has_uplink: boolean;
}

export const TIER_EMOJI: Record<string, string> = {
  National: "🏛️",
  Quartier: "🏘️",
  Maison: "🏠",
};
