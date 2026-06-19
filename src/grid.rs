//! Réseau énergétique **multi-couches** : une hiérarchie de nœuds calquée sur le
//! réseau réel (maison → poste de quartier → réseau national). Le même type de
//! nœud ([`GridNode`]) s'emboîte à toutes les échelles : chaque nœud fait le
//! **même équilibrage local**, puis échange son **résidu** (surplus ou déficit)
//! avec son parent à travers une connexion ([`Link`]) qui a une capacité, des
//! pertes et un prix.
//!
//! L'arbre vit dans une **arène** (`Vec<GridNode>` + indices `NodeId`) : plus
//! simple pour le borrow-checker pendant les deux passes, et sérialisable.
//!
//! ## Dispatch en deux passes (par tick)
//! Pour chaque nœud, `balance(node) -> residual_kwh` (signé : `+` surplus à
//! remonter, `−` déficit à couvrir) :
//! 1. **Équilibrage local** : renouvelable + batterie locale (`Park::balance_local`).
//! 2. **Descente récursive** : on équilibre d'abord chaque enfant.
//! 3. **Troc P2P** entre enfants : on apparie surplus et déficits des voisins au
//!    niveau du parent (le micro-réseau de quartier), avec règlement à un prix local.
//! 4. **Échange par `Link`** : le résidu de chaque enfant remonte (pertes +
//!    capacité + règlement monétaire à contre-sens de l'énergie).
//! 5. **Couverture** : si le pool agrégé est en déficit, le nœud lance ses
//!    centrales pilotables (`Park::cover_with_thermal`).
//! 6. **Résidu** : ce qui reste monte au parent via l'`uplink`. La racine couvre
//!    son déficit (black-out si elle n'y arrive pas) ou écrête son surplus.
//!
//! Tout est **déterministe** : itération en ordre d'indices, aucune `HashMap`.

use serde::{Deserialize, Serialize};

use crate::physics::{FuelKind, ThermalPlant};
use crate::sim::Park;
use crate::weather::{Rng, Weather};

/// Identifiant d'un nœud dans l'arène (indice dans `Grid::nodes`).
pub type NodeId = u32;

const EPS: f64 = 1e-6;

/// Heures par an (pour annualiser les économies d'auto-production).
const HOURS_PER_YEAR: f64 = 8760.0;
/// Facteur de charge solaire moyen (France ~13 %) pour estimer le productible.
const SOLAR_CF: f64 = 0.13;
/// Facteur de charge éolien domestique moyen (~20 %).
const WIND_CF: f64 = 0.20;
/// Premier palier de toiture solaire installé par un foyer NPC (kWc).
const NPC_SOLAR_KWC: f64 = 6.0;
/// Palier de batterie domestique (kWh).
const NPC_BATTERY_KWH: f64 = 10.0;
/// Horizon de payback de base accepté par un foyer (années), avant l'effet
/// d'`autonomy_pref` qui le rend plus patient/agressif.
const NPC_PAYBACK_YEARS: f64 = 8.0;

/// Échelle d'un nœud du réseau.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// Réseau national : la racine, grosses centrales pilotables, transport.
    National,
    /// Poste de quartier : actifs partagés locaux, micro-réseau.
    District,
    /// Maison : un foyer (charge) + prod individuelle.
    Household,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::National => "National",
            Tier::District => "Quartier",
            Tier::Household => "Maison",
        }
    }
}

/// Connexion d'un nœud vers son parent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    /// Limite de transit (kW) : la ligne sature au-delà.
    pub capacity_kw: f64,
    /// Pertes en ligne, 0.0..~0.1 (croît avec la distance).
    pub loss_factor: f64,
    /// Prix payé par l'enfant pour **importer** (€/kWh).
    pub import_price_eur_kwh: f64,
    /// Prix reçu par l'enfant pour son **surplus** exporté (€/kWh).
    pub export_price_eur_kwh: f64,
    /// `false` = îloté (déconnecté du parent).
    pub connected: bool,
}

impl Link {
    /// Crée une connexion raccordée (`connected = true`).
    pub fn new(capacity_kw: f64, loss_factor: f64, import_price_eur_kwh: f64, export_price_eur_kwh: f64) -> Self {
        Self {
            capacity_kw,
            loss_factor: loss_factor.clamp(0.0, 0.99),
            import_price_eur_kwh,
            export_price_eur_kwh,
            connected: true,
        }
    }
}

/// Portefeuille propre à chaque nœud. L'argent circule **à contre-sens** de
/// l'énergie : qui importe paie, qui exporte touche.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wallet {
    pub balance_eur: f64,
    /// OPEX lignes + centrales : draine le portefeuille même sans vente.
    pub fixed_cost_eur_per_day: f64,
}

impl Wallet {
    pub fn new(balance_eur: f64) -> Self {
        Self { balance_eur, fixed_cost_eur_per_day: 0.0 }
    }
}

/// Un nœud du réseau, à n'importe quelle échelle (cf. [`Tier`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GridNode {
    pub id: NodeId,
    pub tier: Tier,
    pub name: String,
    /// Parc local (prod + batterie), réutilise le `Park` du jeu mono-carte.
    pub park: Park,
    /// Charge locale (kW), agrégée depuis les bâtiments/appareils à terme.
    pub load_kw: f64,
    /// 0 = « branche-moi au réseau », 1 = « je veux l'autonomie ». Réduit la part
    /// d'import qu'un nœud accepte (il préfère risquer la coupure que dépendre).
    pub autonomy_pref: f64,
    /// Revenu propre du nœud (€/jour) : pour une maison, la somme des
    /// salaires/pensions de ses résidents. Crédité au portefeuille à chaque pas
    /// et finance l'auto-investissement des foyers NPC.
    pub income_eur_per_day: f64,
    pub wallet: Wallet,
    /// Connexion vers le parent. `None` pour la racine.
    pub uplink: Option<Link>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Météo locale (corrélée au parent, légèrement décorrélée pour le foisonnement).
    pub weather: Weather,
}

impl GridNode {
    fn new(id: NodeId, tier: Tier, name: impl Into<String>) -> Self {
        Self {
            id,
            tier,
            name: name.into(),
            park: Park::default(),
            load_kw: 0.0,
            autonomy_pref: 0.0,
            income_eur_per_day: 0.0,
            wallet: Wallet::new(0.0),
            uplink: None,
            parent: None,
            children: Vec::new(),
            weather: Weather::default(),
        }
    }

    /// Le nœud est-il îloté (déconnecté de son parent) ? La racine ne l'est jamais.
    pub fn islanded(&self) -> bool {
        self.uplink.as_ref().map(|l| !l.connected).unwrap_or(false)
    }
}

/// Bilan d'un nœud sur un pas de temps (analogue à `TickReport`, par nœud).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeReport {
    pub id: NodeId,
    pub tier_label: String,
    pub name: String,
    pub wind_kw: f64,
    pub solar_kw: f64,
    pub hydro_kw: f64,
    pub thermal_kw: f64,
    /// Puissance batterie : positive en décharge, négative en charge.
    pub battery_kw: f64,
    pub load_kw: f64,
    /// Énergie **reçue** du parent (kW moyens) sur le pas.
    pub import_kw: f64,
    /// Énergie **livrée** au parent (kW moyens) sur le pas.
    pub export_kw: f64,
    /// Énergie troquée en P2P avec les voisins (kW moyens, valeur absolue).
    pub p2p_kw: f64,
    /// Déficit non fourni (kW moyens) → black-out local.
    pub unmet_kw: f64,
    pub blackout: bool,
    /// Surplus écrêté (curtailment) faute de débouché (kW moyens).
    pub curtailed_kw: f64,
    pub soc_pct: f64,
    pub co2_kg_step: f64,
    /// Variation du portefeuille sur le pas (€).
    pub cash_flow_eur: f64,
    pub balance_eur: f64,
    pub islanded: bool,
}

impl NodeReport {
    fn blank(n: &GridNode) -> Self {
        Self {
            id: n.id,
            tier_label: n.tier.label().to_string(),
            name: n.name.clone(),
            islanded: n.islanded(),
            ..Default::default()
        }
    }
}

/// Indicateurs agrégés du réseau pour l'UI : ils rendent la **spirale de la
/// mort** visible. Calculés à partir des `NodeReport` d'un pas.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GridSummary {
    /// Marge nette du national sur le pas (€) : son cash-flow. Devient négative
    /// quand son revenu (marges import/export) ne couvre plus ses coûts fixes.
    pub national_margin_eur: f64,
    pub national_balance_eur: f64,
    /// Taux de dépendance au réseau (0..1) : part de la charge des maisons
    /// couverte par un **import** (depuis le quartier/national) plutôt qu'en
    /// propre ou en P2P. Il s'effondre quand les foyers s'autonomisent.
    pub dependency_rate: f64,
    pub total_load_kw: f64,
    pub total_import_kw: f64,
    /// Nombre de maisons et combien ont déjà de l'auto-production.
    pub households: u32,
    pub self_producing_households: u32,
}

/// L'arbre complet du réseau.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub nodes: Vec<GridNode>,
    /// La racine (le national).
    pub root: NodeId,
    pub tick_count: u64,
    /// Heures simulées cumulées (cadence des décisions journalières NPC).
    pub sim_hours: f64,
    /// Dernier jour où l'auto-investissement NPC a été évalué.
    last_invest_day: u32,
}

impl Grid {
    /// Crée un réseau réduit à sa racine nationale.
    pub fn new_national(name: impl Into<String>, starting_balance_eur: f64) -> Self {
        let mut root = GridNode::new(0, Tier::National, name);
        root.wallet = Wallet::new(starting_balance_eur);
        Self { nodes: vec![root], root: 0, tick_count: 0, sim_hours: 0.0, last_invest_day: 0 }
    }

    /// **Scénario de départ jouable** : 1 national → `n_districts` quartiers →
    /// `houses_per_district` maisons. Déterministe (seedé) : portefeuilles,
    /// revenus, charges et préférences d'autonomie des foyers varient légèrement
    /// d'une maison à l'autre. Le national a de grosses centrales pilotables et
    /// de lourds coûts fixes — terreau de la spirale.
    pub fn scenario(seed: u64, n_districts: usize, houses_per_district: usize) -> Self {
        let mut rng = Rng::new(seed.max(1));
        let mut g = Grid::new_national("Réseau national", 5_000_000.0);
        {
            let nat = g.node_mut(0);
            nat.wallet.fixed_cost_eur_per_day = 20_000.0;
            nat.park.add_thermal(ThermalPlant::new(FuelKind::GasCcgt, 200_000.0));
        }
        // Tarif national initial (modifiable par le joueur via set_national_tariff).
        let import0 = 0.20;
        let export0 = 0.10;
        for d in 0..n_districts {
            let dlink = Link::new(5_000.0, 0.04, import0, export0);
            let did = g.add_child(0, Tier::District, format!("Quartier {}", d + 1), dlink);
            {
                let dn = g.node_mut(did);
                dn.wallet = Wallet::new(100_000.0);
                dn.wallet.fixed_cost_eur_per_day = 800.0;
            }
            for h in 0..houses_per_district {
                let hlink = Link::new(30.0, 0.06, import0, export0);
                let hid = g.add_child(did, Tier::Household, format!("Maison {}-{}", d + 1, h + 1), hlink);
                let node = g.node_mut(hid);
                node.wallet = Wallet::new(8_000.0 + rng.next_f64() * 4_000.0);
                node.income_eur_per_day = 70.0 + rng.next_f64() * 60.0;
                node.load_kw = 2.0 + rng.next_f64() * 2.0;
                node.autonomy_pref = rng.next_f64() * 0.6;
            }
        }
        g
    }

    /// Ajoute un enfant sous `parent`, raccordé par `link`. Renvoie son `NodeId`.
    pub fn add_child(&mut self, parent: NodeId, tier: Tier, name: impl Into<String>, link: Link) -> NodeId {
        let id = self.nodes.len() as NodeId;
        let mut n = GridNode::new(id, tier, name);
        n.parent = Some(parent);
        n.uplink = Some(link);
        self.nodes.push(n);
        self.nodes[parent as usize].children.push(id);
        id
    }

    pub fn node(&self, id: NodeId) -> &GridNode {
        &self.nodes[id as usize]
    }
    pub fn node_mut(&mut self, id: NodeId) -> &mut GridNode {
        &mut self.nodes[id as usize]
    }

    /// Îlote (ou reconnecte) un nœud : bascule `uplink.connected`. Sans effet sur
    /// la racine (pas d'uplink).
    pub fn set_islanded(&mut self, id: NodeId, islanded: bool) {
        if let Some(l) = &mut self.nodes[id as usize].uplink {
            l.connected = !islanded;
        }
    }

    /// Règle le **tarif national** : prix d'import/export sur les liens des
    /// enfants directs de la racine (les quartiers raccordés au national).
    pub fn set_national_tariff(&mut self, import_price_eur_kwh: f64, export_price_eur_kwh: f64) {
        let children = self.nodes[self.root as usize].children.clone();
        for c in children {
            if let Some(l) = &mut self.nodes[c as usize].uplink {
                l.import_price_eur_kwh = import_price_eur_kwh;
                l.export_price_eur_kwh = export_price_eur_kwh;
            }
        }
    }

    fn is_islanded(&self, id: NodeId) -> bool {
        self.nodes[id as usize].islanded()
    }

    /// Calcule les [indicateurs de spirale](GridSummary) à partir des rapports
    /// d'un pas. Le `dependency_rate` agrège les maisons : import / charge.
    pub fn summary(&self, reports: &[NodeReport]) -> GridSummary {
        let mut s = GridSummary {
            national_margin_eur: reports[self.root as usize].cash_flow_eur,
            national_balance_eur: reports[self.root as usize].balance_eur,
            ..Default::default()
        };
        for (n, r) in self.nodes.iter().zip(reports.iter()) {
            if n.tier != Tier::Household {
                continue;
            }
            s.households += 1;
            s.total_load_kw += r.load_kw;
            s.total_import_kw += r.import_kw;
            let self_equipped = !n.park.solar.is_empty()
                || !n.park.wind.is_empty()
                || n.park.battery.is_some();
            if self_equipped {
                s.self_producing_households += 1;
            }
        }
        s.dependency_rate = if s.total_load_kw > EPS {
            (s.total_import_kw / s.total_load_kw).clamp(0.0, 1.0)
        } else {
            0.0
        };
        s
    }

    /// Avance le réseau d'un pas `dt_h` : équilibre tout l'arbre en deux passes et
    /// renvoie un [`NodeReport`] par nœud (indexé par `NodeId`).
    pub fn tick(&mut self, dt_h: f64) -> Vec<NodeReport> {
        let inv_dt = if dt_h > 0.0 { 1.0 / dt_h } else { 0.0 };
        let mut reports: Vec<NodeReport> = self.nodes.iter().map(NodeReport::blank).collect();
        let before: Vec<f64> = self.nodes.iter().map(|n| n.wallet.balance_eur).collect();

        let root = self.root;
        let root_residual = self.balance(root, dt_h, inv_dt, &mut reports);

        // La racine n'a pas de parent : son déficit résiduel est un black-out
        // national, son surplus résiduel est écrêté.
        {
            let r = &mut reports[root as usize];
            if root_residual < -EPS {
                r.unmet_kw += (-root_residual) * inv_dt;
                r.blackout = true;
            } else if root_residual > EPS {
                r.curtailed_kw += root_residual * inv_dt;
            }
        }

        // Coûts fixes (OPEX lignes + centrales) et revenus propres (salaires des
        // résidents) de chaque nœud, prorata du pas. Les coûts fixes drainent le
        // portefeuille même sans vente : c'est le moteur de la « spirale ».
        let day_frac = dt_h / 24.0;
        for n in &mut self.nodes {
            n.wallet.balance_eur -= n.wallet.fixed_cost_eur_per_day * day_frac;
            n.wallet.balance_eur += n.income_eur_per_day * day_frac;
        }

        // Finalise cash-flow, solde et état de charge par nœud (avant les
        // décisions d'investissement, qui relèvent du CAPEX, pas de l'OPEX).
        for (i, n) in self.nodes.iter().enumerate() {
            reports[i].cash_flow_eur = n.wallet.balance_eur - before[i];
            reports[i].balance_eur = n.wallet.balance_eur;
            reports[i].soc_pct = n.park.battery.as_ref().map(|b| b.soc_pct()).unwrap_or(0.0);
        }

        // Horloge + décision d'auto-investissement NPC à cadence journalière.
        self.sim_hours += dt_h;
        let day = (self.sim_hours / 24.0).floor() as u32;
        if day != self.last_invest_day {
            self.last_invest_day = day;
            self.npc_invest_step();
        }

        self.tick_count += 1;
        reports
    }

    /// Décision d'auto-investissement des **foyers NPC** (cadence journalière).
    /// Parcourt les maisons en ordre d'indice (déterministe) et tente le prochain
    /// palier d'équipement de chacune.
    fn npc_invest_step(&mut self) {
        for i in 0..self.nodes.len() {
            if self.nodes[i].tier == Tier::Household {
                self.try_invest(i as NodeId);
            }
        }
    }

    /// Heuristique d'auto-investissement d'un foyer : il estime les imports
    /// **évités** au tarif courant si l'on ajoutait le prochain palier (toiture
    /// solaire → batterie → micro-éolienne) et investit si
    /// `économies_annualisées × horizon_payback ≥ coût` **et** `solde ≥ coût`.
    /// `autonomy_pref` allonge l'horizon accepté → décrochage plus agressif.
    /// Renvoie `true` si un palier a été acheté.
    fn try_invest(&mut self, id: NodeId) -> bool {
        use crate::economy::capex_wind;
        use crate::physics::WindTurbine;

        let node = &self.nodes[id as usize];
        let import_price = node.uplink.as_ref().map(|l| l.import_price_eur_kwh).unwrap_or(0.0);
        let autonomy = node.autonomy_pref.clamp(0.0, 1.0);
        let balance = node.wallet.balance_eur;
        let solar_kwc: f64 = node.park.solar.iter().map(|p| p.asset.kwc).sum();
        let has_battery = node.park.battery.is_some();
        let has_wind = !node.park.wind.is_empty();
        let payback_target = NPC_PAYBACK_YEARS * (1.0 + autonomy);

        // Économies annuelles d'un productible (kWh/an) au tarif d'import courant.
        let savings = |annual_kwh: f64| annual_kwh * import_price;
        let worth_it = |cost: f64, annual_kwh: f64| {
            balance >= cost && savings(annual_kwh) * payback_target >= cost
        };

        // Palier 1 : toiture solaire (tant qu'on n'en a pas).
        if solar_kwc < NPC_SOLAR_KWC - EPS {
            let cost = NPC_SOLAR_KWC * 1100.0;
            let annual = NPC_SOLAR_KWC * SOLAR_CF * HOURS_PER_YEAR;
            return worth_it(cost, annual) && self.build_solar(id, NPC_SOLAR_KWC);
        }
        // Palier 2 : batterie domestique (auto-consommation du surplus solaire).
        if !has_battery {
            let cost = NPC_BATTERY_KWH * 600.0;
            // ~1 cycle/jour de la part utile (80 %) de la capacité.
            let annual = NPC_BATTERY_KWH * 0.8 * 300.0;
            return worth_it(cost, annual) && self.build_battery(id, NPC_BATTERY_KWH);
        }
        // Palier 3 : micro-éolienne de jardin.
        if !has_wind {
            let t = WindTurbine::micro();
            let cost = capex_wind(&t);
            let annual = t.rated_kw * WIND_CF * HOURS_PER_YEAR;
            return worth_it(cost, annual) && self.build_wind_micro(id);
        }
        false
    }

    /// **Foisonnement** : propage une météo de base à tous les nœuds en y
    /// ajoutant un **bruit déterministe décorrélé** par nœud (seedé sur l'id et
    /// le tick). Les variances locales étant indépendantes, un parent qui agrège
    /// de nombreux enfants voit un résidu plus lisse → une maison isolée subit
    /// toute la variance, le national la moyenne. `amplitude` ∈ 0..1 module
    /// l'écart relatif appliqué au vent et à l'irradiance.
    pub fn propagate_weather(&mut self, base: Weather, amplitude: f64) {
        let amp = amplitude.clamp(0.0, 1.0);
        let tick = self.tick_count;
        for n in &mut self.nodes {
            // Graine stable par (nœud, tick) : reproductible, sans HashMap.
            let seed = (n.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ tick.wrapping_add(1);
            let mut rng = Rng::new(seed.max(1));
            // Bruit centré dans [-amp, +amp].
            let nw = (rng.next_f64() - 0.5) * 2.0 * amp;
            let ni = (rng.next_f64() - 0.5) * 2.0 * amp;
            let mut w = base;
            w.wind_ms = (base.wind_ms * (1.0 + nw)).max(0.0);
            w.irradiance_kw_m2 = (base.irradiance_kw_m2 * (1.0 + ni)).max(0.0);
            n.weather = w;
        }
    }

    /// Équilibre récursivement le sous-arbre enraciné en `id` et renvoie son
    /// **résidu signé** (kWh sur le pas) : `+` surplus à remonter, `−` déficit
    /// non couvert.
    fn balance(&mut self, id: NodeId, dt_h: f64, inv_dt: f64, reports: &mut Vec<NodeReport>) -> f64 {
        // --- 1. Équilibrage local : renouvelable + batterie (pas de centrales). ---
        let local_residual = {
            let node = &mut self.nodes[id as usize];
            let disp = node.park.balance_local(node.load_kw, &node.weather, dt_h, |_, _| 0.0);
            let r = &mut reports[id as usize];
            r.wind_kw = disp.wind_kw;
            r.solar_kw = disp.solar_kw;
            r.hydro_kw = disp.hydro_kw;
            r.battery_kw = disp.battery_kwh * inv_dt;
            r.load_kw = node.load_kw;
            disp.residual_kwh
        };

        // --- 2. Descente récursive : équilibre chaque enfant. ---
        let children = self.nodes[id as usize].children.clone();
        let mut child_res: Vec<(NodeId, f64)> = Vec::with_capacity(children.len());
        for &c in &children {
            let r = self.balance(c, dt_h, inv_dt, reports);
            child_res.push((c, r));
        }

        // --- 3. Troc P2P entre enfants (micro-réseau local). ---
        self.settle_p2p(&mut child_res, inv_dt, reports);

        // --- 4. Échange du résidu de chaque enfant via son uplink. ---
        let mut pool = local_residual;
        for &(c, r) in &child_res {
            pool += self.exchange_link(id, c, r, dt_h, inv_dt, reports);
        }

        // --- 5. Couverture du déficit agrégé par les centrales du nœud. ---
        if pool < -EPS {
            let cover = {
                let node = &mut self.nodes[id as usize];
                node.park.cover_with_thermal(-pool, dt_h, |_, _| 0.0)
            };
            self.nodes[id as usize].wallet.balance_eur -= cover.fuel_cost_eur;
            let r = &mut reports[id as usize];
            r.thermal_kw = cover.thermal_kwh * inv_dt;
            r.co2_kg_step += cover.co2_kg;
            pool += cover.delivered_kwh;
        }

        // --- 6. Résidu remonté au parent. ---
        pool
    }

    /// **Troc P2P** : apparie surplus et déficits des enfants entre eux, en ordre
    /// d'indice (déterministe), avec règlement à un prix local (entre export et
    /// import). Met à jour les résidus dans `child_res`. Les enfants îlotés ne
    /// participent pas (ils sont coupés du bus local).
    fn settle_p2p(&mut self, child_res: &mut [(NodeId, f64)], inv_dt: f64, reports: &mut Vec<NodeReport>) {
        let len = child_res.len();
        for i in 0..len {
            if self.is_islanded(child_res[i].0) {
                continue;
            }
            for j in 0..len {
                if i == j {
                    continue;
                }
                let surplus = child_res[i].1;
                if surplus <= EPS {
                    break; // l'enfant i n'a plus rien à partager
                }
                let deficit = -child_res[j].1;
                if deficit <= EPS || self.is_islanded(child_res[j].0) {
                    continue;
                }
                let transfer = surplus.min(deficit); // pertes locales négligées
                if transfer <= EPS {
                    continue;
                }
                let sid = child_res[i].0;
                let did = child_res[j].0;
                // Prix local entre l'export du vendeur et l'import de l'acheteur :
                // tout le monde y gagne par rapport au passage par le national.
                let sell = self.nodes[sid as usize].uplink.as_ref().map(|l| l.export_price_eur_kwh).unwrap_or(0.0);
                let buy = self.nodes[did as usize].uplink.as_ref().map(|l| l.import_price_eur_kwh).unwrap_or(0.0);
                let local_price = 0.5 * (sell + buy);
                // Règlement : l'acheteur paie, le vendeur reçoit.
                self.nodes[did as usize].wallet.balance_eur -= local_price * transfer;
                self.nodes[sid as usize].wallet.balance_eur += local_price * transfer;
                child_res[i].1 -= transfer;
                child_res[j].1 += transfer;
                reports[sid as usize].p2p_kw += transfer * inv_dt;
                reports[did as usize].p2p_kw += transfer * inv_dt;
            }
        }
    }

    /// Échange le résidu d'un enfant à travers son `uplink` avec le parent
    /// (`parent_id`). Renvoie la contribution **signée** au pool du parent :
    /// `+` énergie reçue de l'enfant (surplus remonté), `−` énergie que le parent
    /// doit fournir (import de l'enfant). Met à jour wallets et rapports.
    fn exchange_link(
        &mut self,
        parent_id: NodeId,
        child_id: NodeId,
        residual: f64,
        dt_h: f64,
        inv_dt: f64,
        reports: &mut Vec<NodeReport>,
    ) -> f64 {
        let (connected, cap_kwh, loss, import_price, export_price, autonomy) = {
            let child = &self.nodes[child_id as usize];
            match &child.uplink {
                Some(l) => (
                    l.connected,
                    l.capacity_kw * dt_h,
                    l.loss_factor.clamp(0.0, 0.99),
                    l.import_price_eur_kwh,
                    l.export_price_eur_kwh,
                    child.autonomy_pref.clamp(0.0, 1.0),
                ),
                None => return 0.0, // pas d'uplink : ne devrait pas arriver hors racine
            }
        };

        // Îloté : l'enfant se débrouille seul (résilience modélisée).
        if !connected {
            let r = &mut reports[child_id as usize];
            if residual < -EPS {
                r.unmet_kw += (-residual) * inv_dt;
                r.blackout = true;
            } else if residual > EPS {
                r.curtailed_kw += residual * inv_dt;
            }
            return 0.0;
        }

        if residual > EPS {
            // Surplus de l'enfant -> remonte au parent (pertes + capacité).
            let sent = residual.min(cap_kwh);
            let delivered = sent * (1.0 - loss);
            // Argent à contre-sens : le parent paie l'enfant pour son export.
            self.nodes[child_id as usize].wallet.balance_eur += export_price * delivered;
            self.nodes[parent_id as usize].wallet.balance_eur -= export_price * delivered;
            let curtail = residual - sent; // au-delà de la capacité -> écrêté
            let r = &mut reports[child_id as usize];
            r.export_kw += delivered * inv_dt;
            if curtail > EPS {
                r.curtailed_kw += curtail * inv_dt;
            }
            delivered
        } else if residual < -EPS {
            // Déficit de l'enfant -> le parent fournit (autonomie + capacité).
            let deficit = -residual;
            // L'autonomie réduit la part qu'on accepte d'importer.
            let want = deficit * (1.0 - autonomy);
            // Énergie à injecter par le parent pour livrer `want` après pertes,
            // bornée par la capacité de transit de la ligne.
            let sent = (want / (1.0 - loss)).min(cap_kwh);
            let received = sent * (1.0 - loss);
            // Argent à contre-sens : l'enfant paie le parent pour son import.
            self.nodes[child_id as usize].wallet.balance_eur -= import_price * received;
            self.nodes[parent_id as usize].wallet.balance_eur += import_price * received;
            let unmet = deficit - received;
            let r = &mut reports[child_id as usize];
            r.import_kw += received * inv_dt;
            if unmet > EPS {
                r.unmet_kw += unmet * inv_dt;
                r.blackout = true;
            }
            -sent
        } else {
            0.0
        }
    }

    // --- Construction d'actifs sur un nœud (renvoie un bool, pas de panique). ---

    /// Ajoute du solaire (kWc) au parc d'un nœud, débité de **son** portefeuille.
    /// Renvoie `false` si le solde est insuffisant.
    pub fn build_solar(&mut self, id: NodeId, kwc: f64) -> bool {
        use crate::economy::capex_solar;
        use crate::physics::SolarArray;
        let s = SolarArray::new(kwc);
        let c = capex_solar(&s);
        let node = &mut self.nodes[id as usize];
        if node.wallet.balance_eur < c {
            return false;
        }
        node.wallet.balance_eur -= c;
        node.park.add_solar(s);
        true
    }

    /// Ajoute une micro-éolienne au parc d'un nœud, débitée de son portefeuille.
    pub fn build_wind_micro(&mut self, id: NodeId) -> bool {
        use crate::economy::capex_wind;
        use crate::physics::WindTurbine;
        let t = WindTurbine::micro();
        let c = capex_wind(&t);
        let node = &mut self.nodes[id as usize];
        if node.wallet.balance_eur < c {
            return false;
        }
        node.wallet.balance_eur -= c;
        node.park.add_wind(t);
        true
    }

    /// Ajoute de la batterie (kWh) au parc d'un nœud, débitée de son portefeuille.
    pub fn build_battery(&mut self, id: NodeId, capacity_kwh: f64) -> bool {
        use crate::economy::capex_battery_per_kwh;
        let c = capacity_kwh * capex_battery_per_kwh();
        let node = &mut self.nodes[id as usize];
        if node.wallet.balance_eur < c {
            return false;
        }
        node.wallet.balance_eur -= c;
        node.park.add_battery(capacity_kwh);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{SolarArray, ThermalPlant, FuelKind};

    /// Météo plein soleil sans vent (pour piloter le solaire des tests).
    fn sunny() -> Weather {
        Weather { wind_ms: 0.0, irradiance_kw_m2: 1.0, air_temp_c: 20.0, river_flow_m3s: 0.0 }
    }
    fn dark() -> Weather {
        Weather { wind_ms: 0.0, irradiance_kw_m2: 0.0, air_temp_c: 15.0, river_flow_m3s: 0.0 }
    }

    /// Lien généreux (grosse capacité, faibles pertes) avec un tarif donné.
    fn link(import: f64, export: f64) -> Link {
        Link::new(1_000.0, 0.02, import, export)
    }

    /// Construit un national -> 1 quartier -> 2 maisons. Renvoie les ids.
    fn small_grid() -> (Grid, NodeId, NodeId, NodeId) {
        let mut g = Grid::new_national("National", 1_000_000.0);
        let district = g.add_child(g.root, Tier::District, "Quartier", link(0.20, 0.10));
        let h_a = g.add_child(district, Tier::Household, "Maison A", link(0.25, 0.12));
        let h_b = g.add_child(district, Tier::Household, "Maison B", link(0.25, 0.12));
        for id in [district, h_a, h_b] {
            g.node_mut(id).wallet = Wallet::new(10_000.0);
        }
        (g, district, h_a, h_b)
    }

    #[test]
    fn household_surplus_feeds_neighbour_via_p2p() {
        // Maison A : beaucoup de solaire, faible charge -> surplus.
        // Maison B : pas de prod, charge -> déficit. Le P2P doit les apparier
        // sans que le national ne fournisse l'essentiel.
        let (mut g, _district, h_a, h_b) = small_grid();
        g.node_mut(h_a).park.add_solar(SolarArray::new(20.0));
        g.node_mut(h_a).weather = sunny();
        g.node_mut(h_a).load_kw = 2.0;
        g.node_mut(h_b).weather = sunny();
        g.node_mut(h_b).load_kw = 5.0;

        let reports = g.tick(1.0);
        let ra = &reports[h_a as usize];
        let rb = &reports[h_b as usize];
        assert!(ra.p2p_kw > 0.0, "la maison A partage son surplus");
        assert!(rb.p2p_kw > 0.0, "la maison B reçoit de l'énergie locale");
        assert!(!rb.blackout, "le voisin couvre le déficit de B");
        // B est servie majoritairement en P2P, pas par un gros import national.
        assert!(rb.p2p_kw >= rb.import_kw, "le P2P prime sur l'import national");
    }

    #[test]
    fn islanded_district_survives_national_outage() {
        // Quartier autosuffisant (gros solaire + batterie), îloté : même si le
        // national « tombe », ses maisons restent alimentées.
        let (mut g, district, h_a, h_b) = small_grid();
        g.node_mut(district).park.add_solar(SolarArray::new(50.0));
        g.node_mut(district).park.add_battery(100.0);
        g.node_mut(district).weather = sunny();
        g.node_mut(h_a).load_kw = 4.0;
        g.node_mut(h_b).load_kw = 4.0;
        g.node_mut(h_a).weather = sunny();
        g.node_mut(h_b).weather = sunny();

        // Coupe le national : le national n'a aucune prod et une charge énorme.
        g.node_mut(g.root).load_kw = 1_000.0;
        g.set_islanded(district, true);

        let reports = g.tick(1.0);
        assert!(reports[district as usize].islanded, "le quartier est îloté");
        assert!(!reports[h_a as usize].blackout, "la maison A reste alimentée");
        assert!(!reports[h_b as usize].blackout, "la maison B reste alimentée");
        // Le national, lui, est en black-out (charge énorme, aucune prod) :
        // l'îlotage protège le quartier de cette panne.
        assert!(reports[g.root as usize].blackout, "le national est en panne");
    }

    #[test]
    fn money_flows_opposite_to_energy_through_link() {
        // Une maison déficitaire importe du quartier : son portefeuille baisse,
        // celui du parent monte d'autant (à contre-sens de l'énergie).
        let (mut g, district, h_a, _h_b) = small_grid();
        // Quartier producteur (solaire), maison A consommatrice pure.
        g.node_mut(district).park.add_solar(SolarArray::new(50.0));
        g.node_mut(district).weather = sunny();
        g.node_mut(h_a).load_kw = 6.0;
        g.node_mut(h_a).weather = dark();
        // Désactive la 2e maison pour isoler le flux A <-> quartier.
        // (charge nulle, pas de prod -> résidu nul, n'interfère pas)

        let wallet_a_before = g.node(h_a).wallet.balance_eur;
        let wallet_d_before = g.node(district).wallet.balance_eur;
        let reports = g.tick(1.0);

        let ra = &reports[h_a as usize];
        assert!(ra.import_kw > 0.0, "la maison A importe");
        let paid = wallet_a_before - g.node(h_a).wallet.balance_eur;
        let received = g.node(district).wallet.balance_eur - wallet_d_before;
        assert!(paid > 0.0, "A a payé son import");
        // Le quartier encaisse l'import de A (à la louche : il a aussi pu vendre
        // son surplus au national, donc on vérifie juste le sens et l'ordre).
        assert!(received > 0.0, "le quartier a encaissé de l'argent");
        // Le prix d'import payé par A correspond à received_kwh * import_price.
        let expected = ra.import_kw * 1.0 * 0.25; // dt=1h, import_price=0.25
        assert!((paid - expected).abs() < 1e-6, "paiement = import * tarif ({paid} ~ {expected})");
    }

    #[test]
    fn deterministic_same_seed_same_result() {
        let build = || {
            let (mut g, _d, h_a, h_b) = small_grid();
            g.node_mut(h_a).park.add_solar(SolarArray::new(15.0));
            g.node_mut(h_a).weather = sunny();
            g.node_mut(h_a).load_kw = 3.0;
            g.node_mut(h_b).load_kw = 7.0;
            g.node_mut(h_b).weather = sunny();
            g
        };
        let mut a = build();
        let mut b = build();
        for _ in 0..48 {
            let ra = a.tick(0.5);
            let rb = b.tick(0.5);
            for (x, y) in ra.iter().zip(rb.iter()) {
                assert_eq!(x.balance_eur.to_bits(), y.balance_eur.to_bits(), "soldes identiques");
                assert_eq!(x.import_kw.to_bits(), y.import_kw.to_bits(), "imports identiques");
                assert_eq!(x.p2p_kw.to_bits(), y.p2p_kw.to_bits(), "P2P identiques");
            }
        }
    }

    #[test]
    fn lone_national_behaves_like_single_map() {
        // Un national seul (sans enfants) avec une centrale couvre sa charge :
        // pas de black-out, du CO2 émis. C'est le cas dégénéré « mono-carte ».
        let mut g = Grid::new_national("National", 1_000_000.0);
        g.node_mut(g.root).park.add_thermal(ThermalPlant::new(FuelKind::Coal, 1000.0));
        g.node_mut(g.root).load_kw = 800.0;
        let reports = g.tick(1.0);
        let r = &reports[g.root as usize];
        assert!(!r.blackout, "le charbon couvre 800 kW < 1000 kW");
        assert!(r.thermal_kw > 0.0);
        assert!(r.co2_kg_step > 0.0, "le charbon émet du CO2");
    }

    #[test]
    fn autonomous_node_refuses_part_of_import() {
        // À déficit égal, un nœud autonome importe moins (et subit donc plus de
        // non-fourni) qu'un nœud dépendant.
        let make = |autonomy: f64| {
            let mut g = Grid::new_national("National", 1_000_000.0);
            // Le national a de quoi fournir tout le monde.
            g.node_mut(g.root).park.add_thermal(ThermalPlant::new(FuelKind::Coal, 10_000.0));
            let h = g.add_child(g.root, Tier::Household, "Maison", link(0.25, 0.12));
            g.node_mut(h).wallet = Wallet::new(10_000.0);
            g.node_mut(h).load_kw = 10.0;
            g.node_mut(h).weather = dark();
            g.node_mut(h).autonomy_pref = autonomy;
            (g, h)
        };
        let (mut dep, h1) = make(0.0);
        let (mut aut, h2) = make(0.8);
        let rd = dep.tick(1.0);
        let ra = aut.tick(1.0);
        assert!(rd[h1 as usize].import_kw > ra[h2 as usize].import_kw, "l'autonome importe moins");
        assert!(ra[h2 as usize].unmet_kw > 0.0, "l'autonome subit du non-fourni");
        assert!(!rd[h1 as usize].blackout, "le dépendant est servi");
    }

    // --- Phase 2 : économie, coûts fixes, NPC, spirale, foisonnement ---

    /// Météo ensoleillée constante (pour un solaire productif et déterministe).
    fn daylight() -> Weather {
        Weather { wind_ms: 0.0, irradiance_kw_m2: 0.6, air_temp_c: 20.0, river_flow_m3s: 0.0 }
    }

    /// National (gros charbon) → 1 quartier → `n` maisons consommatrices au tarif
    /// d'import `import_price`. Chaque maison a un revenu et de la trésorerie.
    fn spiral_grid(import_price: f64, n: usize) -> (Grid, Vec<NodeId>) {
        let mut g = Grid::new_national("National", 5_000_000.0);
        // De quoi alimenter tout le monde si personne ne s'autoproduit.
        g.node_mut(g.root).park.add_thermal(ThermalPlant::new(FuelKind::Coal, 100_000.0));
        let district = g.add_child(g.root, Tier::District, "Quartier", link(import_price, import_price * 0.5));
        g.node_mut(district).wallet = Wallet::new(50_000.0);
        let mut houses = Vec::new();
        for i in 0..n {
            let h = g.add_child(district, Tier::Household, format!("Maison {i}"), link(import_price, import_price * 0.5));
            let node = g.node_mut(h);
            node.wallet = Wallet::new(10_000.0);
            node.income_eur_per_day = 90.0;
            node.load_kw = 3.0;
            node.weather = daylight();
            houses.push(h);
        }
        (g, houses)
    }

    #[test]
    fn fixed_cost_drains_wallet() {
        // Un nœud aux coûts fixes, sans revenu ni vente, voit son solde fondre.
        let mut g = Grid::new_national("National", 10_000.0);
        g.node_mut(g.root).wallet.fixed_cost_eur_per_day = 1_200.0;
        let start = g.node(g.root).wallet.balance_eur;
        for _ in 0..48 {
            g.tick(1.0); // 48 h
        }
        let end = g.node(g.root).wallet.balance_eur;
        // 2 jours × 1200 €/j ≈ 2400 € prélevés.
        assert!((start - end - 2_400.0).abs() < 1.0, "les coûts fixes drainent ({start} -> {end})");
    }

    #[test]
    fn high_tariff_triggers_npc_investment() {
        // Tarif élevé : la maison investit dans le solaire (payback favorable).
        // Tarif bas : elle reste branchée au réseau, n'investit pas.
        let mut high = spiral_grid(0.30, 1).0;
        let mut low = spiral_grid(0.08, 1).0;
        for _ in 0..48 {
            // 2 jours -> au moins un passage de jour (décision NPC).
            high.tick(1.0);
            low.tick(1.0);
        }
        let high_solar = high.nodes.iter().any(|n| n.tier == Tier::Household && !n.park.solar.is_empty());
        let low_solar = low.nodes.iter().any(|n| n.tier == Tier::Household && !n.park.solar.is_empty());
        assert!(high_solar, "tarif élevé -> la maison s'équipe en solaire");
        assert!(!low_solar, "tarif bas -> la maison ne s'équipe pas");
    }

    #[test]
    fn death_spiral_lowers_dependency_and_national_revenue() {
        // Cœur de la tension politique : un tarif élevé pousse les foyers à
        // s'autonomiser, ce qui effondre la dépendance au réseau ET le revenu que
        // le national tire de leurs imports.
        let run = |import_price: f64| {
            let (mut g, _houses) = spiral_grid(import_price, 3);
            // Marge nationale au tout premier pas (avant tout investissement).
            let r0 = g.tick(1.0);
            let margin_before = g.summary(&r0).national_margin_eur;
            // Plusieurs jours : les NPC décident, la spirale s'installe.
            let mut last = Vec::new();
            for _ in 0..(24 * 6) {
                last = g.tick(1.0);
            }
            let s = g.summary(&last);
            (margin_before, s)
        };
        let (margin_before_high, high) = run(0.30);
        let (_margin_before_low, low) = run(0.08);

        // À tarif bas, personne ne décroche ; à tarif élevé, tout le monde.
        assert_eq!(low.self_producing_households, 0, "tarif bas -> aucun décrochage");
        assert_eq!(high.self_producing_households, 3, "tarif élevé -> les 3 maisons décrochent");
        // La dépendance au réseau s'effondre côté tarif élevé.
        assert!(high.dependency_rate < low.dependency_rate, "spirale : dépendance en baisse ({} < {})", high.dependency_rate, low.dependency_rate);
        assert!(high.dependency_rate < 0.2, "les foyers autoproduisent l'essentiel ({})", high.dependency_rate);
        // Le revenu que le national tire des imports s'effondre après décrochage.
        assert!(high.national_margin_eur < margin_before_high, "la marge nationale chute après décrochage ({} < {})", high.national_margin_eur, margin_before_high);
    }

    #[test]
    fn scenario_builds_and_ticks_deterministically() {
        let mut a = Grid::scenario(2024, 2, 3);
        let mut b = Grid::scenario(2024, 2, 3);
        // 1 national + 2 quartiers + 2*3 maisons = 9 nœuds.
        assert_eq!(a.nodes.len(), 9);
        assert_eq!(a.nodes.iter().filter(|n| n.tier == Tier::Household).count(), 6);
        for _ in 0..50 {
            let ra = a.tick(0.5);
            let rb = b.tick(0.5);
            for (x, y) in ra.iter().zip(rb.iter()) {
                assert_eq!(x.balance_eur.to_bits(), y.balance_eur.to_bits());
            }
        }
    }

    #[test]
    fn foisonnement_is_deterministic_and_decorrelated() {
        let base = Weather { wind_ms: 8.0, irradiance_kw_m2: 0.5, air_temp_c: 15.0, river_flow_m3s: 2.0 };
        let (mut a, ..) = small_grid();
        let (mut b, ..) = small_grid();
        a.propagate_weather(base, 0.3);
        b.propagate_weather(base, 0.3);
        // Reproductible : même graine -> même météo par nœud.
        for (na, nb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(na.weather.wind_ms.to_bits(), nb.weather.wind_ms.to_bits());
            assert_eq!(na.weather.irradiance_kw_m2.to_bits(), nb.weather.irradiance_kw_m2.to_bits());
        }
        // Décorrélé : au moins deux nœuds ont une météo différente.
        let winds: Vec<f64> = a.nodes.iter().map(|n| n.weather.wind_ms).collect();
        assert!(winds.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-9), "les nœuds ne sont pas tous identiques");
    }
}
