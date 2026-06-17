//! Appareils consommateurs de la maison. C'est le cœur du gameplay côté
//! demande : le joueur ajoute des appareils, et les habitants (`resident.rs`)
//! les allument/éteignent au fil de la journée. La charge instantanée du
//! `Park` est la somme des appareils allumés.

use serde::{Deserialize, Serialize};

/// Catégorie d'appareil domestique. Les puissances sont des ordres de grandeur
/// réalistes (cf. `data_energie_research.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplianceKind {
    Fridge,         // frigo/congélateur (cyclage moyen)
    Lighting,       // éclairage de la maison
    Heating,        // chauffage électrique (convecteurs / PAC d'appoint)
    WaterHeater,    // ballon d'eau chaude
    WashingMachine, // lave-linge
    Oven,           // four / plaques de cuisson
    EvCharger,      // recharge véhicule électrique
}

impl ApplianceKind {
    /// Puissance moyenne en fonctionnement (kW).
    pub fn default_power_kw(self) -> f64 {
        match self {
            ApplianceKind::Fridge => 0.15,
            ApplianceKind::Lighting => 0.3,
            ApplianceKind::Heating => 2.5,
            ApplianceKind::WaterHeater => 2.0,
            ApplianceKind::WashingMachine => 2.0,
            ApplianceKind::Oven => 2.5,
            ApplianceKind::EvCharger => 7.0,
        }
    }

    /// Libellé court pour l'UI.
    pub fn label(self) -> &'static str {
        match self {
            ApplianceKind::Fridge => "Réfrigérateur",
            ApplianceKind::Lighting => "Éclairage",
            ApplianceKind::Heating => "Chauffage",
            ApplianceKind::WaterHeater => "Ballon d'eau chaude",
            ApplianceKind::WashingMachine => "Lave-linge",
            ApplianceKind::Oven => "Four / cuisson",
            ApplianceKind::EvCharger => "Recharge VE",
        }
    }
}

/// Un appareil installé dans la maison.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Appliance {
    pub id: u32,
    pub kind: ApplianceKind,
    pub name: String,
    pub power_kw: f64,
    pub on: bool,
}

impl Appliance {
    /// Crée un appareil à partir d'un preset de catégorie.
    pub fn from_kind(id: u32, kind: ApplianceKind) -> Self {
        Self {
            id,
            kind,
            name: kind.label().to_string(),
            power_kw: kind.default_power_kw(),
            // Le frigo tourne en continu ; le reste démarre éteint et sera
            // piloté par les habitants.
            on: matches!(kind, ApplianceKind::Fridge),
        }
    }

    /// Puissance instantanée appelée (kW) : `power_kw` si allumé, sinon 0.
    pub fn draw_kw(&self) -> f64 {
        if self.on {
            self.power_kw
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_appliance_draws_nothing() {
        let mut a = Appliance::from_kind(1, ApplianceKind::Oven);
        assert_eq!(a.draw_kw(), 0.0);
        a.on = true;
        assert!((a.draw_kw() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn fridge_runs_by_default() {
        let f = Appliance::from_kind(0, ApplianceKind::Fridge);
        assert!(f.on);
        assert!(f.draw_kw() > 0.0);
    }
}
