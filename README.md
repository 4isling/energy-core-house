# energy-core

> 🎮 **Jouer en ligne** : https://4isling.github.io/energy-core-house/

Moteur de simulation énergétique **déterministe**, **testé** et **portable** pour
un jeu de gestion. Toute la physique (éolien, solaire, hydro, thermique,
stockage), le dispatch et l'économie vivent ici — **zéro dépendance au rendu**.

Le même crate compile :

- en **WASM** (`--features wasm`) pour un front web (React / PixiJS / Canvas) ;
- en **natif** (`rlib`) pour un moteur de jeu (Godot via [gdext], Bevy) ou des tests.

C'est le « cerveau » du jeu. Le visuel (web, Godot, Bevy…) n'est qu'un afficheur
qui appelle `tick()` et lit le `TickReport`.

## Genèse

Ce projet est né pour un **hackathon DefendIntelligence**. Faute de temps de
développement, il n'a finalement **pas été soumis** — mais le sujet était trop
intéressant pour le laisser tomber, alors il a continué d'évoluer après coup.

L'idée de départ : modéliser **honnêtement** un système énergétique, et laisser
une vraie tension *émerger de la mécanique* plutôt que de la scripter. D'un petit
jeu de **village mono-carte** (foyers, appareils, météo, dispatch), il a grandi
vers un **réseau multi-couches** (maison → quartier → national) où apparaît la
« spirale de la mort » des réseaux : plus on autoproduit, plus le réseau central
perd des revenus, plus il monte ses tarifs, plus les gens décrochent…

Bref, un prototype inachevé côté soumission, mais un bac à sable qui marche, testé
et déterministe. C'était surtout **sympa à coder**. 🙂


## Architecture

```
src/
├── physics.rs    Production par filière (formules + valeurs par défaut France)
│                 WindTurbine  : P = ½·ρ·A·v³·Cp, bornée par Betz, 3 régimes
│                 SolarArray   : kWc × irradiance × PR × dérating température
│                 HydroTurbine : P = ρ·g·Q·H·η, rendement partiel par type
│                 ThermalPlant : charbon/gaz/fioul, CO2 & coût combustible
├── storage.rs    Battery : rendement round-trip, limites de puissance
├── weather.rs    Weather (grandeurs physiques) + ProceduralWeather (RNG seedé)
├── economy.rs    CAPEX/OPEX, émissions, réseau (import/export prix spot)
├── appliance.rs  Appliance : appareils consommateurs (frigo, four, VE…)
├── resident.rs   Resident : habitants (NPC) + routines journalières
├── sim.rs        Park + SimState + tick() : dispatch merit-order (mono-carte)
├── grid.rs       Réseau MULTI-COUCHES : Grid/GridNode/Link/Wallet, dispatch
│                 récursif deux passes, P2P, îlotage, NPC, spirale
└── wasm.rs       Façade wasm-bindgen : Game (mono-carte) + GridGame (réseau)
```

### Réseau énergétique multi-couches (national / quartier / maison)

Au-dessus du jeu mono-carte, `grid.rs` modélise le réseau comme une **hiérarchie
de nœuds** calquée sur le réel : maison → poste de quartier → réseau national.
Un seul type, [`GridNode`], s'emboîte à toutes les échelles ; l'arbre vit dans
une **arène** (`Vec<GridNode>` + indices `NodeId`), sérialisable et déterministe.

**Dispatch en deux passes**, par tick, pour chaque nœud (`balance` récursif) :

1. **Équilibrage local** — renouvelable + batterie locale (`Park::balance_local`).
2. **Descente récursive** — on équilibre d'abord chaque enfant.
3. **Troc P2P** — au niveau du parent, on apparie surplus et déficits des enfants
   (le micro-réseau de quartier : le voisin qui a trop de solaire alimente celui
   qui en manque), avec règlement à un **prix local** entre import et export.
4. **Échange par `Link`** — le résidu de chaque enfant remonte (capacité, pertes
   en ligne, règlement monétaire **à contre-sens de l'énergie**). `autonomy_pref`
   réduit la part qu'un enfant accepte d'importer.
5. **Couverture** — si le pool agrégé est en déficit, le nœud lance ses centrales
   pilotables (`Park::cover_with_thermal`).
6. **Résidu** — ce qui reste monte au parent via l'`uplink` ; la racine couvre
   son déficit (black-out sinon) ou écrête (curtailment) son surplus.

**Économie & spirale de la mort.** Chaque nœud a un `Wallet` : l'argent circule à
contre-sens de l'énergie (qui importe paie, qui exporte touche), et des **coûts
fixes** drainent le portefeuille quoi qu'il arrive. Le tarif national est un
levier joueur : l'augmenter améliore le revenu par kWh… mais rend
l'**autoproduction** des foyers plus rentable. Les maisons NPC s'auto-équipent
alors (toiture solaire → batterie → micro-éolienne) selon le payback au tarif
courant → leurs imports s'effondrent → le revenu du national fond alors que ses
coûts fixes restent : la **spirale de la mort** émerge de la mécanique, sans être
scriptée (`GridSummary` expose la marge nationale et le taux de dépendance).
**Îlotage** : un nœud peut se déconnecter de son parent (`Link.connected`) et
survivre sur sa prod — un quartier bien équipé tient même si le national tombe.

**Foisonnement** : `Grid::propagate_weather` éclate la météo de base en un bruit
**décorrélé par nœud** (seedé) ; une maison isolée subit toute la variance, alors
qu'un parent agrégeant de nombreux enfants la lisse — l'autonomie a un prix réel.

Le front web propose un **sélecteur de mode** : « Village » (mono-carte, existant)
et « Réseau » (multi-couches) avec navigation drill-down National ⇄ Quartier ⇄
Maison, panneau de spirale + tarif au national, gestion des actifs partagés au
quartier, et inspecteur de maison (les foyers NPC décident ; on les influence par
le tarif).

### Jeu « village micro-réseau »

Le même cœur sert un jeu de gestion de **colonie / village** centré sur l'énergie.
Le joueur gère plusieurs **bâtiments** (`building.rs`, foyers de types Studio /
Family / Elders) reliés à un **micro-réseau partagé** : la production (`WindTurbine::micro()`
~5 kW, `ThermalPlant::genset()` ~6 kW, solaire kWc) et le stockage (batterie kWh)
sont mutualisés (`Park`), tandis que chaque bâtiment porte ses **appareils**
(`appliance.rs`) et ses **habitants** (`resident.rs`) dont les routines déterministes
allument/éteignent ces appareils. La demande du village = somme des bâtiments. Le
dispatch équilibre cette demande agrégée ; un black-out fait baisser le confort de
tous les colons (`TickReport.avg_comfort_pct`, détail par bâtiment dans
`TickReport.buildings`). La boucle : faire **grandir** la colonie (nouveaux foyers →
plus d'habitants → plus de demande), **dimensionner** le réseau pour éviter les
black-out, et garder les colons **confortables** sous contrainte de budget et de CO₂.

Un front web React + WASM vit dans `web/` (dashboard + graphes + schéma SVG
animé) :
```bash
cargo install wasm-pack          # une fois
cd web && npm install
npm run wasm                     # compile le cœur en WASM dans web/src/pkg
npm run dev                      # http://localhost:5173
```

**Dispatch (ordre de mérite)** à chaque pas de temps :
renouvelable → batterie → thermique → réseau → non-fourni (black-out).

## Build

### Natif + tests
```bash
cargo test          # tests unitaires + doctest
cargo build --release
```

### WASM pour le web
```bash
# une fois : cargo install wasm-pack
wasm-pack build --target web --features wasm --out-dir web/src/pkg
```
Si `wasm-pack` est incompatible avec votre version de cargo (flag `--out-dir` /
`--artifact-dir`), repli manuel équivalent :
```bash
cargo build --target wasm32-unknown-unknown --release --features wasm
wasm-bindgen target/wasm32-unknown-unknown/release/energy_core.wasm \
  --out-dir web/src/pkg --target web
```
Puis côté front :
```js
import init, { Game } from "./pkg/energy_core.js";

await init();
const game = new Game(120_000, 1234);   // budget €, seed météo ; village de départ peuplé
game.build_solar(6.0);                   // 6 kWc, prod partagée
game.build_battery(10.0);                // 10 kWh, stockage partagé
const id = game.build_building("family"); // nouveau foyer (CAPEX débité)
game.add_appliance_to(id, "ev_charger"); // appareil dans ce foyer

// boucle de jeu (la météo est générée dans le core, déterministe)
setInterval(() => {
  const report = game.tick(0.5);        // avance de 30 min, renvoie un objet JS
  render(report);                       // report.population, .load_kw, .blackout, .buildings…
}, 250);
```

Pour brancher de **vraies données** (CSV éCO2mix / Météo-France / PVGIS /
Hub'Eau) au lieu de la météo procédurale, utilise `tick_with_weather(dt, wind,
irradiance, temp, flow)` et `set_spot_price(...)` alimentés depuis tes datasets.

### Réseau multi-couches via `GridGame`

```js
import init, { GridGame } from "./pkg/energy_core.js";

await init();
const grid = new GridGame(1234, 3, 4);   // seed, 3 quartiers, 4 maisons chacun

grid.set_national_tariff(0.30, 0.10);    // tarif élevé → pousse au décrochage
grid.island_node(1, true);               // îlote le quartier #1 (résilience)

setInterval(() => {
  const reports = grid.tick(0.5);        // un NodeReport par nœud (objets JS)
  const s = grid.summary();              // marge nationale, taux de dépendance…
  render(reports, s, grid.nodes());      // grid.nodes() = arbre pour le drill-down
}, 250);
```

## Calibrer avec les données France

Les valeurs par défaut sont des ordres de grandeur réalistes. Pour le réalisme :

| Donnée                | Source open data (Licence Ouverte 2.0)        |
|-----------------------|------------------------------------------------|
| Profils de charge     | Enedis `coefficients-des-profils`              |
| Mix / facteurs charge | RTE éCO2mix `eco2mix-national-cons-def`        |
| Vent, irradiance      | Météo-France (meteo.data.gouv.fr), PVGIS       |
| Débits de rivière     | Hub'Eau hydrométrie                            |
| Prix de gros          | data.gouv.fr `wholesale-market` (éviter EPEX)  |

## Licence

AGPL-3.0-or-later. À ajuster selon ta stratégie (GPL/MPL si tu veux un usage
plus permissif par d'autres moteurs).

[gdext]: https://github.com/godot-rust/gdext
