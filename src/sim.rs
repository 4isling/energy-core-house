//! Cœur de la simulation : le parc, le dispatch (renouvelable -> batterie ->
//! thermique -> réseau) et le pas de temps. Déterministe et pur : aucune
//! dépendance au rendu. Une frame d'UI = un `TickReport`.

use serde::{Deserialize, Serialize};

use crate::appliance::{Appliance, ApplianceKind};
use crate::economy::{
    self, capex_battery_per_kwh, capex_hydro, capex_solar, capex_thermal, capex_wind,
    opex_hydro_year, opex_solar_year, opex_thermal_year, opex_wind_year, Economy,
};
use crate::physics::{HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::resident::{Resident, ResidentProfile};
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
    /// Appareils consommateurs de la maison.
    pub appliances: Vec<Appliance>,
    /// Habitants (NPC) qui pilotent les appareils au fil de la journée.
    pub residents: Vec<Resident>,
    /// Charge additionnelle imposée manuellement (kW) — override/tests.
    /// La charge totale = somme des appareils allumés + `load_kw`.
    pub load_kw: f64,
    /// Compteur d'identifiants d'appareils.
    next_appliance_id: u32,
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

    /// Charge appelée par les appareils allumés (kW).
    pub fn appliance_load_kw(&self) -> f64 {
        self.appliances.iter().map(|a| a.draw_kw()).sum()
    }

    /// Ajoute un appareil d'une catégorie donnée, renvoie son id.
    pub fn add_appliance(&mut self, kind: ApplianceKind) -> u32 {
        let id = self.next_appliance_id;
        self.next_appliance_id += 1;
        self.appliances.push(Appliance::from_kind(id, kind));
        id
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

    /// Applique la routine des habitants : un appareil est allumé si au moins
    /// un résident le souhaite à cette heure, ou s'il tourne en continu (frigo).
    pub fn apply_resident_schedule(&mut self, hour: f64, day: u32) {
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
    /// Confort moyen des habitants (0..100). 100 s'il n'y a pas d'habitant.
    pub avg_comfort_pct: f64,
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

    // --- Appareils & habitants ---

    pub fn add_appliance(&mut self, kind: ApplianceKind) -> u32 {
        self.park.add_appliance(kind)
    }
    pub fn toggle_appliance(&mut self, id: u32) -> bool {
        self.park.toggle_appliance(id)
    }
    pub fn add_resident(&mut self, name: impl Into<String>, profile: ResidentProfile) {
        self.park.residents.push(Resident::new(name, profile));
    }

    /// Avance la simulation de `dt_h` heures sous une météo donnée.
    pub fn tick(&mut self, weather: &Weather, dt_h: f64) -> TickReport {
        let budget_before = self.economy.budget_eur;

        // 0. Les habitants pilotent les appareils selon l'heure courante.
        self.park.apply_resident_schedule(self.hour, self.day);

        // 1. Production renouvelable instantanée (kW).
        let wind_kw: f64 = self.park.wind.iter().map(|t| t.power_kw(weather.wind_ms)).sum();
        let solar_kw: f64 = self.park.solar.iter()
            .map(|s| s.power_kw(weather.irradiance_kw_m2, weather.air_temp_c)).sum();
        let hydro_kw: f64 = self.park.hydro.iter().map(|h| h.power_kw(weather.river_flow_m3s)).sum();
        let renewable_kw = wind_kw + solar_kw + hydro_kw;

        // Charge = appareils allumés + override manuel.
        let load_kw = self.park.appliance_load_kw() + self.park.load_kw;
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

        // 3. Confort des habitants (un black-out pendant qu'ils sont éveillés
        //    fait baisser le confort). Évalué à l'heure du pas écoulé.
        let blackout = unmet_kwh > EPS;
        let hour_of_step = self.hour;
        for r in &mut self.park.residents {
            r.update_comfort(hour_of_step, blackout, dt_h);
        }
        let avg_comfort_pct = if self.park.residents.is_empty() {
            100.0
        } else {
            self.park.residents.iter().map(|r| r.comfort).sum::<f64>()
                / self.park.residents.len() as f64
        };

        // 4. Avance l'horloge.
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
            blackout,
            soc_pct: self.park.battery.as_ref().map(|b| b.soc_pct()).unwrap_or(0.0),
            co2_kg_step: co2_step,
            cash_flow_eur: self.economy.budget_eur - budget_before,
            budget_eur: self.economy.budget_eur,
            co2_kg_total: self.economy.co2_kg,
            avg_comfort_pct,
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
    fn appliance_increases_load() {
        use crate::appliance::ApplianceKind;
        let mut s = SimState::new(50_000.0);
        let base = s.tick(&weather(0.0, 0.0), 1.0).load_kw;
        let id = s.add_appliance(ApplianceKind::Oven);
        s.park.toggle_appliance(id); // allume le four (éteint par défaut)
        let with = s.tick(&weather(0.0, 0.0), 1.0).load_kw;
        assert!(with > base, "le four allumé augmente la charge ({with} > {base})");
    }

    #[test]
    fn resident_drives_appliances_deterministically() {
        use crate::appliance::ApplianceKind;
        use crate::resident::ResidentProfile;
        let build = || {
            let mut s = SimState::new(50_000.0);
            s.add_appliance(ApplianceKind::Oven);
            s.add_appliance(ApplianceKind::EvCharger);
            s.add_resident("Alex", ResidentProfile::Worker);
            s
        };
        let mut a = build();
        let mut b = build();
        let mut load_a = Vec::new();
        let mut load_b = Vec::new();
        for _ in 0..48 {
            load_a.push(a.tick(&weather(5.0, 0.5), 0.5).load_kw);
            load_b.push(b.tick(&weather(5.0, 0.5), 0.5).load_kw);
        }
        assert_eq!(load_a, load_b, "même scénario -> même courbe de charge");
        // La charge doit varier sur la journée (l'actif est absent à midi).
        assert!(load_a.iter().cloned().fold(0.0_f64, f64::max) > 0.0);
        assert!(load_a.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-9));
    }

    #[test]
    fn blackout_lowers_comfort() {
        use crate::resident::ResidentProfile;
        let mut s = SimState::new(50_000.0);
        s.economy.grid.connected = false;
        s.add_resident("Alex", ResidentProfile::Retiree);
        s.set_load_kw(3.0); // aucune prod -> black-out
        s.hour = 12.0; // habitant éveillé
        let r = s.tick(&weather(0.0, 0.0), 1.0);
        assert!(r.blackout);
        assert!(r.avg_comfort_pct < 100.0, "confort doit chuter, obtenu {}", r.avg_comfort_pct);
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
