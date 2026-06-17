//! Façade WASM (compilée uniquement avec `--features wasm`).
//!
//! Expose une poignée de méthodes au JavaScript et renvoie l'état via
//! `serde-wasm-bindgen` (objets JS natifs, pas de parsing de chaîne).

use wasm_bindgen::prelude::*;

use crate::appliance::ApplianceKind;
use crate::physics::{FuelKind, HydroKind, HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::resident::ResidentProfile;
use crate::sim::{SimState, TickReport};
use crate::weather::{ProceduralWeather, Weather};

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

#[wasm_bindgen]
pub struct Game {
    sim: SimState,
    weather: ProceduralWeather,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(starting_budget_eur: f64, seed: u32) -> Game {
        Game {
            sim: SimState::new(starting_budget_eur),
            weather: ProceduralWeather::new(seed as u64),
        }
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

    // --- Appareils consommateurs ---

    /// Ajoute un appareil (`code` : "fridge", "lighting", "heating",
    /// "water_heater", "washing_machine", "oven", "ev_charger").
    /// Renvoie l'id de l'appareil, ou -1 si le code est inconnu.
    pub fn add_appliance(&mut self, code: &str) -> i32 {
        match parse_appliance_kind(code) {
            Some(kind) => self.sim.add_appliance(kind) as i32,
            None => -1,
        }
    }
    /// Bascule on/off d'un appareil. Renvoie false si l'id est inconnu.
    pub fn toggle_appliance(&mut self, id: u32) -> bool {
        self.sim.toggle_appliance(id)
    }
    /// Liste des appareils (objets JS : id, kind, name, power_kw, on).
    pub fn list_appliances(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.sim.park.appliances)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // --- Habitants (NPC) ---

    /// Ajoute un habitant (`profile` : "worker", "retiree", "teenager").
    /// Renvoie false si le profil est inconnu.
    pub fn add_resident(&mut self, name: &str, profile: &str) -> bool {
        match parse_profile(profile) {
            Some(p) => {
                self.sim.add_resident(name, p);
                true
            }
            None => false,
        }
    }
    /// Liste des habitants (objets JS : name, profile, comfort).
    pub fn list_residents(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.sim.park.residents)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn budget_eur(&self) -> f64 {
        self.sim.economy.budget_eur
    }
    pub fn co2_kg(&self) -> f64 {
        self.sim.economy.co2_kg
    }
}
