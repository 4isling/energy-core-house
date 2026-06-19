//! Façade WASM (compilée uniquement avec `--features wasm`).
//!
//! Expose une poignée de méthodes au JavaScript et renvoie l'état via
//! `serde-wasm-bindgen` (objets JS natifs, pas de parsing de chaîne).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::appliance::ApplianceKind;
use crate::building::BuildingKind;
use crate::grid::{Grid, NodeReport, Tier};
use crate::physics::{FuelKind, HydroKind, HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::resident::ResidentProfile;
use crate::sim::{BuildError, SimState, TickReport};
use crate::weather::{ProceduralWeather, Weather};

/// Mappe un résultat de construction spatiale vers un code numérique pour le JS :
/// 0 = OK, 1 = budget, 2 = hors carte, 3 = tuile occupée, 4 = terrain invalide.
fn build_code(r: Result<(), BuildError>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(BuildError::Budget) => 1,
        Err(BuildError::OutOfBounds) => 2,
        Err(BuildError::Occupied) => 3,
        Err(BuildError::BadTerrain) => 4,
    }
}

/// Un élément posé sur la carte, pour le rendu (`list_placements`).
#[derive(Serialize)]
struct PlacementView {
    kind: &'static str,
    x: u16,
    y: u16,
}

/// Une ligne électrique producteur → hub, pour le rendu (`power_lines`).
/// `(x1,y1)` = actif posé, `(x2,y2)` = hub de distribution (barycentre des foyers),
/// `loss_pct` = pertes en ligne (%) dues à la distance.
#[derive(Serialize)]
struct PowerLineView {
    x1: u16,
    y1: u16,
    x2: f64,
    y2: f64,
    kind: &'static str,
    loss_pct: f64,
}

/// Infos d'une tuile pour l'infobulle (`tile_info`).
#[derive(Serialize)]
struct TileInfoView {
    x: u16,
    y: u16,
    ground: &'static str,
    elevation: f32,
    wind_factor: f32,
    solar_factor: f32,
    water_factor: f32,
    buildable: bool,
    is_water: bool,
    occupied: bool,
}

/// Mappe un code texte (envoyé par le JS) vers une catégorie d'appareil.
fn parse_appliance_kind(code: &str) -> Option<ApplianceKind> {
    Some(match code {
        "fridge" => ApplianceKind::Fridge,
        "lighting" => ApplianceKind::Lighting,
        "heating" => ApplianceKind::Heating,
        "water_heater" => ApplianceKind::WaterHeater,
        "washing_machine" => ApplianceKind::WashingMachine,
        "oven" => ApplianceKind::Oven,
        "ev_charger" => ApplianceKind::EvCharger,
        _ => return None,
    })
}

/// Mappe un code texte vers un profil d'habitant.
fn parse_profile(code: &str) -> Option<ResidentProfile> {
    Some(match code {
        "worker" => ResidentProfile::Worker,
        "retiree" => ResidentProfile::Retiree,
        "teenager" => ResidentProfile::Teenager,
        _ => return None,
    })
}

/// Mappe un code texte vers un type de bâtiment.
fn parse_building_kind(code: &str) -> Option<BuildingKind> {
    Some(match code {
        "studio" => BuildingKind::Studio,
        "family" => BuildingKind::Family,
        "elders" => BuildingKind::Elders,
        _ => return None,
    })
}

#[wasm_bindgen]
pub struct Game {
    sim: SimState,
    weather: ProceduralWeather,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(starting_budget_eur: f64, seed: u32) -> Game {
        let mut sim = SimState::new(starting_budget_eur);
        // Carte de terrain procédurale (même seed que la météo → monde cohérent).
        sim.generate_map(seed as u64);
        // Village de départ : quelques foyers déjà placés sur la carte (gratuit).
        sim.place_starter_village();
        Game { sim, weather: ProceduralWeather::new(seed as u64) }
    }

    /// Avance d'un pas, génère la météo en interne, renvoie le `TickReport`.
    pub fn tick(&mut self, dt_h: f64) -> Result<JsValue, JsValue> {
        let w = self.weather.sample(self.sim.hour);
        let report: TickReport = self.sim.tick(&w, dt_h);
        serde_wasm_bindgen::to_value(&report).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Variante : la météo est fournie par le JS (depuis un CSV par ex.).
    pub fn tick_with_weather(
        &mut self,
        dt_h: f64,
        wind_ms: f64,
        irradiance_kw_m2: f64,
        air_temp_c: f64,
        river_flow_m3s: f64,
    ) -> Result<JsValue, JsValue> {
        let w = Weather { wind_ms, irradiance_kw_m2, air_temp_c, river_flow_m3s };
        let report = self.sim.tick(&w, dt_h);
        serde_wasm_bindgen::to_value(&report).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn set_load_kw(&mut self, kw: f64) {
        self.sim.set_load_kw(kw);
    }
    pub fn set_spot_price(&mut self, eur_per_kwh: f64) {
        self.sim.economy.grid.spot_price_eur_kwh = eur_per_kwh;
    }
    pub fn set_grid_connected(&mut self, connected: bool) {
        self.sim.economy.grid.connected = connected;
    }

    pub fn build_wind(&mut self) -> bool {
        self.sim.build_wind(WindTurbine::onshore_2mw())
    }
    /// Micro-éolienne domestique (~5 kW) — échelle maison.
    pub fn build_wind_micro(&mut self) -> bool {
        self.sim.build_wind(WindTurbine::micro())
    }
    pub fn build_solar(&mut self, kwc: f64) -> bool {
        self.sim.build_solar(SolarArray::new(kwc))
    }
    pub fn build_hydro(&mut self, design_flow_m3s: f64, head_m: f64) -> bool {
        self.sim.build_hydro(HydroTurbine::new(HydroKind::Kaplan, design_flow_m3s, head_m))
    }
    pub fn build_coal(&mut self, rated_kw: f64) -> bool {
        self.sim.build_thermal(ThermalPlant::new(FuelKind::Coal, rated_kw))
    }
    /// Groupe électrogène domestique (~6 kW) — secours pilotable.
    pub fn build_genset(&mut self) -> bool {
        self.sim.build_thermal(ThermalPlant::genset())
    }
    pub fn build_battery(&mut self, capacity_kwh: f64) -> bool {
        self.sim.build_battery(capacity_kwh)
    }

    // --- Bâtiments du village ---

    /// Construit un bâtiment (`code` : "studio", "family", "elders") en débitant
    /// le CAPEX. Renvoie l'id du bâtiment, ou -1 si le code est inconnu ou le
    /// budget insuffisant.
    pub fn build_building(&mut self, code: &str) -> i32 {
        match parse_building_kind(code) {
            Some(kind) => self.sim.build_building(kind).map(|id| id as i32).unwrap_or(-1),
            None => -1,
        }
    }
    /// Liste détaillée des bâtiments (objets JS : id, kind, name, appliances,
    /// residents, load_kw…).
    pub fn list_buildings(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.sim.buildings)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // --- Appareils consommateurs (scopés par bâtiment) ---

    /// Ajoute un appareil à un bâtiment (`code` : "fridge", "lighting",
    /// "heating", "water_heater", "washing_machine", "oven", "ev_charger").
    /// Renvoie l'id de l'appareil, ou -1 si code/bâtiment inconnu.
    pub fn add_appliance_to(&mut self, building_id: u32, code: &str) -> i32 {
        match parse_appliance_kind(code) {
            Some(kind) => self.sim.add_appliance_to(building_id, kind).map(|id| id as i32).unwrap_or(-1),
            None => -1,
        }
    }
    /// Bascule on/off d'un appareil par son id (unique dans le village).
    /// Renvoie false si l'id est inconnu.
    pub fn toggle_appliance(&mut self, id: u32) -> bool {
        self.sim.toggle_appliance(id)
    }

    // --- Habitants (NPC, scopés par bâtiment) ---

    /// Ajoute un habitant à un bâtiment (`profile` : "worker", "retiree",
    /// "teenager"). Renvoie false si le profil ou le bâtiment est inconnu.
    pub fn add_resident_to(&mut self, building_id: u32, name: &str, profile: &str) -> bool {
        match parse_profile(profile) {
            Some(p) => self.sim.add_resident_to(building_id, name, p),
            None => false,
        }
    }

    // --- Carte de terrain (lecture pour le rendu) ---

    pub fn map_width(&self) -> u32 {
        self.sim.map.width as u32
    }
    pub fn map_height(&self) -> u32 {
        self.sim.map.height as u32
    }

    /// Nature du sol par tuile (1 octet/tuile, row-major) : 0=Eau, 1=Plaine,
    /// 2=Forêt, 3=Colline, 4=Montagne. Lu une seule fois par le front.
    pub fn terrain_ground(&self) -> Vec<u8> {
        self.sim.map.tiles.iter().map(|t| t.ground as u8).collect()
    }
    /// Facteur de vent quantifié 0..255 (échelle 0..2.0).
    pub fn terrain_wind(&self) -> Vec<u8> {
        self.sim.map.tiles.iter()
            .map(|t| ((t.wind_factor / 2.0) * 255.0).clamp(0.0, 255.0) as u8)
            .collect()
    }
    /// Facteur de soleil quantifié 0..255 (échelle 0..1.0).
    pub fn terrain_solar(&self) -> Vec<u8> {
        self.sim.map.tiles.iter()
            .map(|t| (t.solar_factor * 255.0).clamp(0.0, 255.0) as u8)
            .collect()
    }
    /// Facteur de débit d'eau quantifié 0..255 (échelle 0..4.0 ; 0 = terre).
    pub fn terrain_water(&self) -> Vec<u8> {
        self.sim.map.tiles.iter()
            .map(|t| ((t.water_factor / 4.0) * 255.0).clamp(0.0, 255.0) as u8)
            .collect()
    }

    /// Infos d'une tuile (objet JS) pour l'infobulle de survol.
    pub fn tile_info(&self, x: u32, y: u32) -> Result<JsValue, JsValue> {
        let (x, y) = (x as u16, y as u16);
        let tile = self
            .sim
            .map
            .get(x, y)
            .ok_or_else(|| JsValue::from_str("tuile hors carte"))?;
        let view = TileInfoView {
            x,
            y,
            ground: tile.ground.label(),
            elevation: tile.elevation,
            wind_factor: tile.wind_factor,
            solar_factor: tile.solar_factor,
            water_factor: tile.water_factor,
            buildable: tile.buildable(),
            is_water: tile.is_water(),
            occupied: self.sim.is_occupied(x, y),
        };
        serde_wasm_bindgen::to_value(&view).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Liste des éléments posés sur la carte (objets JS : kind, x, y) pour le
    /// rendu des sprites. `kind` ∈ {wind, solar, hydro, genset, battery, building}.
    pub fn list_placements(&self) -> Result<JsValue, JsValue> {
        let mut v: Vec<PlacementView> = Vec::new();
        for p in &self.sim.park.wind {
            v.push(PlacementView { kind: "wind", x: p.x, y: p.y });
        }
        for p in &self.sim.park.solar {
            v.push(PlacementView { kind: "solar", x: p.x, y: p.y });
        }
        for p in &self.sim.park.hydro {
            v.push(PlacementView { kind: "hydro", x: p.x, y: p.y });
        }
        for p in &self.sim.park.thermal {
            v.push(PlacementView { kind: "genset", x: p.x, y: p.y });
        }
        for (x, y) in &self.sim.park.battery_tiles {
            v.push(PlacementView { kind: "battery", x: *x, y: *y });
        }
        for b in &self.sim.buildings {
            v.push(PlacementView { kind: "building", x: b.x, y: b.y });
        }
        serde_wasm_bindgen::to_value(&v).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Lignes électriques producteur → hub (objets JS : x1,y1,x2,y2,kind,loss_pct)
    /// pour dessiner le réseau et ses pertes. Vide tant qu'aucun foyer n'existe.
    pub fn power_lines(&self) -> Result<JsValue, JsValue> {
        let mut v: Vec<PowerLineView> = Vec::new();
        if let Some((hx, hy)) = self.sim.distribution_hub() {
            for p in &self.sim.park.wind {
                v.push(PowerLineView { x1: p.x, y1: p.y, x2: hx, y2: hy, kind: "wind", loss_pct: self.sim.line_loss_frac(p.x, p.y) * 100.0 });
            }
            for p in &self.sim.park.solar {
                v.push(PowerLineView { x1: p.x, y1: p.y, x2: hx, y2: hy, kind: "solar", loss_pct: self.sim.line_loss_frac(p.x, p.y) * 100.0 });
            }
            for p in &self.sim.park.hydro {
                v.push(PowerLineView { x1: p.x, y1: p.y, x2: hx, y2: hy, kind: "hydro", loss_pct: self.sim.line_loss_frac(p.x, p.y) * 100.0 });
            }
            for p in &self.sim.park.thermal {
                v.push(PowerLineView { x1: p.x, y1: p.y, x2: hx, y2: hy, kind: "genset", loss_pct: self.sim.line_loss_frac(p.x, p.y) * 100.0 });
            }
        }
        serde_wasm_bindgen::to_value(&v).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // --- Construction spatiale (clic sur une tuile). Renvoie un code build_code. ---

    pub fn build_solar_at(&mut self, x: u32, y: u32, kwc: f64) -> i32 {
        build_code(self.sim.build_solar_at(x as u16, y as u16, kwc))
    }
    pub fn build_wind_at(&mut self, x: u32, y: u32) -> i32 {
        build_code(self.sim.build_wind_at(x as u16, y as u16))
    }
    pub fn build_hydro_at(&mut self, x: u32, y: u32) -> i32 {
        build_code(self.sim.build_hydro_at(x as u16, y as u16))
    }
    pub fn build_genset_at(&mut self, x: u32, y: u32) -> i32 {
        build_code(self.sim.build_genset_at(x as u16, y as u16))
    }
    pub fn build_battery_at(&mut self, x: u32, y: u32, capacity_kwh: f64) -> i32 {
        build_code(self.sim.build_battery_at(x as u16, y as u16, capacity_kwh))
    }
    /// Construit un bâtiment (`code`) sur une tuile. Renvoie l'id (>=0) ou un code
    /// d'erreur négatif : -1 budget, -2 hors carte, -3 occupée, -4 terrain, -5 code.
    pub fn build_building_at(&mut self, x: u32, y: u32, code: &str) -> i32 {
        match parse_building_kind(code) {
            Some(kind) => match self.sim.build_building_at(x as u16, y as u16, kind) {
                Ok(id) => id as i32,
                Err(e) => -build_code(Err(e)),
            },
            None => -5,
        }
    }

    pub fn budget_eur(&self) -> f64 {
        self.sim.economy.budget_eur
    }
    pub fn co2_kg(&self) -> f64 {
        self.sim.economy.co2_kg
    }
}

// ===========================================================================
// Façade WASM du réseau multi-couches (national / quartier / maison).
// Indépendante de `Game` (mono-carte), qui reste inchangé.
// ===========================================================================

/// Amplitude du **foisonnement** : écart météo relatif décorrélé par nœud.
const FOISONNEMENT_AMPLITUDE: f64 = 0.25;

/// Vue d'un nœud du réseau pour le front (arbre + inspecteur). Synthétise le
/// `GridNode` (dont le `Park` complet n'a pas besoin de remonter au JS).
#[derive(Serialize)]
struct GridNodeView {
    id: u32,
    tier: &'static str,
    name: String,
    parent: Option<u32>,
    children: Vec<u32>,
    load_kw: f64,
    autonomy_pref: f64,
    income_eur_per_day: f64,
    balance_eur: f64,
    fixed_cost_eur_per_day: f64,
    islanded: bool,
    /// Composition de l'auto-production (pour l'inspecteur de maison/quartier).
    solar_kwc: f64,
    battery_kwh: f64,
    wind_count: u32,
    thermal_count: u32,
    /// Tarif de la connexion au parent (None pour la racine).
    import_price_eur_kwh: f64,
    export_price_eur_kwh: f64,
    link_capacity_kw: f64,
    has_uplink: bool,
}

fn tier_label(t: Tier) -> &'static str {
    t.label()
}

/// Construit la vue front d'un nœud du réseau.
fn node_view(n: &crate::grid::GridNode) -> GridNodeView {
    let (import_price, export_price, cap, has_uplink) = match &n.uplink {
        Some(l) => (l.import_price_eur_kwh, l.export_price_eur_kwh, l.capacity_kw, true),
        None => (0.0, 0.0, 0.0, false),
    };
    GridNodeView {
        id: n.id,
        tier: tier_label(n.tier),
        name: n.name.clone(),
        parent: n.parent,
        children: n.children.clone(),
        load_kw: n.load_kw,
        autonomy_pref: n.autonomy_pref,
        income_eur_per_day: n.income_eur_per_day,
        balance_eur: n.wallet.balance_eur,
        fixed_cost_eur_per_day: n.wallet.fixed_cost_eur_per_day,
        islanded: n.islanded(),
        solar_kwc: n.park.solar.iter().map(|p| p.asset.kwc).sum(),
        battery_kwh: n.park.battery.as_ref().map(|b| b.capacity_kwh).unwrap_or(0.0),
        wind_count: n.park.wind.len() as u32,
        thermal_count: n.park.thermal.len() as u32,
        import_price_eur_kwh: import_price,
        export_price_eur_kwh: export_price,
        link_capacity_kw: cap,
        has_uplink,
    }
}

#[wasm_bindgen]
pub struct GridGame {
    grid: Grid,
    weather: ProceduralWeather,
    /// Derniers rapports (un par nœud) du dernier `tick`, pour `summary`/UI.
    last_reports: Vec<NodeReport>,
}

#[wasm_bindgen]
impl GridGame {
    /// Crée un réseau de départ : 1 national → `n_districts` quartiers →
    /// `houses_per_district` maisons. Déterministe (seedé).
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, n_districts: u32, houses_per_district: u32) -> GridGame {
        let grid = Grid::scenario(seed as u64, n_districts as usize, houses_per_district as usize);
        GridGame {
            grid,
            weather: ProceduralWeather::new(seed as u64),
            last_reports: Vec::new(),
        }
    }

    /// Avance tout l'arbre d'un pas : génère la météo de base, l'éclate par nœud
    /// (foisonnement), équilibre en deux passes et renvoie la liste des
    /// `NodeReport` (objets JS natifs).
    pub fn tick(&mut self, dt_h: f64) -> Result<JsValue, JsValue> {
        let base = self.weather.sample(self.grid.sim_hours);
        self.grid.propagate_weather(base, FOISONNEMENT_AMPLITUDE);
        let reports = self.grid.tick(dt_h);
        let js = serde_wasm_bindgen::to_value(&reports).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.last_reports = reports;
        Ok(js)
    }

    /// Indicateurs de spirale (marge nationale, taux de dépendance…) calculés sur
    /// le dernier pas. Renvoie `null` tant qu'aucun `tick` n'a eu lieu.
    pub fn summary(&self) -> Result<JsValue, JsValue> {
        if self.last_reports.is_empty() {
            return Ok(JsValue::NULL);
        }
        let s = self.grid.summary(&self.last_reports);
        serde_wasm_bindgen::to_value(&s).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn root(&self) -> u32 {
        self.grid.root
    }
    pub fn node_count(&self) -> u32 {
        self.grid.nodes.len() as u32
    }

    /// Vue synthétique d'un nœud (objet JS) pour l'arbre et l'inspecteur.
    pub fn node(&self, id: u32) -> Result<JsValue, JsValue> {
        let n = self
            .grid
            .nodes
            .get(id as usize)
            .ok_or_else(|| JsValue::from_str("nœud inconnu"))?;
        serde_wasm_bindgen::to_value(&node_view(n)).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Liste des vues de **tous** les nœuds (pour construire l'arbre côté front).
    pub fn nodes(&self) -> Result<JsValue, JsValue> {
        let views: Vec<GridNodeView> = self.grid.nodes.iter().map(node_view).collect();
        serde_wasm_bindgen::to_value(&views).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Ids des enfants directs d'un nœud (drill-down).
    pub fn children(&self, id: u32) -> Vec<u32> {
        self.grid
            .nodes
            .get(id as usize)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    // --- Actions joueur ---

    /// Règle le tarif national (prix import/export des liens des quartiers). C'est
    /// le levier de la spirale : l'augmenter rend l'autoproduction plus rentable.
    pub fn set_national_tariff(&mut self, import_price_eur_kwh: f64, export_price_eur_kwh: f64) {
        self.grid.set_national_tariff(import_price_eur_kwh, export_price_eur_kwh);
    }

    /// Îlote (ou reconnecte) un nœud : coupe son lien au parent pour tester la
    /// résilience d'un quartier face à une panne du national.
    pub fn island_node(&mut self, id: u32, islanded: bool) {
        self.grid.set_islanded(id, islanded);
    }

    /// Construit du solaire (kWc) sur un nœud (national/quartier), débité de son
    /// portefeuille. Renvoie `false` si le solde est insuffisant.
    pub fn build_solar_on(&mut self, id: u32, kwc: f64) -> bool {
        self.grid.build_solar(id, kwc)
    }
    /// Construit une micro-éolienne sur un nœud (débit du portefeuille du nœud).
    pub fn build_wind_on(&mut self, id: u32) -> bool {
        self.grid.build_wind_micro(id)
    }
    /// Ajoute de la batterie (kWh) sur un nœud (débit du portefeuille du nœud).
    pub fn build_battery_on(&mut self, id: u32, capacity_kwh: f64) -> bool {
        self.grid.build_battery(id, capacity_kwh)
    }
}
