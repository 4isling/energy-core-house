# CLAUDE.md

Guide pour travailler dans ce dépôt (cœur Rust + front web WASM).

## Vue d'ensemble

`energy-core` est un moteur de simulation énergétique **déterministe** compilé en
WASM, sans dépendance au rendu. Un front React + TypeScript + Vite (`web/`)
appelle le WASM. Deux jeux partagent le même cœur :

1. **Mono-carte (« Village »)** — `sim.rs` : un `Park` mutualisé alimente des
   `Building` (habitants + appareils) sur une carte de tuiles. `Game` (wasm.rs).
2. **Multi-couches (« Réseau »)** — `grid.rs` : hiérarchie de `GridNode`
   (maison → quartier → national) avec dispatch récursif deux passes, P2P,
   îlotage, économie par portefeuille et NPC auto-investisseurs. `GridGame`.

Le mono-carte **n'a pas été réécrit** : la couche réseau s'ajoute au-dessus. Un
national sans enfants se comporte comme le jeu mono-carte d'origine.

## Conventions

- **Doc en français, identifiants en anglais.** `serde` derive sur les types
  publics.
- **Déterminisme strict** dans les chemins de simulation : RNG seedé
  (`weather::Rng`, xorshift), **aucune `HashMap`** non ordonnée, itération en
  ordre d'indices. À scénario + seed fixés, la sortie est reproductible bit-à-bit.
- **Pas de panique sur budget insuffisant** : les `build_*` renvoient `bool`
  (ou un `Result`/code) — jamais d'`unwrap` sur une décision joueur.
- `cargo test` doit rester **vert** à chaque commit ; ne pas supprimer de tests.

## Cœur partagé : `Park` (sim.rs)

Le dispatch local est factorisé en méthodes **pures** réutilisées aux deux
échelles :

- `Park::balance_local(load, weather, dt, line_loss)` → renouvelable + batterie,
  renvoie un résidu signé (`ParkDispatch`), **sans** centrales.
- `Park::cover_with_thermal(deficit, dt, line_loss)` → centrales pilotables sur
  le déficit agrégé (`ThermalCover`).
- `Park::dispatch(...)` = les deux composées (ce qu'utilise `SimState::tick`).

La couche réseau (`grid.rs`) appelle `balance_local` en première passe, agrège
les enfants (P2P + liens), **puis** `cover_with_thermal` sur le déficit agrégé.

## Réseau multi-couches : `grid.rs`

- Arène `Vec<GridNode>` + `NodeId` (indices). `Grid::scenario(seed, n_districts,
  houses_per_district)` construit un arbre de départ.
- `Grid::tick(dt)` équilibre tout l'arbre (`balance` récursif) et renvoie un
  `NodeReport` par nœud. `Grid::summary(&reports)` → indicateurs de spirale.
- L'argent circule **à contre-sens** de l'énergie. Coûts fixes + revenus crédités
  par tick. `npc_invest_step` (cadence journalière) : palier solaire → batterie →
  micro-éolienne selon le payback au tarif courant.
- `set_national_tariff`, `set_islanded`, `propagate_weather` (foisonnement).

## Façade WASM : `wasm.rs`

- `Game` (mono-carte) **inchangé**.
- `GridGame` : `tick`, `summary`, `node(id)`/`nodes()`/`children(id)`,
  `set_national_tariff`, `island_node`, `build_solar_on/wind_on/battery_on`.
- Sérialisation via `serde-wasm-bindgen` (objets JS natifs).

## Front web (`web/`)

- `Root.tsx` : sélecteur de mode Village / Réseau.
- Village : `App.tsx` + `useGame.ts` (inchangé).
- Réseau : `GridApp.tsx` + `useGrid.ts` ; composants `components/grid/`
  (`LayerNav`, `SpiralPanel`, `NodeInspector`). Types dans `gridTypes.ts`.
- `web/src/pkg/` est **généré** par wasm-bindgen (gitignoré) : ne pas le commiter.

## Commandes

```bash
cargo test                       # cœur Rust (doit rester vert)
cargo build --features wasm      # vérifie la compilation de la façade WASM

cd web
npm install
npm run wasm                     # génère web/src/pkg (cargo + wasm-bindgen)
npm run dev                      # http://localhost:5173
tsc --noEmit                     # type-check du front
npm run build                    # tsc && vite build
```

> ⚠️ Le toolchain WASM (cible `wasm32-unknown-unknown` + `wasm-bindgen`) est
> requis pour `npm run wasm` et donc pour exécuter le front. Sans lui, on peut
> tout de même `cargo test`, `cargo build --features wasm` et type-checker le TS
> (le dossier `pkg` doit alors contenir une déclaration `.d.ts` reflétant les
> façades).

## Phases d'implémentation du réseau (historique)

0. Refactor non destructif (`Park::dispatch` scindé).
1. `grid.rs` cœur (dispatch deux passes, P2P, îlotage) — testé.
2. Économie, coûts fixes, NPC, spirale, foisonnement — testé.
3. Façade WASM `GridGame`.
4. Frontend multi-couches (navigation, vues, inspecteur).
5. Polish (visualisation des flux P2P, équilibrage UX).
