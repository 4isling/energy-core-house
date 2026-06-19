//! Façade WASM (compilée uniquement avec `--features wasm`).
//!
//! Expose une poignée de méthodes au JavaScript et renvoie l'état via
//! `serde-wasm-bindgen` (objets JS natifs, pas de parsing de chaîne).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::appliance::ApplianceKind;
use crate::building::BuildingKind;
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
