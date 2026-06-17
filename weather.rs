//! Météo. `Weather` porte les grandeurs physiques que la simulation consomme.
//! Tu peux la fixer toi-même (depuis un CSV éCO2mix / Météo-France / PVGIS)
//! ou utiliser `ProceduralWeather` pour un prototype jouable sans données.

use serde::{Deserialize, Serialize};

/// Conditions météo à un instant donné.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Weather {
    /// Vitesse du vent à hauteur de moyeu (m/s).
    pub wind_ms: f64,
    /// Irradiance normalisée (1.0 = 1000 W/m²).
    pub irradiance_kw_m2: f64,
    /// Température de l'air (°C).
    pub air_temp_c: f64,
    /// Débit disponible à la prise d'eau (m³/s).
    pub river_flow_m3s: f64,
}

impl Default for Weather {
    fn default() -> Self {
        Self { wind_ms: 6.0, irradiance_kw_m2: 0.0, air_temp_c: 15.0, river_flow_m3s: 3.0 }
    }
}

/// RNG xorshift minimal : déterministe, sans dépendance, identique web/natif.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }
    /// Flottant uniforme dans [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Génère une météo plausible heure par heure : cycle diurne pour le solaire,
/// marche aléatoire bornée pour le vent, saison pour la température et le débit.
/// Déterministe à graine fixée — parfait pour les replays et les tests.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProceduralWeather {
    rng: Rng,
    wind_ms: f64,
    /// Facteur saisonnier d'ensoleillement (0.4 hiver .. 1.0 été).
    pub season_sun: f64,
    pub base_flow_m3s: f64,
}

impl ProceduralWeather {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), wind_ms: 6.0, season_sun: 0.8, base_flow_m3s: 3.0 }
    }

    /// `hour` en heures (peut dépasser 24, on prend le modulo).
    pub fn sample(&mut self, hour: f64) -> Weather {
        let h = hour.rem_euclid(24.0);

        // Solaire : arche sinusoïdale 6h–20h, modulée par la saison et un aléa nuageux.
        let daylight = if (6.0..=20.0).contains(&h) {
            (std::f64::consts::PI * (h - 6.0) / 14.0).sin().max(0.0)
        } else {
            0.0
        };
        let cloud = 0.5 + 0.5 * self.rng.next_f64(); // 0.5..1.0
        let irradiance = daylight * self.season_sun * cloud;

        // Vent : marche aléatoire bornée [0.5, 22] m/s.
        let step = (self.rng.next_f64() - 0.5) * 2.0;
        self.wind_ms = (self.wind_ms + step).clamp(0.5, 22.0);

        // Température : creux la nuit, pic l'après-midi, décalée par la saison.
        let temp = 8.0 + 14.0 * self.season_sun + 6.0 * (std::f64::consts::PI * (h - 9.0) / 12.0).sin();

        // Débit : base + bruit léger (la pluie remonterait ce chiffre).
        let flow = (self.base_flow_m3s * (0.8 + 0.4 * self.rng.next_f64())).max(0.0);

        Weather { wind_ms: self.wind_ms, irradiance_kw_m2: irradiance, air_temp_c: temp, river_flow_m3s: flow }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_with_same_seed() {
        let mut a = ProceduralWeather::new(42);
        let mut b = ProceduralWeather::new(42);
        for h in 0..48 {
            let wa = a.sample(h as f64 * 0.5);
            let wb = b.sample(h as f64 * 0.5);
            assert_eq!(wa.wind_ms, wb.wind_ms);
            assert_eq!(wa.irradiance_kw_m2, wb.irradiance_kw_m2);
        }
    }

    #[test]
    fn no_sun_at_night() {
        let mut w = ProceduralWeather::new(7);
        let night = w.sample(2.0);
        assert_eq!(night.irradiance_kw_m2, 0.0);
    }
}
