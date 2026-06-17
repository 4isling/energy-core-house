// Façade typée autour du module WASM généré par wasm-pack dans `./pkg`.
// `init()` charge le .wasm ; `Game` est la struct exposée par src/wasm.rs.
import init, { Game } from "./pkg/energy_core.js";

let ready: Promise<void> | null = null;

/** Charge le module WASM une seule fois (idempotent). */
export function initEngine(): Promise<void> {
  if (!ready) ready = init().then(() => undefined);
  return ready;
}

/** Instancie une partie : budget en €, graine météo déterministe. */
export function createGame(budget: number, seed: number): Game {
  return new Game(budget, seed);
}

export type { Game };
