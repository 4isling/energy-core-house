//! Carte de tuiles et terrain procédural.
//!
//! La carte est une grille déterministe générée depuis un `seed` : même seed →
//! même carte (rejouabilité + tests). Chaque tuile porte des **facteurs locaux**
//! qui *modulent* les entrées de la physique (`physics.rs`) sans changer aucune
//! formule : `vent_local = météo.vent × wind_factor`, idem soleil et débit d'eau.
//! Le placement d'un actif sur une tuile capture ces facteurs (cf. `TileEnv`
//! dans `sim.rs`).
//!
//! Génération (tout déterministe via un xorshift/hash seedé) :
//! 1. **Élévation** : bruit fractal (fBm de bruit de valeur).
//! 2. **Rivières** : routage d'écoulement D8 + accumulation de flux (les vallées
//!    drainantes deviennent de l'eau ; plus de flux en aval = plus gros débit).
//! 3. **Vent / soleil** : dérivés de l'altitude, du couvert forestier et d'un
//!    léger bruit, bornés.

use serde::{Deserialize, Serialize};

/// Largeur par défaut de la carte (tuiles).
pub const MAP_W: u16 = 500;
/// Hauteur par défaut de la carte (tuiles).
pub const MAP_H: u16 = 500;

/// Nature du sol d'une tuile (sert au rendu et aux règles de constructibilité).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ground {
    Water = 0,
    Plain = 1,
    Forest = 2,
    Hill = 3,
    Mountain = 4,
}

impl Ground {
    pub fn label(self) -> &'static str {
        match self {
            Ground::Water => "Eau",
            Ground::Plain => "Plaine",
            Ground::Forest => "Forêt",
            Ground::Hill => "Colline",
            Ground::Mountain => "Montagne",
        }
    }
}

/// Une tuile de terrain. Facteurs stockés en `f32` (carte = beaucoup de tuiles).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TerrainTile {
    /// Altitude normalisée 0..1.
    pub elevation: f32,
    /// Multiplicateur de vent local (~0.4..1.6) : crêtes ventées, vallées abritées.
    pub wind_factor: f32,
    /// Multiplicateur d'ensoleillement local (~0.5..1.0) : forêt/ombrage réduisent.
    pub solar_factor: f32,
    /// Multiplicateur de débit d'eau (0 sur la terre ferme, >0 sur une rivière).
    pub water_factor: f32,
    pub ground: Ground,
}

impl TerrainTile {
    /// Peut-on poser un bâtiment / un actif terrestre ici ?
    /// (ni eau, ni montagne trop raide.)
    pub fn buildable(&self) -> bool {
        matches!(self.ground, Ground::Plain | Ground::Forest | Ground::Hill)
    }

    /// Tuile d'eau exploitable pour l'hydraulique ?
    pub fn is_water(&self) -> bool {
        self.ground == Ground::Water && self.water_factor > 0.0
    }
}

/// Une carte de terrain : grille `width × height` de `TerrainTile`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainMap {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<TerrainTile>,
}

impl Default for TerrainMap {
    /// Carte vide (0×0) : utilisée tant qu'aucune génération n'a eu lieu.
    fn default() -> Self {
        Self { width: 0, height: 0, tiles: Vec::new() }
    }
}

impl TerrainMap {
    /// Génère la carte par défaut (500×500) depuis un seed.
    pub fn generate(seed: u64) -> Self {
        Self::generate_sized(seed, MAP_W, MAP_H)
    }

    /// Génère une carte `w × h` depuis un seed (tailles réduites utiles aux tests).
    pub fn generate_sized(seed: u64, w: u16, h: u16) -> Self {
        let (wu, hu) = (w as usize, h as usize);
        let n = wu * hu;

        // --- 1. Champ d'élévation (fBm) + couvert forestier (bruit séparé). ---
        let mut elevation = vec![0.0f32; n];
        // Échelle des reliefs : plus grand = features plus larges.
        let scale = (w.max(h) as f64 / 6.0).max(8.0);
        for y in 0..hu {
            for x in 0..wu {
                let e = fbm(x as f64 / scale, y as f64 / scale, seed, 5);
                elevation[y * wu + x] = e as f32;
            }
        }

        // --- 2. Rivières : routage D8 + accumulation de flux. ---
        let water_factor = compute_rivers(&elevation, wu, hu);

        // --- 3. Assemblage des tuiles (sol, vent, soleil). ---
        let mut tiles = Vec::with_capacity(n);
        for y in 0..hu {
            for x in 0..wu {
                let i = y * wu + x;
                let e = elevation[i];
                let wf = water_factor[i];
                let forest = fbm(
                    x as f64 / (scale * 0.6),
                    y as f64 / (scale * 0.6),
                    seed ^ 0xF0_7E_57_00u64,
                    3,
                ) as f32;

                let ground = if wf > 0.0 {
                    Ground::Water
                } else if e > 0.82 {
                    Ground::Mountain
                } else if e > 0.62 {
                    Ground::Hill
                } else if forest > 0.62 && e > 0.30 {
                    Ground::Forest
                } else {
                    Ground::Plain
                };

                // Vent : croît avec l'altitude (crêtes ventées) ; forêt freine un peu.
                let mut wind_factor = (0.45 + 1.05 * e).clamp(0.40, 1.60);
                if ground == Ground::Forest {
                    wind_factor *= 0.85;
                }

                // Soleil : plein ~1.0 ; forêt ombrage ; léger bruit de variation.
                let sun_noise = 0.9
                    + 0.1
                        * (fbm(
                            x as f64 / (scale * 0.4),
                            y as f64 / (scale * 0.4),
                            seed ^ 0x5A_55_00_00,
                            2,
                        ) as f32);
                let base_sun = match ground {
                    Ground::Forest => 0.70,
                    Ground::Mountain => 0.90,
                    _ => 1.0,
                };
                let solar_factor = (base_sun * sun_noise).clamp(0.50, 1.0);

                tiles.push(TerrainTile {
                    elevation: e,
                    wind_factor: wind_factor as f32,
                    solar_factor,
                    water_factor: wf,
                    ground,
                });
            }
        }

        Self { width: w, height: h, tiles }
    }

    #[inline]
    pub fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    #[inline]
    pub fn in_bounds(&self, x: u16, y: u16) -> bool {
        x < self.width && y < self.height
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&TerrainTile> {
        if self.in_bounds(x, y) {
            Some(&self.tiles[self.idx(x, y)])
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Bruit de valeur déterministe (sans dépendance, identique web/natif)
// ---------------------------------------------------------------------------

/// Hash entier -> [0,1). Mélange façon SplitMix64 sur (x, y, seed).
fn hash01(xi: i64, yi: i64, seed: u64) -> f64 {
    let mut h = seed
        ^ (xi as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (yi as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

#[inline]
fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Bruit de valeur bilinéaire lissé en (x, y).
fn value_noise(x: f64, y: f64, seed: u64) -> f64 {
    let x0 = x.floor();
    let y0 = y.floor();
    let xi = x0 as i64;
    let yi = y0 as i64;
    let sx = smoothstep(x - x0);
    let sy = smoothstep(y - y0);
    let n00 = hash01(xi, yi, seed);
    let n10 = hash01(xi + 1, yi, seed);
    let n01 = hash01(xi, yi + 1, seed);
    let n11 = hash01(xi + 1, yi + 1, seed);
    lerp(lerp(n00, n10, sx), lerp(n01, n11, sx), sy)
}

/// Bruit fractal (fBm) normalisé dans [0,1].
fn fbm(x: f64, y: f64, seed: u64, octaves: u32) -> f64 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o as u64 * 0x9E37));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Rivières : routage D8 + accumulation de flux
// ---------------------------------------------------------------------------

/// Renvoie le `water_factor` par tuile (0 sur terre, >0 sur rivière), calculé par
/// accumulation de flux descendant : chaque tuile draine vers son plus bas voisin,
/// et le flux s'accumule de l'amont vers l'aval (rivières dendritiques).
fn compute_rivers(elevation: &[f32], w: usize, h: usize) -> Vec<f32> {
    let n = w * h;
    // Plus bas voisin (D8) pour chaque tuile, s'il est strictement plus bas.
    let mut downstream: Vec<i32> = vec![-1; n];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let e = elevation[i];
            let mut best = e;
            let mut best_idx: i32 = -1;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    if elevation[ni] < best {
                        best = elevation[ni];
                        best_idx = ni as i32;
                    }
                }
            }
            downstream[i] = best_idx;
        }
    }

    // Tri des indices par altitude décroissante (amont -> aval).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by(|&a, &b| {
        elevation[b]
            .partial_cmp(&elevation[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Accumulation : chaque tuile reçoit 1 unité de pluie + l'amont, et la
    // transmet à son aval.
    let mut flow = vec![1.0f32; n];
    for &i in &order {
        let d = downstream[i];
        if d >= 0 {
            flow[d as usize] += flow[i];
        }
    }

    // Seuil au-delà duquel une tuile devient rivière. Mis à l'échelle de la carte
    // pour donner des rivières sur les grandes comme sur les petites cartes.
    let threshold = (n as f32 * 0.0035).max(6.0);

    let mut water = vec![0.0f32; n];
    for i in 0..n {
        if flow[i] >= threshold {
            // Facteur de débit borné : plus de flux en aval = plus gros cours d'eau.
            let f = 0.6 + (flow[i] / (threshold * 6.0));
            water[i] = f.min(4.0);
        }
    }
    water
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_seed() {
        let a = TerrainMap::generate_sized(123, 80, 80);
        let b = TerrainMap::generate_sized(123, 80, 80);
        assert_eq!(a.width, b.width);
        for (ta, tb) in a.tiles.iter().zip(b.tiles.iter()) {
            assert_eq!(ta.elevation, tb.elevation);
            assert_eq!(ta.ground, tb.ground);
            assert_eq!(ta.water_factor, tb.water_factor);
        }
    }

    #[test]
    fn different_seed_differs() {
        let a = TerrainMap::generate_sized(1, 80, 80);
        let b = TerrainMap::generate_sized(2, 80, 80);
        assert!(a.tiles.iter().zip(b.tiles.iter()).any(|(x, y)| x.elevation != y.elevation));
    }

    #[test]
    fn factors_are_bounded() {
        let m = TerrainMap::generate_sized(7, 64, 64);
        for t in &m.tiles {
            assert!(t.elevation >= 0.0 && t.elevation <= 1.0);
            assert!(t.wind_factor >= 0.30 && t.wind_factor <= 1.7, "wind {}", t.wind_factor);
            assert!(t.solar_factor >= 0.45 && t.solar_factor <= 1.05, "solar {}", t.solar_factor);
            assert!(t.water_factor >= 0.0 && t.water_factor <= 4.0);
        }
    }

    #[test]
    fn has_some_rivers_and_buildable_land() {
        let m = TerrainMap::generate_sized(42, 120, 120);
        let water = m.tiles.iter().filter(|t| t.is_water()).count();
        let buildable = m.tiles.iter().filter(|t| t.buildable()).count();
        assert!(water > 0, "la carte doit comporter au moins une rivière");
        assert!(buildable > 0, "la carte doit comporter de la terre constructible");
    }

    #[test]
    fn water_tiles_are_not_buildable() {
        let m = TerrainMap::generate_sized(9, 64, 64);
        for t in &m.tiles {
            if t.is_water() {
                assert!(!t.buildable());
            }
        }
    }
}
