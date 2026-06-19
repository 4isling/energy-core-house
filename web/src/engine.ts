// Façade typée autour du module WASM généré par wasm-pack dans `./pkg`.
// `init()` charge le .wasm ; `Game` (mono-carte) et `GridGame` (réseau
// multi-couches) sont les structs exposées par src/wasm.rs.
import init, { Game, GridGame } from "./pkg/energy_core.js";

let ready: Promise<void> | null = null;

/** Charge le module WASM une seule fois (idempotent). */
export function initEngine(): Promise<void> {
  if (!ready) ready = init().then(() => undefined);
  return ready;
}

/** Instancie une partie mono-carte : budget en €, graine météo déterministe. */
export function createGame(budget: number, seed: number): Game {
  return new Game(budget, seed);
}

/** Instancie un réseau multi-couches : 1 national → quartiers → maisons. */
export function createGridGame(
  seed: number,
  nDistricts: number,
  housesPerDistrict: number,
): GridGame {
  return new GridGame(seed, nDistricts, housesPerDistrict);
}

export type { Game, GridGame };
