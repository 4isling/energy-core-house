//! Cœur de la simulation : le parc, le dispatch (renouvelable -> batterie ->
//! thermique -> réseau) et le pas de temps. Déterministe et pur : aucune
//! dépendance au rendu. Une frame d'UI = un `TickReport`.

use serde::{Deserialize, Serialize};

use crate::appliance::ApplianceKind;
use crate::building::{Building, BuildingKind, BuildingReport};
use crate::economy::{
    self, capex_battery_per_kwh, capex_building, capex_hydro, capex_solar, capex_thermal,
    capex_wind, opex_hydro_year, opex_solar_year, opex_thermal_year, opex_wind_year, Economy,
};
use crate::physics::{HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::resident::ResidentProfile;
use crate::storage::Battery;
use crate::weather::Weather;

const HOURS_PER_YEAR: f64 = 8760.0;
const EPS: f64 = 1e-6;

/// Le **micro-réseau partagé** du village : les actifs de production et de
/// stockage construits par le joueur, mutualisés entre tous les bâtiments.
/// La demande, elle, vit dans les `Building` (`building.rs`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Park {
    pub wind: Vec<WindTurbine>,
    pub solar: Vec<SolarArray>,
    pub hydro: Vec<HydroTurbine>,
    pub thermal: Vec<ThermalPlant>,
    pub battery: Option<Battery>,
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
    /// Confort moyen du village (0..100), sur tous les habitants de tous les
    /// bâtiments. 100 s'il n'y a pas d'habitant.
    pub avg_comfort_pct: f64,
    /// Population totale du village (somme des habitants des bâtiments).
    pub population: u32,
    /// Détail par bâtiment (charge, confort, occupants) pour l'UI.
    pub buildings: Vec<BuildingReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimState {
    pub park: Park,
    /// Les bâtiments du village (foyers) : l'unité de demande du micro-réseau.
    pub buildings: Vec<Building>,
    pub economy: Economy,
    pub hour: f64,
    pub day: u32,
    /// Charge additionnelle globale (kW) — override manuel/tests, hors bâtiments.
    pub load_kw: f64,
    /// Compteur d'identifiants de bâtiments.
    next_building_id: u32,
    /// Compteur d'identifiants d'appareils, unique sur tout le village.
    next_appliance_id: u32,
}

impl SimState {
    pub fn new(starting_budget_eur: f64) -> Self {
        Self {
            park: Park::default(),
            buildings: Vec::new(),
            economy: Economy::new(starting_budget_eur),
            hour: 8.0,
            day: 1,
            load_kw: 0.0,
            next_building_id: 0,
            next_appliance_id: 0,
        }
    }

    /// Peuple un village de départ (quelques foyers déjà habités) pour que la
    /// boucle de jeu ait du sens dès le lancement. Ne touche pas au budget.
    pub fn seed_starter_village(&mut self) {
        self.add_building(BuildingKind::Family);
        self.add_building(BuildingKind::Elders);
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
        self.load_kw = kw.max(0.0);
    }

    // --- Bâtiments du village ---

    /// Construit un bâtiment (débite le CAPEX). Renvoie son id, ou `None` si le
    /// budget est insuffisant.
    pub fn build_building(&mut self, kind: BuildingKind) -> Option<u32> {
        let c = capex_building(kind);
        if self.economy.budget_eur < c {
            return None;
        }
        self.economy.budget_eur -= c;
        Some(self.add_building(kind))
    }

    /// Ajoute un bâtiment avec son loadout par défaut, sans toucher au budget
    /// (utilisé pour l'état initial). Renvoie son id.
    pub fn add_building(&mut self, kind: BuildingKind) -> u32 {
        let id = self.next_building_id;
        self.next_building_id += 1;
        let b = Building::from_kind(id, kind, self.next_appliance_id);
        self.next_appliance_id += b.appliances.len() as u32;
        self.buildings.push(b);
        id
    }

    fn building_mut(&mut self, building_id: u32) -> Option<&mut Building> {
        self.buildings.iter_mut().find(|b| b.id == building_id)
    }

    // --- Appareils & habitants (scopés par bâtiment) ---

    /// Ajoute un appareil à un bâtiment. Renvoie l'id d'appareil (unique dans le
    /// village), ou `None` si le bâtiment est inconnu.
    pub fn add_appliance_to(&mut self, building_id: u32, kind: ApplianceKind) -> Option<u32> {
        let id = self.next_appliance_id;
        match self.building_mut(building_id) {
            Some(b) => {
                b.add_appliance(id, kind);
                self.next_appliance_id += 1;
                Some(id)
            }
            None => None,
        }
    }

    /// Bascule un appareil par son id, où qu'il soit dans le village.
    pub fn toggle_appliance(&mut self, id: u32) -> bool {
        self.buildings.iter_mut().any(|b| b.toggle_appliance(id))
    }

    /// Ajoute un habitant à un bâtiment. Renvoie false si le bâtiment est inconnu.
    pub fn add_resident_to(
        &mut self,
        building_id: u32,
        name: impl Into<String>,
        profile: ResidentProfile,
    ) -> bool {
        match self.building_mut(building_id) {
            Some(b) => {
                b.add_resident(name, profile);
                true
            }
            None => false,
        }
    }

    /// Avance la simulation de `dt_h` heures sous une météo donnée.
    pub fn tick(&mut self, weather: &Weather, dt_h: f64) -> TickReport {
        let budget_before = self.economy.budget_eur;

        // 0. Dans chaque bâtiment, les habitants pilotent les appareils selon
        //    l'heure courante, puis on agrège la demande du village.
        let mut load_kw = self.load_kw;
        for b in &mut self.buildings {
            b.apply_resident_schedule(self.hour, self.day);
            load_kw += b.load_kw();
        }

        // 1. Production renouvelable instantanée (kW), mutualisée sur le réseau.
        let wind_kw: f64 = self.park.wind.iter().map(|t| t.power_kw(weather.wind_ms)).sum();
        let solar_kw: f64 = self.park.solar.iter()
            .map(|s| s.power_kw(weather.irradiance_kw_m2, weather.air_temp_c)).sum();
        let hydro_kw: f64 = self.park.hydro.iter().map(|h| h.power_kw(weather.river_flow_m3s)).sum();
        let renewable_kw = wind_kw + solar_kw + hydro_kw;

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

        // 3. Confort des habitants du village (un black-out pendant qu'ils sont
        //    éveillés fait baisser le confort). v1 : black-out à l'échelle du
        //    village, donc tous les bâtiments sont touchés. Évalué à l'heure du
        //    pas écoulé. On en profite pour bâtir le détail par bâtiment.
        let blackout = unmet_kwh > EPS;
        let hour_of_step = self.hour;
        let mut comfort_sum = 0.0;
        let mut population = 0u32;
        let mut building_reports = Vec::with_capacity(self.buildings.len());
        for b in &mut self.buildings {
            for r in &mut b.residents {
                r.update_comfort(hour_of_step, blackout, dt_h);
            }
            comfort_sum += b.residents.iter().map(|r| r.comfort).sum::<f64>();
            population += b.residents.len() as u32;
            building_reports.push(BuildingReport {
                id: b.id,
                name: b.name.clone(),
                kind: b.kind.label().to_string(),
                load_kw: b.load_kw(),
                avg_comfort_pct: b.avg_comfort_pct(),
                resident_count: b.residents.len() as u32,
            });
        }
        let avg_comfort_pct = if population == 0 {
            100.0
        } else {
            comfort_sum / population as f64
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
            population,
            buildings: building_reports,
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
        let b = s.add_building(BuildingKind::Studio);
        // Vide les habitants pour isoler l'effet de l'appareil ajouté.
        s.building_mut(b).unwrap().residents.clear();
        let base = s.tick(&weather(0.0, 0.0), 1.0).load_kw;
        let id = s.add_appliance_to(b, ApplianceKind::Oven).unwrap();
        s.toggle_appliance(id); // allume le four (éteint par défaut)
        let with = s.tick(&weather(0.0, 0.0), 1.0).load_kw;
        assert!(with > base, "le four allumé augmente la charge ({with} > {base})");
    }

    #[test]
    fn resident_drives_appliances_deterministically() {
        let build = || {
            let mut s = SimState::new(50_000.0);
            s.add_building(BuildingKind::Family);
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
    fn village_load_is_sum_of_buildings() {
        // Un village de deux foyers consomme plus qu'un seul, même scénario.
        let mut one = SimState::new(50_000.0);
        one.add_building(BuildingKind::Family);
        let mut two = SimState::new(50_000.0);
        two.add_building(BuildingKind::Family);
        two.add_building(BuildingKind::Family);
        // Heure du soir : les foyers consomment.
        one.hour = 19.0;
        two.hour = 19.0;
        let l1 = one.tick(&weather(0.0, 0.0), 0.5).load_kw;
        let l2 = two.tick(&weather(0.0, 0.0), 0.5).load_kw;
        assert!(l2 > l1, "deux foyers ({l2}) consomment plus qu'un ({l1})");
    }

    #[test]
    fn building_grows_population_and_demand() {
        let mut s = SimState::new(1_000_000.0);
        let r0 = s.tick(&weather(0.0, 0.0), 0.5);
        assert_eq!(r0.population, 0);
        s.hour = 19.0;
        let before = s.tick(&weather(0.0, 0.0), 0.5).load_kw;
        s.build_building(BuildingKind::Family).expect("budget suffisant");
        let after = s.tick(&weather(0.0, 0.0), 0.5);
        assert!(after.population >= 2, "le foyer apporte des habitants");
        assert!(after.load_kw > before, "le nouveau bâtiment augmente la demande");
        assert_eq!(after.buildings.len(), 1, "le détail par bâtiment est présent");
    }

    #[test]
    fn blackout_lowers_comfort_across_village() {
        let mut s = SimState::new(50_000.0);
        s.economy.grid.connected = false;
        // Deux foyers différents, aucune production -> black-out village.
        s.add_building(BuildingKind::Family);
        s.add_building(BuildingKind::Elders);
        s.hour = 12.0; // habitants éveillés
        let r = s.tick(&weather(0.0, 0.0), 1.0);
        assert!(r.blackout);
        assert!(r.avg_comfort_pct < 100.0, "confort doit chuter, obtenu {}", r.avg_comfort_pct);
        // Tous les bâtiments habités voient leur confort baisser.
        for b in &r.buildings {
            assert!(b.avg_comfort_pct < 100.0, "{} touché par le black-out", b.name);
        }
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
