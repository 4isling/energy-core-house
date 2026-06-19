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
use crate::map::{TerrainMap, TerrainTile};
use crate::physics::{HydroTurbine, SolarArray, ThermalPlant, WindTurbine};
use crate::resident::ResidentProfile;
use crate::storage::Battery;
use crate::weather::Weather;

const HOURS_PER_YEAR: f64 = 8760.0;
const EPS: f64 = 1e-6;

/// Facteurs de terrain capturés à la pose d'un actif : ils **modulent** les
/// entrées de la physique (`physics.rs`) sans changer aucune formule
/// (`vent_local = météo.vent × wind_factor`, idem soleil et débit d'eau). Un
/// environnement *neutre* (tout à 1.0) reproduit le comportement hors-carte.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TileEnv {
    pub wind_factor: f64,
    pub solar_factor: f64,
    pub water_factor: f64,
}

impl TileEnv {
    /// Environnement neutre : la production utilise la météo brute.
    pub const NEUTRAL: TileEnv = TileEnv { wind_factor: 1.0, solar_factor: 1.0, water_factor: 1.0 };

    pub fn from_tile(t: &TerrainTile) -> Self {
        Self {
            wind_factor: t.wind_factor as f64,
            solar_factor: t.solar_factor as f64,
            water_factor: t.water_factor as f64,
        }
    }
}

/// Un actif posé sur une tuile : coordonnées + environnement terrain capturé.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Placed<T> {
    pub x: u16,
    pub y: u16,
    pub env: TileEnv,
    pub asset: T,
}

impl<T> Placed<T> {
    /// Pose hors-carte (env neutre), pour les constructeurs non spatiaux/tests.
    fn off_map(asset: T) -> Self {
        Self { x: 0, y: 0, env: TileEnv::NEUTRAL, asset }
    }
}

/// Échec de construction sur la carte, pour un retour d'UI clair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildError {
    /// Budget insuffisant.
    Budget,
    /// Coordonnées hors de la carte (ou carte non générée).
    OutOfBounds,
    /// Tuile déjà occupée par un autre élément.
    Occupied,
    /// Le terrain interdit cet élément (ex. hydro hors rivière, bâti en montagne).
    BadTerrain,
}

/// Le **micro-réseau partagé** du village : les actifs de production et de
/// stockage construits par le joueur, mutualisés entre tous les bâtiments.
/// Chaque actif est **placé** sur une tuile (cf. `Placed`). La demande, elle,
/// vit dans les `Building` (`building.rs`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Park {
    pub wind: Vec<Placed<WindTurbine>>,
    pub solar: Vec<Placed<SolarArray>>,
    pub hydro: Vec<Placed<HydroTurbine>>,
    pub thermal: Vec<Placed<ThermalPlant>>,
    /// Capacité batterie agrégée (le dispatch traite un seul stock mutualisé).
    pub battery: Option<Battery>,
    /// Tuiles où des batteries ont été posées (pour le rendu ; la capacité est
    /// agrégée dans `battery`).
    pub battery_tiles: Vec<(u16, u16)>,
}

impl Park {
    pub fn opex_year(&self) -> f64 {
        let mut o = 0.0;
        for t in &self.wind { o += opex_wind_year(&t.asset); }
        for s in &self.solar { o += opex_solar_year(&s.asset); }
        for h in &self.hydro { o += opex_hydro_year(&h.asset); }
        for t in &self.thermal { o += opex_thermal_year(&t.asset); }
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
    /// Revenu instantané du village (€/jour) : salaires/pensions des habitants,
    /// pondérés par leur confort. Crédité au budget à chaque pas de temps.
    pub revenue_eur_day: f64,
    /// Détail par bâtiment (charge, confort, occupants) pour l'UI.
    pub buildings: Vec<BuildingReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimState {
    pub park: Park,
    /// Les bâtiments du village (foyers) : l'unité de demande du micro-réseau.
    pub buildings: Vec<Building>,
    /// La carte de terrain. Vide (0×0) tant que `generate_map` n'a pas été appelé
    /// (les constructeurs non spatiaux fonctionnent sans carte).
    pub map: TerrainMap,
    pub economy: Economy,
    pub hour: f64,
    pub day: u32,
    /// Charge additionnelle globale (kW) — override manuel/tests, hors bâtiments.
    pub load_kw: f64,
    /// Occupation des tuiles (parallèle à `map.tiles`) : empêche deux poses sur
    /// la même tuile. Vide tant que la carte n'est pas générée.
    occupied: Vec<bool>,
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
            map: TerrainMap::default(),
            economy: Economy::new(starting_budget_eur),
            hour: 8.0,
            day: 1,
            load_kw: 0.0,
            occupied: Vec::new(),
            next_building_id: 0,
            next_appliance_id: 0,
        }
    }

    /// Génère la carte de terrain depuis un seed (500×500 par défaut) et réinit
    /// l'occupation. À appeler une fois au démarrage d'une partie spatiale.
    pub fn generate_map(&mut self, seed: u64) {
        self.map = TerrainMap::generate(seed);
        self.occupied = vec![false; self.map.len()];
    }

    /// Variante de `generate_map` à taille choisie (tests / petites cartes).
    pub fn generate_map_sized(&mut self, seed: u64, w: u16, h: u16) {
        self.map = TerrainMap::generate_sized(seed, w, h);
        self.occupied = vec![false; self.map.len()];
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
        self.park.wind.push(Placed::off_map(t));
        true
    }
    pub fn build_solar(&mut self, s: SolarArray) -> bool {
        let c = capex_solar(&s);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.solar.push(Placed::off_map(s));
        true
    }
    pub fn build_hydro(&mut self, h: HydroTurbine) -> bool {
        let c = capex_hydro(&h);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.hydro.push(Placed::off_map(h));
        true
    }
    pub fn build_thermal(&mut self, t: ThermalPlant) -> bool {
        let c = capex_thermal(&t);
        if self.economy.budget_eur < c { return false; }
        self.economy.budget_eur -= c;
        self.park.thermal.push(Placed::off_map(t));
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

    // --- Construction spatiale sur la carte (renvoie une `BuildError`) ---

    /// Réserve une tuile **terrestre constructible** et renvoie l'environnement
    /// terrain capturé. Ne touche ni au budget ni aux actifs.
    fn claim_land_tile(&mut self, x: u16, y: u16) -> Result<TileEnv, BuildError> {
        let tile = self.map.get(x, y).ok_or(BuildError::OutOfBounds)?;
        if !tile.buildable() {
            return Err(BuildError::BadTerrain);
        }
        let env = TileEnv::from_tile(tile);
        let i = self.map.idx(x, y);
        if self.occupied[i] {
            return Err(BuildError::Occupied);
        }
        self.occupied[i] = true;
        Ok(env)
    }

    /// Réserve une tuile **d'eau** (rivière) pour l'hydraulique.
    fn claim_water_tile(&mut self, x: u16, y: u16) -> Result<TileEnv, BuildError> {
        let tile = self.map.get(x, y).ok_or(BuildError::OutOfBounds)?;
        if !tile.is_water() {
            return Err(BuildError::BadTerrain);
        }
        let env = TileEnv::from_tile(tile);
        let i = self.map.idx(x, y);
        if self.occupied[i] {
            return Err(BuildError::Occupied);
        }
        self.occupied[i] = true;
        Ok(env)
    }

    /// Pose un champ solaire (kWc) sur une tuile constructible.
    pub fn build_solar_at(&mut self, x: u16, y: u16, kwc: f64) -> Result<(), BuildError> {
        let s = SolarArray::new(kwc);
        let c = capex_solar(&s);
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        let env = self.claim_land_tile(x, y)?;
        self.economy.budget_eur -= c;
        self.park.solar.push(Placed { x, y, env, asset: s });
        Ok(())
    }

    /// Pose une micro-éolienne domestique sur une tuile constructible.
    pub fn build_wind_at(&mut self, x: u16, y: u16) -> Result<(), BuildError> {
        let t = WindTurbine::micro();
        let c = capex_wind(&t);
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        let env = self.claim_land_tile(x, y)?;
        self.economy.budget_eur -= c;
        self.park.wind.push(Placed { x, y, env, asset: t });
        Ok(())
    }

    /// Pose une turbine hydraulique sur une tuile **de rivière** (le débit local
    /// vient de la tuile : `débit = météo × water_factor`).
    pub fn build_hydro_at(&mut self, x: u16, y: u16) -> Result<(), BuildError> {
        use crate::physics::HydroKind;
        // Micro-hydro de village (~27 kW) : reste abordable à l'échelle colonie.
        let h = HydroTurbine::new(HydroKind::Kaplan, 1.0, 3.0);
        let c = capex_hydro(&h);
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        let env = self.claim_water_tile(x, y)?;
        self.economy.budget_eur -= c;
        self.park.hydro.push(Placed { x, y, env, asset: h });
        Ok(())
    }

    /// Pose un groupe électrogène domestique sur une tuile constructible.
    pub fn build_genset_at(&mut self, x: u16, y: u16) -> Result<(), BuildError> {
        let t = ThermalPlant::genset();
        let c = capex_thermal(&t);
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        let env = self.claim_land_tile(x, y)?;
        self.economy.budget_eur -= c;
        self.park.thermal.push(Placed { x, y, env, asset: t });
        Ok(())
    }

    /// Pose une batterie (kWh) sur une tuile constructible. La capacité est
    /// agrégée au stock mutualisé ; la tuile sert au rendu.
    pub fn build_battery_at(&mut self, x: u16, y: u16, capacity_kwh: f64) -> Result<(), BuildError> {
        let c = capacity_kwh * capex_battery_per_kwh();
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        // La batterie n'a pas besoin du terrain : env ignoré, mais la tuile doit
        // être constructible et libre.
        let _env = self.claim_land_tile(x, y)?;
        self.economy.budget_eur -= c;
        match &mut self.park.battery {
            Some(b) => {
                b.capacity_kwh += capacity_kwh;
                b.max_charge_kw += capacity_kwh * 0.5;
                b.max_discharge_kw += capacity_kwh * 0.5;
            }
            None => self.park.battery = Some(Battery::new(capacity_kwh)),
        }
        self.park.battery_tiles.push((x, y));
        Ok(())
    }

    /// Construit un bâtiment sur une tuile constructible (débite le CAPEX).
    /// Renvoie l'id du bâtiment.
    pub fn build_building_at(
        &mut self,
        x: u16,
        y: u16,
        kind: BuildingKind,
    ) -> Result<u32, BuildError> {
        let c = capex_building(kind);
        if self.economy.budget_eur < c {
            return Err(BuildError::Budget);
        }
        let _env = self.claim_land_tile(x, y)?;
        self.economy.budget_eur -= c;
        let id = self.add_building(kind);
        if let Some(b) = self.building_mut(id) {
            b.place(x, y);
        }
        Ok(id)
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

    /// Une tuile est-elle occupée par un élément déjà posé ?
    pub fn is_occupied(&self, x: u16, y: u16) -> bool {
        if !self.map.in_bounds(x, y) {
            return false;
        }
        self.occupied.get(self.map.idx(x, y)).copied().unwrap_or(false)
    }

    /// Place quelques foyers de départ (gratuits) sur des tuiles constructibles
    /// proches du centre de la carte. Suppose la carte déjà générée ; sans effet
    /// si la carte est vide.
    pub fn place_starter_village(&mut self) {
        if self.map.is_empty() {
            return;
        }
        let kinds = [BuildingKind::Family, BuildingKind::Elders];
        let mut placed = 0usize;
        let cx = (self.map.width / 2) as i32;
        let cy = (self.map.height / 2) as i32;
        let max_r = self.map.width.max(self.map.height) as i32;
        // Recherche en anneaux croissants autour du centre.
        'outer: for r in 0..max_r {
            for dy in -r..=r {
                for dx in -r..=r {
                    // Ne considère que le périmètre de l'anneau courant.
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let x = cx + dx;
                    let y = cy + dy;
                    if x < 0 || y < 0 || x >= self.map.width as i32 || y >= self.map.height as i32 {
                        continue;
                    }
                    let (x, y) = (x as u16, y as u16);
                    let i = self.map.idx(x, y);
                    if self.map.tiles[i].buildable() && !self.occupied[i] {
                        self.occupied[i] = true;
                        let id = self.add_building(kinds[placed]);
                        if let Some(b) = self.building_mut(id) {
                            b.place(x, y);
                        }
                        placed += 1;
                        if placed >= kinds.len() {
                            break 'outer;
                        }
                    }
                }
            }
        }
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
        //    Chaque actif utilise la météo **modulée par sa tuile** (terrain) :
        //    crête ventée, versant ombragé, gros débit de rivière, etc.
        let wind_kw: f64 = self.park.wind.iter()
            .map(|p| p.asset.power_kw(weather.wind_ms * p.env.wind_factor)).sum();
        let solar_kw: f64 = self.park.solar.iter()
            .map(|p| p.asset.power_kw(weather.irradiance_kw_m2 * p.env.solar_factor, weather.air_temp_c)).sum();
        let hydro_kw: f64 = self.park.hydro.iter()
            .map(|p| p.asset.power_kw(weather.river_flow_m3s * p.env.water_factor)).sum();
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
                let plant = &plant.asset;
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
        // Revenu instantané (€/jour) : salaires/pensions pondérés par le confort.
        let mut revenue_eur_day = 0.0;
        let mut building_reports = Vec::with_capacity(self.buildings.len());
        for b in &mut self.buildings {
            for r in &mut b.residents {
                r.update_comfort(hour_of_step, blackout, dt_h);
                revenue_eur_day += r.profile.income_eur_per_day() * (r.comfort / 100.0);
            }
            comfort_sum += b.residents.iter().map(|r| r.comfort).sum::<f64>();
            population += b.residents.len() as u32;
            building_reports.push(BuildingReport {
                id: b.id,
                name: b.name.clone(),
                kind: b.kind.label().to_string(),
                x: b.x,
                y: b.y,
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

        // 3b. Revenu du pas : crédite le budget (principale rentrée d'argent).
        self.economy.budget_eur += revenue_eur_day * (dt_h / 24.0);

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
            revenue_eur_day,
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

    // --- Couche spatiale (carte + placement) ---

    /// Trouve une tuile constructible et une tuile d'eau sur une petite carte.
    fn find_tiles(s: &SimState) -> (Option<(u16, u16)>, Option<(u16, u16)>) {
        let mut land = None;
        let mut water = None;
        for y in 0..s.map.height {
            for x in 0..s.map.width {
                let t = s.map.get(x, y).unwrap();
                if land.is_none() && t.buildable() {
                    land = Some((x, y));
                }
                if water.is_none() && t.is_water() {
                    water = Some((x, y));
                }
            }
        }
        (land, water)
    }

    #[test]
    fn placement_respects_terrain_and_occupancy() {
        let mut s = SimState::new(10_000_000.0);
        s.generate_map_sized(42, 120, 120);
        let (land, water) = find_tiles(&s);
        let (lx, ly) = land.expect("de la terre constructible existe");
        let (wx, wy) = water.expect("une rivière existe");

        // Solaire sur terre : OK ; rejouer sur la même tuile : occupée.
        assert!(s.build_solar_at(lx, ly, 6.0).is_ok());
        assert_eq!(s.build_solar_at(lx, ly, 6.0), Err(BuildError::Occupied));

        // Hydro sur rivière : OK.
        assert!(s.build_hydro_at(wx, wy).is_ok());

        // Hors carte : rejeté.
        assert_eq!(s.build_solar_at(9999, 9999, 6.0), Err(BuildError::OutOfBounds));
    }

    #[test]
    fn hydro_only_on_water() {
        let mut s = SimState::new(10_000_000.0);
        s.generate_map_sized(7, 120, 120);
        let (land, water) = find_tiles(&s);
        let (lx, ly) = land.expect("terre");
        assert_eq!(s.build_hydro_at(lx, ly), Err(BuildError::BadTerrain));
        if let Some((wx, wy)) = water {
            assert!(s.build_hydro_at(wx, wy).is_ok());
        }
    }

    #[test]
    fn terrain_modulates_production() {
        // Même turbine, deux tuiles de vent différent -> productions différentes.
        let mut s = SimState::new(10_000_000.0);
        s.generate_map_sized(3, 120, 120);
        // Cherche deux tuiles constructibles aux wind_factor nettement distincts.
        let mut tiles: Vec<(u16, u16, f32)> = Vec::new();
        for y in 0..s.map.height {
            for x in 0..s.map.width {
                let t = s.map.get(x, y).unwrap();
                if t.buildable() {
                    tiles.push((x, y, t.wind_factor));
                }
            }
        }
        tiles.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
        let (lowx, lowy, lowf) = tiles.first().copied().unwrap();
        let (hix, hiy, hif) = tiles.last().copied().unwrap();
        assert!(hif > lowf, "il faut deux tuiles de vent différent");
        s.build_wind_at(lowx, lowy).unwrap();
        s.build_wind_at(hix, hiy).unwrap();
        // Vent modéré pour rester dans la rampe (pas plafonné).
        let w = weather(7.0, 0.0);
        // Production de chaque éolienne via l'env capturé.
        let p_low = s.park.wind[0].asset.power_kw(w.wind_ms * s.park.wind[0].env.wind_factor);
        let p_hi = s.park.wind[1].asset.power_kw(w.wind_ms * s.park.wind[1].env.wind_factor);
        assert!(p_hi >= p_low, "tuile plus ventée -> au moins autant de prod");
    }

    #[test]
    fn working_residents_generate_revenue() {
        // Un village bien alimenté gagne de l'argent (salaires/pensions).
        let mut s = SimState::new(50_000.0);
        s.add_building(BuildingKind::Family); // un actif -> revenu
        s.build_battery(50.0); // de quoi éviter tout black-out
        s.build_wind(WindTurbine::onshore_2mw());
        let r = s.tick(&weather(13.0, 0.0), 0.5);
        assert!(r.revenue_eur_day > 0.0, "des habitants actifs rapportent un revenu");
        assert!(!r.blackout);
        assert!(r.cash_flow_eur > 0.0, "village alimenté -> trésorerie positive");
    }

    #[test]
    fn blackout_cuts_revenue() {
        // À confort égalisé, un black-out prolongé réduit le revenu (confort bas).
        let mut happy = SimState::new(50_000.0);
        happy.add_building(BuildingKind::Family);
        let mut sad = SimState::new(50_000.0);
        sad.economy.grid.connected = false;
        sad.add_building(BuildingKind::Family);
        sad.hour = 12.0;
        happy.hour = 12.0;
        // Plusieurs pas de black-out font chuter le confort -> le revenu baisse.
        let mut last_sad = 0.0;
        let mut last_happy = 0.0;
        for _ in 0..20 {
            last_happy = happy.tick(&weather(0.0, 0.0), 1.0).revenue_eur_day;
            last_sad = sad.tick(&weather(0.0, 0.0), 1.0).revenue_eur_day;
        }
        // Le village "happy" n'a pas de prod non plus ici, mais ses habitants ne
        // subissent pas de black-out tant qu'il reste raccordé au réseau.
        assert!(last_happy > last_sad, "le black-out réduit le revenu ({last_happy} > {last_sad})");
    }

    #[test]
    fn off_map_build_still_works() {
        // Les constructeurs non spatiaux fonctionnent sans carte (env neutre).
        let mut s = SimState::new(10_000_000.0);
        s.build_wind(WindTurbine::onshore_2mw());
        let r = s.tick(&weather(13.0, 0.0), 1.0);
        assert!(r.wind_kw > 0.0);
    }
}
