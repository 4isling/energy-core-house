//! Physique de production par filière.
//!
//! Toutes les formules viennent des modèles standards (Betz pour l'éolien,
//! P = rho.g.Q.H.eta pour l'hydraulique, dérating thermique pour le solaire).
//! Les valeurs par défaut sont calées sur des ordres de grandeur français
//! réalistes. Unités : puissance en kW, vent en m/s, irradiance en kW/m²
//! (1.0 = conditions standard STC), débit en m³/s, hauteur en m.

use serde::{Deserialize, Serialize};

/// Masse volumique de l'air au niveau de la mer, 15 °C (kg/m³).
pub const AIR_DENSITY: f64 = 1.225;
/// Limite de Betz : rendement aérodynamique maximal théorique d'une éolienne.
pub const BETZ_LIMIT: f64 = 0.5926;
/// Masse volumique de l'eau (kg/m³).
pub const WATER_DENSITY: f64 = 1000.0;
/// Accélération de la pesanteur (m/s²).
pub const GRAVITY: f64 = 9.81;

/// Met à l'échelle une vitesse de vent mesurée à `ref_height` vers la hauteur
/// de moyeu via la loi de puissance. `alpha` ≈ 0.143 onshore, ≈ 0.11 offshore.
pub fn wind_at_height(ref_ms: f64, ref_height_m: f64, hub_height_m: f64, alpha: f64) -> f64 {
    if ref_height_m <= 0.0 {
        return ref_ms;
    }
    ref_ms * (hub_height_m / ref_height_m).powf(alpha)
}

// ---------------------------------------------------------------------------
// Éolien
// ---------------------------------------------------------------------------

/// Une éolienne. `cp` est le coefficient de puissance effectif (toujours <= Betz) ;
/// il encode le réglage du pitch et le rendement aérodynamique réel (~0.40–0.45).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindTurbine {
    pub rated_kw: f64,
    pub rotor_diameter_m: f64,
    pub hub_height_m: f64,
    pub cut_in_ms: f64,
    pub rated_ms: f64,
    pub cut_out_ms: f64,
    pub cp: f64,
}

impl WindTurbine {
    /// Éolienne terrestre type ~2 MW (gabarit courant en France).
    pub fn onshore_2mw() -> Self {
        Self {
            rated_kw: 2000.0,
            rotor_diameter_m: 90.0,
            hub_height_m: 100.0,
            cut_in_ms: 3.0,
            rated_ms: 12.0,
            cut_out_ms: 25.0,
            cp: 0.42,
        }
    }

    /// Surface balayée par le rotor (m²).
    pub fn swept_area_m2(&self) -> f64 {
        std::f64::consts::PI * (self.rotor_diameter_m / 2.0).powi(2)
    }

    /// Puissance instantanée (kW) pour une vitesse de vent à hauteur de moyeu.
    ///
    /// Trois régimes : nulle hors [cut_in, cut_out], loi en v³ jusqu'à la
    /// vitesse nominale, puis plafonnée à `rated_kw`.
    pub fn power_kw(&self, wind_ms: f64) -> f64 {
        let v = wind_ms;
        if v < self.cut_in_ms || v > self.cut_out_ms {
            return 0.0;
        }
        if v >= self.rated_ms {
            return self.rated_kw;
        }
        let cp = self.cp.clamp(0.0, BETZ_LIMIT);
        let p_watt = 0.5 * AIR_DENSITY * self.swept_area_m2() * v.powi(3) * cp;
        (p_watt / 1000.0).min(self.rated_kw)
    }
}

// ---------------------------------------------------------------------------
// Solaire photovoltaïque
// ---------------------------------------------------------------------------

/// Un champ photovoltaïque dimensionné par sa puissance crête (kWc).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolarArray {
    /// Puissance crête installée (kWc).
    pub kwc: f64,
    /// Performance ratio (pertes onduleur, câblage, salissure) ~0.75–0.85.
    pub perf_ratio: f64,
    /// Coefficient de température (par °C), typiquement -0.0035.
    pub temp_coeff_per_c: f64,
}

impl SolarArray {
    pub fn new(kwc: f64) -> Self {
        Self { kwc, perf_ratio: 0.80, temp_coeff_per_c: -0.0035 }
    }

    /// Puissance instantanée (kW).
    /// `irradiance_kw_m2` : 1.0 = conditions standard (1000 W/m²).
    pub fn power_kw(&self, irradiance_kw_m2: f64, air_temp_c: f64) -> f64 {
        if irradiance_kw_m2 <= 0.0 {
            return 0.0;
        }
        // Température de cellule approchée (montée ~25 °C à plein soleil).
        let cell_c = air_temp_c + irradiance_kw_m2 * 25.0;
        let derate = 1.0 + self.temp_coeff_per_c * (cell_c - 25.0);
        (self.kwc * irradiance_kw_m2 * self.perf_ratio * derate).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// Hydraulique
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydroKind {
    Pelton,    // haute chute, excellent en charge partielle
    Francis,   // moyenne chute, sensible au débit
    Kaplan,    // basse chute, pales réglables, courbe plate
    Waterwheel, // roue à aube, rendement modeste
}

/// Une turbine hydraulique / roue à aube.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HydroTurbine {
    pub kind: HydroKind,
    /// Débit nominal de dimensionnement (m³/s).
    pub design_flow_m3s: f64,
    /// Hauteur de chute nette (m).
    pub head_m: f64,
    /// Rendement de pointe (à débit nominal).
    pub peak_efficiency: f64,
}

impl HydroTurbine {
    pub fn new(kind: HydroKind, design_flow_m3s: f64, head_m: f64) -> Self {
        let peak_efficiency = match kind {
            HydroKind::Pelton => 0.90,
            HydroKind::Francis => 0.93,
            HydroKind::Kaplan => 0.92,
            HydroKind::Waterwheel => 0.65,
        };
        Self { kind, design_flow_m3s, head_m, peak_efficiency }
    }

    /// Facteur de rendement en charge partielle (0..1) selon le ratio de débit.
    /// Pelton/Kaplan restent bons en débit faible ; Francis a besoin d'un débit
    /// élevé ; la roue à aube est ~linéaire.
    fn part_load_factor(&self, ratio: f64) -> f64 {
        let r = ratio.clamp(0.0, 1.0);
        let f = match self.kind {
            HydroKind::Pelton | HydroKind::Kaplan => {
                if r < 0.10 { 0.0 } else if r < 0.25 { (r - 0.10) / 0.15 } else { 1.0 }
            }
            HydroKind::Francis => {
                if r < 0.40 { 0.0 } else if r < 0.70 { (r - 0.40) / 0.30 } else { 1.0 }
            }
            HydroKind::Waterwheel => r,
        };
        f.clamp(0.0, 1.0)
    }

    /// Puissance instantanée (kW) pour un débit disponible (m³/s).
    /// Le débit est écrêté au débit nominal (surplus by-passé).
    pub fn power_kw(&self, flow_m3s: f64) -> f64 {
        if self.design_flow_m3s <= 0.0 {
            return 0.0;
        }
        let q = flow_m3s.clamp(0.0, self.design_flow_m3s);
        let ratio = q / self.design_flow_m3s;
        let eff = self.peak_efficiency * self.part_load_factor(ratio);
        (WATER_DENSITY * GRAVITY * q * self.head_m * eff) / 1000.0
    }
}

// ---------------------------------------------------------------------------
// Thermique fossile (pilotable, polluant)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuelKind {
    Coal,
    GasCcgt,
    GasTac,
    Oil,
}

/// Une centrale thermique pilotable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThermalPlant {
    pub kind: FuelKind,
    pub rated_kw: f64,
}

impl ThermalPlant {
    pub fn new(kind: FuelKind, rated_kw: f64) -> Self {
        Self { kind, rated_kw }
    }

    /// Émissions cycle de vie (gCO2eq/kWh), méthodologie RTE Bilan électrique.
    pub fn co2_g_per_kwh(&self) -> f64 {
        match self.kind {
            FuelKind::Coal => 941.0,
            FuelKind::GasCcgt => 389.0,
            FuelKind::GasTac => 583.0,
            FuelKind::Oil => 928.0,
        }
    }

    /// Coût de combustible approché (€/kWh électrique produit).
    pub fn fuel_cost_eur_per_kwh(&self) -> f64 {
        match self.kind {
            FuelKind::Coal => 0.045,
            FuelKind::GasCcgt => 0.075,
            FuelKind::GasTac => 0.105,
            FuelKind::Oil => 0.120,
        }
    }

    /// Énergie maximale livrable sur un pas de temps (kWh).
    pub fn max_energy_kwh(&self, dt_h: f64) -> f64 {
        (self.rated_kw * dt_h).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_zero_outside_window() {
        let t = WindTurbine::onshore_2mw();
        assert_eq!(t.power_kw(2.0), 0.0); // sous cut-in
        assert_eq!(t.power_kw(26.0), 0.0); // au-delà cut-out
    }

    #[test]
    fn wind_caps_at_rated() {
        let t = WindTurbine::onshore_2mw();
        assert!((t.power_kw(15.0) - t.rated_kw).abs() < 1e-9);
        assert!((t.power_kw(24.0) - t.rated_kw).abs() < 1e-9);
    }

    #[test]
    fn wind_monotonic_in_ramp() {
        let t = WindTurbine::onshore_2mw();
        let a = t.power_kw(5.0);
        let b = t.power_kw(8.0);
        let c = t.power_kw(11.0);
        assert!(a < b && b < c, "{a} < {b} < {c}");
    }

    #[test]
    fn cp_never_exceeds_betz() {
        let mut t = WindTurbine::onshore_2mw();
        t.cp = 0.9; // réglage absurde
        // Le résultat reste borné par Betz (donc < ce qu'un cp=0.9 donnerait).
        let p = t.power_kw(8.0);
        let unbounded = 0.5 * AIR_DENSITY * t.swept_area_m2() * 8f64.powi(3) * BETZ_LIMIT / 1000.0;
        assert!(p <= unbounded + 1e-6);
    }

    #[test]
    fn solar_zero_at_night() {
        let s = SolarArray::new(3.0);
        assert_eq!(s.power_kw(0.0, 10.0), 0.0);
    }

    #[test]
    fn solar_scales_with_irradiance() {
        let s = SolarArray::new(3.0);
        assert!(s.power_kw(1.0, 20.0) > s.power_kw(0.4, 20.0));
    }

    #[test]
    fn hydro_matches_formula_at_design_flow() {
        let h = HydroTurbine::new(HydroKind::Francis, 2.0, 30.0);
        let expected = WATER_DENSITY * GRAVITY * 2.0 * 30.0 * 0.93 / 1000.0;
        assert!((h.power_kw(2.0) - expected).abs() < 1e-6);
    }

    #[test]
    fn hydro_clamps_excess_flow() {
        let h = HydroTurbine::new(HydroKind::Kaplan, 5.0, 8.0);
        assert!((h.power_kw(20.0) - h.power_kw(5.0)).abs() < 1e-9);
    }

    #[test]
    fn francis_dead_at_low_flow() {
        let h = HydroTurbine::new(HydroKind::Francis, 10.0, 20.0);
        assert_eq!(h.power_kw(2.0), 0.0); // 20 % de débit -> sous le seuil Francis
        assert!(HydroTurbine::new(HydroKind::Kaplan, 10.0, 20.0).power_kw(2.0) > 0.0);
    }
}
