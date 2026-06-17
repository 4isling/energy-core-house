//! Stockage : batterie agrégée avec rendement aller-retour et limites de
//! puissance. Les pertes du round-trip sont réparties symétriquement
//! (sqrt à la charge et à la décharge).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Battery {
    pub capacity_kwh: f64,
    pub soc_kwh: f64,
    /// Rendement aller-retour (round-trip), ~0.85–0.95.
    pub round_trip_eff: f64,
    pub max_charge_kw: f64,
    pub max_discharge_kw: f64,
}

impl Battery {
    /// Batterie Li-ion/LFP type. Démarre à 50 % de charge.
    pub fn new(capacity_kwh: f64) -> Self {
        Self {
            capacity_kwh,
            soc_kwh: capacity_kwh * 0.5,
            round_trip_eff: 0.90,
            // Par défaut ~0.5C : puissance = moitié de la capacité.
            max_charge_kw: capacity_kwh * 0.5,
            max_discharge_kw: capacity_kwh * 0.5,
        }
    }

    fn one_way_eff(&self) -> f64 {
        self.round_trip_eff.clamp(0.01, 1.0).sqrt()
    }

    pub fn soc_pct(&self) -> f64 {
        if self.capacity_kwh <= 0.0 { 0.0 } else { self.soc_kwh / self.capacity_kwh * 100.0 }
    }

    /// Tente d'absorber `avail_kwh` depuis le bus. Retourne l'énergie
    /// réellement prélevée sur le bus (avant pertes).
    pub fn charge(&mut self, avail_kwh: f64, dt_h: f64) -> f64 {
        if avail_kwh <= 0.0 {
            return 0.0;
        }
        let power_cap = self.max_charge_kw * dt_h;
        let from_bus = avail_kwh.min(power_cap);
        let space = (self.capacity_kwh - self.soc_kwh).max(0.0);
        let eff = self.one_way_eff();
        let stored = (from_bus * eff).min(space);
        self.soc_kwh += stored;
        // Énergie effectivement tirée du bus pour stocker `stored`.
        if eff > 0.0 { stored / eff } else { 0.0 }
    }

    /// Tente de fournir `need_kwh` au bus. Retourne l'énergie réellement
    /// livrée (après pertes).
    pub fn discharge(&mut self, need_kwh: f64, dt_h: f64) -> f64 {
        if need_kwh <= 0.0 {
            return 0.0;
        }
        let power_cap = self.max_discharge_kw * dt_h;
        let want = need_kwh.min(power_cap);
        let eff = self.one_way_eff();
        let drawn = (want / eff).min(self.soc_kwh);
        let delivered = drawn * eff;
        self.soc_kwh -= drawn;
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_loses_energy() {
        let mut b = Battery::new(100.0);
        b.soc_kwh = 0.0;
        let drawn = b.charge(10.0, 10.0); // pas de limite de puissance ici
        let out = b.discharge(100.0, 10.0);
        assert!(out < drawn, "sortie {out} < entrée {drawn}");
        // ~0.90 de round-trip
        assert!((out / drawn - 0.90).abs() < 0.02);
    }

    #[test]
    fn cannot_overfill() {
        let mut b = Battery::new(10.0);
        b.soc_kwh = 9.0;
        b.max_charge_kw = 1000.0;
        let taken = b.charge(100.0, 1.0);
        assert!(b.soc_kwh <= 10.0 + 1e-9);
        assert!(taken > 0.0);
    }

    #[test]
    fn power_limited() {
        let mut b = Battery::new(100.0);
        b.max_discharge_kw = 10.0;
        let out = b.discharge(1000.0, 1.0); // demande énorme, 1 h
        assert!(out <= 10.0 + 1e-9);
    }
}
