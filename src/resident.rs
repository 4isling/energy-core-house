//! Habitants (NPC) de la maison. Ce sont eux qui rendent la consommation
//! vivante : chaque résident suit une routine journalière (sommeil, douche,
//! cuisine, absence, soirée, recharge VE…) qui allume/éteint les appareils
//! (`appliance.rs`). Les routines sont **déterministes** (fonction de l'heure
//! et du jour) : à scénario identique, la courbe de charge est reproductible.

use serde::{Deserialize, Serialize};

use crate::appliance::ApplianceKind;

/// Profil de vie d'un habitant : il détermine la routine journalière.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResidentProfile {
    /// Actif : part travailler la journée, consomme matin et soir.
    Worker,
    /// Retraité : présent toute la journée, conso étalée.
    Retiree,
    /// Ado : couche-tard, conso nocturne et en soirée.
    Teenager,
}

impl ResidentProfile {
    pub fn label(self) -> &'static str {
        match self {
            ResidentProfile::Worker => "Actif",
            ResidentProfile::Retiree => "Retraité",
            ResidentProfile::Teenager => "Ado",
        }
    }

    /// Revenu brut apporté par cet habitant (€/jour) : salaire pour un actif,
    /// pension pour un retraité, rien pour un ado. C'est la principale source de
    /// rentrées d'argent du village (cf. `SimState::tick`). Le revenu effectif
    /// est ensuite pondéré par le **confort** de l'habitant : un colon mal
    /// alimenté travaille/dépense moins.
    pub fn income_eur_per_day(self) -> f64 {
        match self {
            ResidentProfile::Worker => 90.0,
            ResidentProfile::Retiree => 40.0,
            ResidentProfile::Teenager => 0.0,
        }
    }
}

/// Un habitant de la maison.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resident {
    pub name: String,
    pub profile: ResidentProfile,
    /// Niveau de confort (0..100) : baisse pendant un black-out si l'habitant
    /// est éveillé, remonte doucement quand tout est alimenté.
    pub comfort: f64,
}

impl Resident {
    pub fn new(name: impl Into<String>, profile: ResidentProfile) -> Self {
        Self { name: name.into(), profile, comfort: 100.0 }
    }

    /// L'habitant est-il éveillé à cette heure ?
    pub fn awake(&self, hour: f64) -> bool {
        let h = hour.rem_euclid(24.0);
        match self.profile {
            ResidentProfile::Worker => (6.0..23.0).contains(&h),
            ResidentProfile::Retiree => (7.0..22.0).contains(&h),
            // L'ado dort tard le matin et veille la nuit.
            ResidentProfile::Teenager => !(2.0..10.0).contains(&h),
        }
    }

    /// Appareils que cet habitant souhaite voir allumés à `hour` le jour `day`.
    /// Le `day` introduit une variation déterministe (ex. lave-linge un jour
    /// sur deux/trois) sans casser la reproductibilité.
    pub fn desired_appliances(&self, hour: f64, day: u32) -> Vec<ApplianceKind> {
        use ApplianceKind::*;
        let h = hour.rem_euclid(24.0);
        let mut v = Vec::new();
        match self.profile {
            ResidentProfile::Worker => {
                if (0.0..6.0).contains(&h) {
                    v.push(EvCharger); // recharge la nuit
                } else if (6.0..8.0).contains(&h) {
                    v.extend([Lighting, WaterHeater, Oven]); // petit-déj
                } else if (18.0..23.0).contains(&h) {
                    v.extend([Lighting, Heating, Oven]);
                    if day % 3 == 0 {
                        v.push(WashingMachine);
                    }
                }
                // 8h–18h : absent → rien (hors appareils toujours actifs).
            }
            ResidentProfile::Retiree => {
                if (7.0..9.0).contains(&h) {
                    v.extend([Lighting, WaterHeater, Oven]);
                } else if (9.0..12.0).contains(&h) {
                    v.extend([Lighting, Heating]);
                } else if (12.0..14.0).contains(&h) {
                    v.extend([Lighting, Oven]);
                } else if (14.0..19.0).contains(&h) {
                    v.extend([Lighting, Heating]);
                    if day % 2 == 0 {
                        v.push(WashingMachine);
                    }
                } else if (19.0..22.0).contains(&h) {
                    v.extend([Lighting, Heating, Oven]);
                }
            }
            ResidentProfile::Teenager => {
                if (0.0..2.0).contains(&h) {
                    v.extend([Lighting, EvCharger]);
                } else if (10.0..12.0).contains(&h) {
                    v.push(Lighting);
                } else if (12.0..14.0).contains(&h) {
                    v.extend([Lighting, Oven]);
                } else if (18.0..24.0).contains(&h) {
                    v.extend([Lighting, Oven]);
                }
            }
        }
        v
    }

    /// Met à jour le confort sur un pas de temps `dt_h` selon l'état du réseau.
    pub fn update_comfort(&mut self, hour: f64, blackout: bool, dt_h: f64) {
        if blackout && self.awake(hour) {
            self.comfort = (self.comfort - 15.0 * dt_h).max(0.0);
        } else {
            self.comfort = (self.comfort + 2.0 * dt_h).min(100.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_is_away_midday() {
        let r = Resident::new("Alex", ResidentProfile::Worker);
        assert!(r.desired_appliances(13.0, 1).is_empty(), "actif absent à 13h");
        assert!(!r.desired_appliances(19.0, 1).is_empty(), "actif présent le soir");
    }

    #[test]
    fn routine_is_deterministic() {
        let r = Resident::new("Alex", ResidentProfile::Retiree);
        assert_eq!(r.desired_appliances(10.0, 4), r.desired_appliances(10.0, 4));
    }

    #[test]
    fn comfort_drops_on_blackout_when_awake() {
        let mut r = Resident::new("Alex", ResidentProfile::Worker);
        r.update_comfort(12.0, true, 1.0); // éveillé + black-out
        assert!(r.comfort < 100.0);
        let before = r.comfort;
        r.update_comfort(3.0, true, 1.0); // endormi → pas de pénalité
        assert!(r.comfort >= before);
    }
}
