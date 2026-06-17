//! Cœur de la simulation : le parc, le dispatch (renouvelable -> batterie ->
//! thermique -> réseau) et le pas de temps. Déterministe et pur : aucune
//! dépendance au rendu. Une frame d'UI = un `TickReport`.

use serde::{Deserialize, Serialize};

use crate::economy::{
    self, capex_battery_per_kwh, capex_hydro, capex_solar, capex_thermal, capex_wind,
    opex_hydro_year, opex_solar_year, opex_thermal_year, opex_wind_year, Economy,
};
use crate::physics::{HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::storage::Battery;
use crate::weather::Weather;

const HOURS_PER_YEAR: f64 = 8760.0;
const EPS: f64 = 1e-6;

/// L'ensemble des actifs construits par le joueur.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Park {
    pub wind: Vec<WindTurbine>,
    pub solar: Vec<SolarArray>,
    pub hydro: Vec<HydroTurbine>,
    pub thermal: Vec<ThermalPlant>,
    pub battery: Option<Battery>,
    /// Demande instantanée à couvrir (kW). À piloter depuis un profil de charge.
    pub load_kw: f64,
}

impl Park {
    pub fn opex_year(&self) -> f64 {
        let mut o = 0.0;
        for t in &self.wind { o += opex_wind_year(t); }
        for s in &self.solar { o += opex_solar_year(s); }
        for h in &self.hydro { o += opex_hydro_year(h); }
        for t in &self.thermal { o += opex_thermal_year(t); }
        o
    }
}

/// Bilan d'un pas de temps : tout ce dont l'UI a besoin pour une frame.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TickReport {
    pub hour: f64,
    pub day: u32,
    pub wind_kw: f64,
    pub solar_kw: f64,
    pub hydro_kw: f64,
    pub thermal_kw: f64,
    /// Puissance batterie : positive en décharge, négative en charge.
    pub battery_kw: f64,
    pub import_kw: f64,
    pub export_kw: f64,
    pub load_kw: f64,
    pub unmet_kw: f64,
    pub blackout: bool,
    pub soc_pct: f64,
    pub co2_kg_step: f64,
    pub cash_flow_eur: f64,
    pub budget_eur: f64,
    pub co2_kg_total: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimState {
    pub park: Park,
    pub economy: Economy,
    pub hour: f64,
    pub day: u32,
}

impl SimState {
    pub fn new(starting_budget_eur: f64) -> Self {
        Self { park: Park::default(), economy: Economy::new(starting_budget_eur), hour: 8.0, day: 1 }
    }

    // --- Construction (renvoie false si budget insuffisant) ---

    pub fn build_wind(&mut self, t: WindTurbine) -> bool {
        let c = capex_wind(&t);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.wind.push(t);
        true
    }
    pub fn build_solar(&mut self, s: SolarArray) -> bool {
        let c = capex_solar(&s);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.solar.push(s);
        true
    }
    pub fn build_hydro(&mut self, h: HydroTurbine) -> bool {
        let c = capex_hydro(&h);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.hydro.push(h);
        true
    }
    pub fn build_thermal(&mut self, t: ThermalPlant) -> bool {
        let c = capex_thermal(&t);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.thermal.push(t);
        true
    }
    /// Ajoute de la capacité batterie (fusionne avec l'existante).
    pub fn build_battery(&mut self, capacity_kwh: f64) -> bool {
        let c = capacity_kwh * capex_battery_per_kwh();
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        match &mut self.park.battery {
            Some(b) => {
                b.capacity_kwh += capacity_kwh;
                b.max_charge_kw += capacity_kwh * 0.5;
                b.max_discharge_kw += capacity_kwh * 0.5;
            }
            None => self.park.battery = Some(Battery::new(capacity_kwh)),
        }
        true
    }

    pub fn set_load_kw(&mut self, kw: f64) {
        self.park.load_kw = kw.max(0.0);
    }

    /// Avance la simulation de `dt_h` heures sous une météo donnée.
    pub fn tick(&mut self, weather: &Weather, dt_h: f64) -> TickReport {
        let budget_before = self.economy.budget_eur;

        // 1. Production renouvelable instantanée (kW).
        let wind_kw: f64 = self.park.wind.iter().map(|t| t.power_kw(weather.wind_ms)).sum();
        let solar_kw: f64 = self.park.solar.iter()
            .map(|s| s.power_kw(weather.irradiance_kw_m2, weather.air_temp_c)).sum();
        let hydro_kw: f64 = self.park.hydro.iter().map(|h| h.power_kw(weather.river_flow_m3s)).sum();
        let renewable_kw = wind_kw + solar_kw + hydro_kw;

        let load_kw = self.park.load_kw;
        let net_kwh = (renewable_kw - load_kw) * dt_h;

        let mut thermal_kwh = 0.0;
        let mut battery_kwh = 0.0; // + décharge, - charge
        let mut import_kwh = 0.0;
        let mut export_kwh = 0.0;
        let mut unmet_kwh = 0.0;
        let mut co2_step = 0.0;

        if net_kwh >= 0.0 {
            // Surplus -> batterie -> export/réseau.
            let mut surplus = net_kwh;
            if let Some(b) = &mut self.park.battery {
                let charged = b.charge(surplus, dt_h);
                battery_kwh -= charged;
                surplus -= charged;
            }
            if surplus > 0.0 && self.economy.grid.connected {
                export_kwh = surplus;
                self.economy.budget_eur += self.economy.export_revenue(surplus);
            }
        } else {
            // Déficit -> batterie -> thermique -> import réseau -> non-fourni.
            let mut deficit = -net_kwh;

            if let Some(b) = &mut self.park.battery {
                let d = b.discharge(deficit, dt_h);
                battery_kwh += d;
                deficit -= d;
            }

            for plant in &self.park.thermal {
                if deficit <= EPS { break; }
                let gen = plant.max_energy_kwh(dt_h).min(deficit);
                deficit -= gen;
                thermal_kwh += gen;
                self.economy.budget_eur -= gen * plant.fuel_cost_eur_per_kwh();
                let c = economy::Economy::co2_of_fuel(plant.kind, gen);
                self.economy.co2_kg += c;
                co2_step += c;
            }

            if deficit > EPS && self.economy.grid.connected {
                import_kwh = deficit;
                self.economy.budget_eur -= self.economy.import_cost(deficit);
                let c = deficit * self.economy.grid_co2_g_per_kwh() / 1000.0;
                self.economy.co2_kg += c;
                co2_step += c;
                deficit = 0.0;
            }

            unmet_kwh = deficit.max(0.0);
        }

        // 2. OPEX du pas de temps.
        let opex_step = self.park.opex_year() * dt_h / HOURS_PER_YEAR;
        self.economy.budget_eur -= opex_step;

        // 3. Avance l'horloge.
        self.hour += dt_h;
        while self.hour >= 24.0 {
            self.hour -= 24.0;
            self.day += 1;
        }

        let inv_dt = if dt_h > 0.0 { 1.0 / dt_h } else { 0.0 };
        TickReport {
            hour: self.hour,
            day: self.day,
            wind_kw,
            solar_kw,
            hydro_kw,
            thermal_kw: thermal_kwh * inv_dt,
            battery_kw: battery_kwh * inv_dt,
            import_kw: import_kwh * inv_dt,
            export_kw: export_kwh * inv_dt,
            load_kw,
            unmet_kw: unmet_kwh * inv_dt,
            blackout: unmet_kwh > EPS,
            soc_pct: self.park.battery.as_ref().map(|b| b.soc_pct()).unwrap_or(0.0),
            co2_kg_step: co2_step,
            cash_flow_eur: self.economy.budget_eur - budget_before,
            budget_eur: self.economy.budget_eur,
            co2_kg_total: self.economy.co2_kg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{FuelKind, WindTurbine};

    fn weather(wind: f64, irr: f64) -> Weather {
        Weather { wind_ms: wind, irradiance_kw_m2: irr, air_temp_c: 15.0, river_flow_m3s: 3.0 }
    }

    #[test]
    fn surplus_charges_battery() {
        let mut s = SimState::new(10_000_000.0);
        s.build_wind(WindTurbine::onshore_2mw());
        s.build_battery(1000.0);
        s.set_load_kw(100.0); // bien moins que la prod en vent fort
        let r = s.tick(&weather(13.0, 0.0), 1.0);
        assert!(r.battery_kw < 0.0, "la batterie doit se charger (négatif)");
        assert!(!r.blackout);
    }

    #[test]
    fn deficit_without_backup_blacks_out() {
        let mut s = SimState::new(1_000_000.0);
        s.economy.grid.connected = false; // hors réseau
        s.set_load_kw(500.0); // aucune source -> tout en déficit
        let r = s.tick(&weather(0.0, 0.0), 1.0);
        assert!(r.blackout);
        assert!(r.unmet_kw > 0.0);
    }

    #[test]
    fn thermal_covers_deficit_and_emits_co2() {
        let mut s = SimState::new(1_000_000.0);
        s.economy.grid.connected = false;
        s.build_thermal(ThermalPlant::new(FuelKind::Coal, 1000.0));
        s.set_load_kw(800.0);
        let r = s.tick(&weather(0.0, 0.0), 1.0);
        assert!(!r.blackout, "le charbon doit couvrir 800 kW < 1000 kW");
        assert!(r.thermal_kw > 0.0);
        assert!(r.co2_kg_step > 0.0, "le charbon doit émettre du CO2");
        assert!(r.cash_flow_eur < 0.0, "le combustible coûte de l'argent");
    }

    #[test]
    fn clock_advances_days() {
        let mut s = SimState::new(1000.0);
        for _ in 0..50 {
            s.tick(&weather(6.0, 0.0), 0.5);
        }
        assert!(s.day >= 2, "25 h écoulées -> jour 2, obtenu {}", s.day);
    }
}
