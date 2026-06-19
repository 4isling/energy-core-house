//! Un bâtiment du village : un foyer (et plus tard atelier, serre, salle
//! commune) qui abrite des habitants et des appareils. C'est l'**unité de
//! demande** du micro-réseau : chaque bâtiment agrège ses appareils — pilotés
//! par ses habitants (`resident.rs`) — et expose sa charge instantanée. La
//! demande du village est la somme des bâtiments.

use serde::{Deserialize, Serialize};

use crate::appliance::{Appliance, ApplianceKind};
use crate::resident::{Resident, ResidentProfile};

/// Type de bâtiment. v1 : des foyers aux profils d'occupation variés, qui
/// réutilisent les profils d'habitants existants. Conçu pour s'étendre plus tard
/// (atelier, serre, salle commune) sans changer le moteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildingKind {
    /// Petit logement : un actif, conso matin/soir, recharge VE la nuit.
    Studio,
    /// Foyer familial : un actif + un ado, conso forte et étalée.
    Family,
    /// Logement de retraités : conso étalée sur toute la journée.
    Elders,
}

impl BuildingKind {
    /// Libellé court pour l'UI.
    pub fn label(self) -> &'static str {
        match self {
            BuildingKind::Studio => "Studio",
            BuildingKind::Family => "Foyer familial",
            BuildingKind::Elders => "Logement séniors",
        }
    }

    /// Nombre maximal d'habitants que ce type de bâtiment peut accueillir.
    /// Au-delà du loadout de départ, des colons peuvent **emménager** tant qu'il
    /// reste de la place (cf. `SimState::population_step`).
    pub fn capacity(self) -> usize {
        match self {
            BuildingKind::Studio => 2,
            BuildingKind::Family => 5,
            // Couple de séniors : pas d'accueil de nouveaux arrivants.
            BuildingKind::Elders => 2,
        }
    }

    /// Appareils installés par défaut dans ce type de bâtiment.
    pub fn default_appliances(self) -> Vec<ApplianceKind> {
        use ApplianceKind::*;
        match self {
            BuildingKind::Studio => {
                vec![Fridge, Lighting, Heating, WaterHeater, Oven, EvCharger]
            }
            BuildingKind::Family => {
                vec![Fridge, Lighting, Heating, WaterHeater, WashingMachine, Oven, EvCharger]
            }
            BuildingKind::Elders => {
                vec![Fridge, Lighting, Heating, WaterHeater, WashingMachine, Oven]
            }
        }
    }

    /// Habitants présents par défaut (nom, profil de routine).
    pub fn default_residents(self) -> Vec<(&'static str, ResidentProfile)> {
        match self {
            BuildingKind::Studio => vec![("Alex", ResidentProfile::Worker)],
            BuildingKind::Family => {
                vec![("Sam", ResidentProfile::Worker), ("Lou", ResidentProfile::Teenager)]
            }
            BuildingKind::Elders => {
                vec![("René", ResidentProfile::Retiree), ("Odette", ResidentProfile::Retiree)]
            }
        }
    }
}

/// Un bâtiment du village.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    pub id: u32,
    pub kind: BuildingKind,
    pub name: String,
    /// Position sur la carte (tuile). `(0, 0)` tant que non placé sur la carte.
    pub x: u16,
    pub y: u16,
    /// Appareils consommateurs installés dans ce bâtiment.
    pub appliances: Vec<Appliance>,
    /// Habitants qui pilotent les appareils au fil de la journée.
    pub residents: Vec<Resident>,
    /// Charge additionnelle imposée manuellement (kW) — override/tests.
    pub load_kw: f64,
}

impl Building {
    /// Crée un bâtiment vide (sans loadout). Les appareils ajoutés ensuite
    /// reçoivent des ids alloués par l'appelant pour rester uniques dans le
    /// village.
    pub fn new(id: u32, kind: BuildingKind, name: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            name: name.into(),
            x: 0,
            y: 0,
            appliances: Vec::new(),
            residents: Vec::new(),
            load_kw: 0.0,
        }
    }

    /// Positionne le bâtiment sur une tuile de la carte.
    pub fn place(&mut self, x: u16, y: u16) {
        self.x = x;
        self.y = y;
    }

    /// Crée un bâtiment avec son loadout par défaut (appareils + habitants).
    /// Les ids d'appareils sont alloués à partir de `first_appliance_id` ;
    /// l'appelant doit ensuite avancer son compteur de `appliances.len()`.
    pub fn from_kind(id: u32, kind: BuildingKind, first_appliance_id: u32) -> Self {
        let mut b = Building::new(id, kind, kind.label());
        let mut next = first_appliance_id;
        for ak in kind.default_appliances() {
            b.appliances.push(Appliance::from_kind(next, ak));
            next += 1;
        }
        for (name, profile) in kind.default_residents() {
            b.residents.push(Resident::new(name, profile));
        }
        b
    }

    /// Charge appelée par les appareils allumés (kW).
    pub fn appliance_load_kw(&self) -> f64 {
        self.appliances.iter().map(|a| a.draw_kw()).sum()
    }

    /// Charge instantanée totale du bâtiment : appareils allumés + override.
    pub fn load_kw(&self) -> f64 {
        self.appliance_load_kw() + self.load_kw
    }

    /// Ajoute un appareil avec un id fourni (unique dans le village).
    pub fn add_appliance(&mut self, id: u32, kind: ApplianceKind) {
        self.appliances.push(Appliance::from_kind(id, kind));
    }

    /// Bascule l'état on/off d'un appareil. Renvoie false si l'id est inconnu.
    pub fn toggle_appliance(&mut self, id: u32) -> bool {
        if let Some(a) = self.appliances.iter_mut().find(|a| a.id == id) {
            a.on = !a.on;
            true
        } else {
            false
        }
    }

    /// Ajoute un habitant à ce bâtiment.
    pub fn add_resident(&mut self, name: impl Into<String>, profile: ResidentProfile) {
        self.residents.push(Resident::new(name, profile));
    }

    /// Applique la routine des habitants : un appareil est allumé si au moins un
    /// résident le souhaite à cette heure, ou s'il tourne en continu (frigo).
    pub fn apply_resident_schedule(&mut self, hour: f64, day: u32) {
        // Sans habitant, on laisse les appareils dans l'état réglé manuellement
        // (contrôle direct du joueur / tests).
        if self.residents.is_empty() {
            return;
        }
        let mut wanted: Vec<ApplianceKind> = Vec::new();
        for r in &self.residents {
            wanted.extend(r.desired_appliances(hour, day));
        }
        for a in &mut self.appliances {
            a.on = a.kind == ApplianceKind::Fridge || wanted.contains(&a.kind);
        }
    }

    /// Confort moyen des habitants (0..100). 100 s'il n'y a pas d'habitant.
    pub fn avg_comfort_pct(&self) -> f64 {
        if self.residents.is_empty() {
            100.0
        } else {
            self.residents.iter().map(|r| r.comfort).sum::<f64>() / self.residents.len() as f64
        }
    }
}

/// Vue d'un bâtiment pour une frame d'UI (incluse dans `TickReport`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildingReport {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub x: u16,
    pub y: u16,
    pub load_kw: f64,
    pub avg_comfort_pct: f64,
    pub resident_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_kind_installs_loadout() {
        let b = Building::from_kind(0, BuildingKind::Family, 0);
        assert!(!b.appliances.is_empty(), "le foyer a des appareils");
        assert_eq!(b.residents.len(), 2, "le foyer familial a deux habitants");
        // Les ids d'appareils sont consécutifs à partir de la base fournie.
        assert_eq!(b.appliances[0].id, 0);
        assert_eq!(b.appliances[1].id, 1);
    }

    #[test]
    fn empty_building_keeps_only_fridge_on() {
        let mut b = Building::from_kind(0, BuildingKind::Studio, 0);
        b.residents.clear();
        b.apply_resident_schedule(12.0, 1);
        for a in &b.appliances {
            if a.kind == ApplianceKind::Fridge {
                assert!(a.on, "le frigo reste allumé");
            } else {
                assert!(!a.on, "sans habitant, le reste est éteint");
            }
        }
    }

    #[test]
    fn toggle_unknown_id_is_false() {
        let mut b = Building::from_kind(0, BuildingKind::Studio, 0);
        assert!(!b.toggle_appliance(9999));
    }
}
