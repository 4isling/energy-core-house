//! Couche économique : CAPEX/OPEX par filière (ordres de grandeur France,
//! sources CRE PPE2 / Fraunhofer / IRENA), émissions, et raccordement réseau
//! (import/export au prix spot). Volontairement séparée de la physique.

use serde::{Deserialize, Serialize};

use crate::building::BuildingKind;
use crate::physics::{FuelKind, HydroTurbine, SolarArray, ThermalPlant, WindTurbine};

/// CAPEX d'une éolienne (€), ~1850 €/kW installé.
pub fn capex_wind(t: &WindTurbine) -> f64 {
    t.rated_kw * 1850.0
}
/// CAPEX d'un champ solaire (€), ~1100 €/kWc.
pub fn capex_solar(s: &SolarArray) -> f64 {
    s.kwc * 1100.0
}
/// CAPEX d'une turbine hydro (€), ~4000 €/kW de puissance nominale.
pub fn capex_hydro(h: &HydroTurbine) -> f64 {
    h.power_kw(h.design_flow_m3s) * 4000.0
}
/// CAPEX d'une centrale thermique (€), ~900 €/kW.
pub fn capex_thermal(t: &ThermalPlant) -> f64 {
    t.rated_kw * 900.0
}
/// CAPEX batterie (€), ~600 €/kWh.
pub fn capex_battery_per_kwh() -> f64 {
    600.0
}

/// CAPEX d'un bâtiment du village (€) : raccordement au micro-réseau +
/// équipement du foyer. Ordre de grandeur d'un logement neuf raccordé, modulé
/// par la taille du foyer.
pub fn capex_building(kind: BuildingKind) -> f64 {
    match kind {
        BuildingKind::Studio => 8_000.0,
        BuildingKind::Family => 14_000.0,
        BuildingKind::Elders => 11_000.0,
    }
}

/// OPEX annuel d'une éolienne (€/an).
pub fn opex_wind_year(t: &WindTurbine) -> f64 {
    t.rated_kw * 45.0
}
pub fn opex_solar_year(s: &SolarArray) -> f64 {
    s.kwc * 22.0
}
pub fn opex_hydro_year(h: &HydroTurbine) -> f64 {
    h.power_kw(h.design_flow_m3s) * 60.0
}
pub fn opex_thermal_year(t: &ThermalPlant) -> f64 {
    t.rated_kw * 40.0
}

/// Raccordement au réseau (RTE/Enedis). Le prix spot peut être alimenté
/// depuis un historique éCO2mix / ENTSO-E.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub connected: bool,
    /// Prix de gros (€/kWh). 0.097 €/kWh = prix spot moyen France 2023.
    pub spot_price_eur_kwh: f64,
    /// Part du prix spot reversée à l'export (vente du surplus).
    pub export_factor: f64,
}

impl Default for Grid {
    fn default() -> Self {
        Self { connected: true, spot_price_eur_kwh: 0.097, export_factor: 0.85 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Economy {
    pub budget_eur: f64,
    pub co2_kg: f64,
    pub grid: Grid,
}

impl Economy {
    pub fn new(starting_budget_eur: f64) -> Self {
        Self { budget_eur: starting_budget_eur, co2_kg: 0.0, grid: Grid::default() }
    }

    pub fn import_cost(&self, kwh: f64) -> f64 {
        kwh * self.grid.spot_price_eur_kwh
    }
    pub fn export_revenue(&self, kwh: f64) -> f64 {
        kwh * self.grid.spot_price_eur_kwh * self.grid.export_factor
    }

    /// Émissions du réseau (gCO2/kWh) — défaut mix France ~32 g.
    pub fn grid_co2_g_per_kwh(&self) -> f64 {
        32.0
    }

    pub fn co2_of_fuel(kind: FuelKind, kwh: f64) -> f64 {
        let g = match kind {
            FuelKind::Coal => 941.0,
            FuelKind::GasCcgt => 389.0,
            FuelKind::GasTac => 583.0,
            FuelKind::Oil => 928.0,
        };
        kwh * g / 1000.0 // kg
    }
}
