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

use crate::sim::Park;
use crate::weather::Weather;

/// Identifiant d'un nœud dans l'arène (indice dans `Grid::nodes`).
pub type NodeId = u32;

const EPS: f64 = 1e-6;

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

/// L'arbre complet du réseau.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub nodes: Vec<GridNode>,
    /// La racine (le national).
    pub root: NodeId,
    pub tick_count: u64,
}

impl Grid {
    /// Crée un réseau réduit à sa racine nationale.
    pub fn new_national(name: impl Into<String>, starting_balance_eur: f64) -> Self {
        let mut root = GridNode::new(0, Tier::National, name);
        root.wallet = Wallet::new(starting_balance_eur);
        Self { nodes: vec![root], root: 0, tick_count: 0 }
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

        // Finalise cash-flow, solde et état de charge par nœud.
        for (i, n) in self.nodes.iter().enumerate() {
            reports[i].cash_flow_eur = n.wallet.balance_eur - before[i];
            reports[i].balance_eur = n.wallet.balance_eur;
            reports[i].soc_pct = n.park.battery.as_ref().map(|b| b.soc_pct()).unwrap_or(0.0);
        }

        self.tick_count += 1;
        reports
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
}
