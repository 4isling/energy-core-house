//! Façade WASM (compilée uniquement avec `--features wasm`).
//!
//! Expose une poignée de méthodes au JavaScript et renvoie l'état via
//! `serde-wasm-bindgen` (objets JS natifs, pas de parsing de chaîne).

use wasm_bindgen::prelude::*;

use crate::physics::{FuelKind, HydroKind, HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::sim::{SimState, TickReport};
use crate::weather::{ProceduralWeather, Weather};

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
    pub fn build_solar(&mut self, kwc: f64) -> bool {
        self.sim.build_solar(SolarArray::new(kwc))
    }
    pub fn build_hydro(&mut self, design_flow_m3s: f64, head_m: f64) -> bool {
        self.sim.build_hydro(HydroTurbine::new(HydroKind::Kaplan, design_flow_m3s, head_m))
    }
    pub fn build_coal(&mut self, rated_kw: f64) -> bool {
        self.sim.build_thermal(ThermalPlant::new(FuelKind::Coal, rated_kw))
    }
    pub fn build_battery(&mut self, capacity_kwh: f64) -> bool {
        self.sim.build_battery(capacity_kwh)
    }

    pub fn budget_eur(&self) -> f64 {
        self.sim.economy.budget_eur
    }
    pub fn co2_kg(&self) -> f64 {
        self.sim.economy.co2_kg
    }
}
