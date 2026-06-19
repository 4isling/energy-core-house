// Détail par bâtiment dans une frame — reflète `BuildingReport` (src/building.rs).
export interface BuildingReport {
  id: number;
  name: string;
  kind: string;
  x: number;
  y: number;
  load_kw: number;
  avg_comfort_pct: number;
  resident_count: number;
}

// Un élément posé sur la carte — reflète `PlacementView` (src/wasm.rs).
export interface PlacementView {
  kind: "wind" | "solar" | "hydro" | "genset" | "battery" | "building";
  x: number;
  y: number;
}

// Infos d'une tuile — reflète `TileInfoView` (src/wasm.rs).
export interface TileInfo {
  x: number;
  y: number;
  ground: string;
  elevation: number;
  wind_factor: number;
  solar_factor: number;
  water_factor: number;
  buildable: boolean;
  is_water: boolean;
  occupied: boolean;
}

// Données de terrain chargées une seule fois pour le rendu.
export interface TerrainData {
  width: number;
  height: number;
  ground: Uint8Array; // 0=Eau 1=Plaine 2=Forêt 3=Colline 4=Montagne
  wind: Uint8Array; // facteur vent (0..255 = 0..2.0)
  solar: Uint8Array; // facteur soleil (0..255 = 0..1.0)
  water: Uint8Array; // facteur débit (0..255 = 0..4.0)
}

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
  population: number;
  buildings: BuildingReport[];
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

// Détail complet d'un bâtiment — reflète `Building` (src/building.rs),
// renvoyé par `Game.list_buildings()`.
export interface BuildingView {
  id: number;
  kind: string; // "Studio" | "Family" | "Elders" (variante serde)
  name: string;
  x: number;
  y: number;
  appliances: ApplianceView[];
  residents: ResidentView[];
  load_kw: number;
}

// Codes acceptés par `Game.add_appliance_to` (cf. parse_appliance_kind, src/wasm.rs).
export const APPLIANCE_CATALOG: { code: string; label: string; power_kw: number }[] = [
  { code: "fridge", label: "Réfrigérateur", power_kw: 0.15 },
  { code: "lighting", label: "Éclairage", power_kw: 0.3 },
  { code: "heating", label: "Chauffage", power_kw: 2.5 },
  { code: "water_heater", label: "Ballon d'eau chaude", power_kw: 2.0 },
  { code: "washing_machine", label: "Lave-linge", power_kw: 2.0 },
  { code: "oven", label: "Four / cuisson", power_kw: 2.5 },
  { code: "ev_charger", label: "Recharge VE", power_kw: 7.0 },
];

// Codes acceptés par `Game.add_resident_to` (cf. parse_profile, src/wasm.rs).
export const RESIDENT_PROFILES: { code: string; label: string }[] = [
  { code: "worker", label: "Actif" },
  { code: "retiree", label: "Retraité" },
  { code: "teenager", label: "Ado" },
];

// Codes acceptés par `Game.build_building` (cf. parse_building_kind, src/wasm.rs).
// Le coût reflète `capex_building` (src/economy.rs).
export const BUILDING_CATALOG: {
  code: string;
  label: string;
  emoji: string;
  cost: number;
  detail: string;
}[] = [
  { code: "studio", label: "Studio", emoji: "🏠", cost: 8_000, detail: "1 actif" },
  { code: "family", label: "Foyer familial", emoji: "🏡", cost: 14_000, detail: "actif + ado" },
  { code: "elders", label: "Logement séniors", emoji: "🏘️", cost: 11_000, detail: "2 retraités" },
];

// Outils plaçables sur la carte (palette de construction). `icon` est un fichier
// sprite dans `web/public/sprites/` (placeholders pixel-art) ; `emoji` est le
// repli si le sprite est absent. Les coûts reflètent les CAPEX du cœur Rust.
export interface BuildTool {
  id: string;
  label: string;
  icon: string; // nom de fichier sprite
  emoji: string;
  cost: number;
  terrain: "land" | "water";
  category: "energy" | "building";
  detail: string;
  buildingCode?: string; // si category === "building"
}

export const BUILD_TOOLS: BuildTool[] = [
  { id: "solar", label: "Panneau solaire", icon: "solar.png", emoji: "☀️", cost: 6_600, terrain: "land", category: "energy", detail: "6 kWc — mieux au soleil" },
  { id: "wind", label: "Micro-éolienne", icon: "wind.png", emoji: "🌬️", cost: 9_250, terrain: "land", category: "energy", detail: "~5 kW — mieux sur les crêtes" },
  { id: "hydro", label: "Micro-hydro", icon: "hydro.png", emoji: "💧", cost: 108_000, terrain: "water", category: "energy", detail: "~27 kW — sur rivière" },
  { id: "genset", label: "Groupe électrogène", icon: "genset.png", emoji: "🛢️", cost: 5_400, terrain: "land", category: "energy", detail: "~6 kW — secours fossile" },
  { id: "battery", label: "Batterie", icon: "battery.png", emoji: "🔋", cost: 6_000, terrain: "land", category: "energy", detail: "10 kWh — stockage" },
  { id: "studio", label: "Studio", icon: "house.png", emoji: "🏠", cost: 8_000, terrain: "land", category: "building", detail: "1 actif", buildingCode: "studio" },
  { id: "family", label: "Foyer familial", icon: "house.png", emoji: "🏡", cost: 14_000, terrain: "land", category: "building", detail: "actif + ado", buildingCode: "family" },
  { id: "elders", label: "Logement séniors", icon: "house.png", emoji: "🏘️", cost: 11_000, terrain: "land", category: "building", detail: "2 retraités", buildingCode: "elders" },
];
