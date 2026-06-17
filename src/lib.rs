//! # energy-core
//!
//! Moteur de simulation énergétique **déterministe** et **portable** pour un
//! jeu de gestion. Toute la physique (éolien, solaire, hydro, thermique,
//! stockage), le dispatch et l'économie vivent ici, sans aucune dépendance au
//! rendu. Le même crate compile :
//!
//! - en **WASM** (`--features wasm`) pour un front web (React/Pixi),
//! - en **natif** (rlib) pour un moteur (Godot via gdext, Bevy) ou des tests.
//!
//! ## Exemple
//! ```
//! use energy_core::{SimState, WindTurbine, Weather};
//! let mut sim = SimState::new(5_000_000.0);
//! sim.build_wind(WindTurbine::onshore_2mw());
//! sim.set_load_kw(300.0);
//! let w = Weather { wind_ms: 11.0, irradiance_kw_m2: 0.0, air_temp_c: 12.0, river_flow_m3s: 2.0 };
//! let report = sim.tick(&w, 1.0);
//! assert!(report.wind_kw > 0.0);
//! ```

pub mod appliance;
pub mod building;
pub mod economy;
pub mod physics;
pub mod resident;
pub mod sim;
pub mod storage;
pub mod weather;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use appliance::{Appliance, ApplianceKind};
pub use building::{Building, BuildingKind, BuildingReport};
pub use economy::{Economy, Grid};
pub use physics::{
    wind_at_height, FuelKind, HydroKind, HydroTurbine, SolarArray, ThermalPlant, WindTurbine,
    AIR_DENSITY, BETZ_LIMIT, GRAVITY, WATER_DENSITY,
};
pub use resident::{Resident, ResidentProfile};
pub use sim::{Park, SimState, TickReport};
pub use storage::Battery;
pub use weather::{ProceduralWeather, Weather};
