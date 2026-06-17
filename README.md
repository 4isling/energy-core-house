# energy-core

Moteur de simulation énergétique **déterministe**, **testé** et **portable** pour
un jeu de gestion. Toute la physique (éolien, solaire, hydro, thermique,
stockage), le dispatch et l'économie vivent ici — **zéro dépendance au rendu**.

Le même crate compile :

- en **WASM** (`--features wasm`) pour un front web (React / PixiJS / Canvas) ;
- en **natif** (`rlib`) pour un moteur de jeu (Godot via [gdext], Bevy) ou des tests.

C'est le « cerveau » du jeu. Le visuel (web, Godot, Bevy…) n'est qu'un afficheur
qui appelle `tick()` et lit le `TickReport`.

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
├── sim.rs        Park + SimState + tick() : dispatch merit-order
└── wasm.rs       Façade wasm-bindgen (feature "wasm")
```

**Dispatch (ordre de mérite)** à chaque pas de temps :
renouvelable → batterie → thermique → réseau → non-fourni (black-out).

## Build

### Natif + tests
```bash
cargo test          # 18 tests unitaires + doctest
cargo build --release
```

### WASM pour le web
```bash
# une fois : cargo install wasm-pack
wasm-pack build --target web --features wasm --out-dir ../web/pkg
```
Puis côté front :
```js
import init, { Game } from "./pkg/energy_core.js";

await init();
const game = new Game(50_000, 1234);   // budget €, seed météo
game.build_solar(6.0);                  // 6 kWc
game.build_battery(10.0);               // 10 kWh
game.set_load_kw(2.5);

// boucle de jeu (la météo est générée dans le core, déterministe)
setInterval(() => {
  const report = game.tick(0.5);        // avance de 30 min, renvoie un objet JS
  render(report);                       // report.wind_kw, .soc_pct, .blackout, .budget_eur…
}, 250);
```

Pour brancher de **vraies données** (CSV éCO2mix / Météo-France / PVGIS /
Hub'Eau) au lieu de la météo procédurale, utilise `tick_with_weather(dt, wind,
irradiance, temp, flow)` et `set_spot_price(...)` alimentés depuis tes datasets.

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
