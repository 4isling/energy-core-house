import { useState } from "react";
import { App } from "./App";
import { GridApp } from "./GridApp";

type Mode = "village" | "grid";

/** Sélecteur de mode : le jeu mono-carte « Village » (existant) et la nouvelle
 * vue « Réseau » multi-couches coexistent. Chaque mode a sa propre instance
 * WASM, montée à la demande. */
export function Root() {
  const [mode, setMode] = useState<Mode>("village");
  return (
    <div className="root">
      <nav className="mode-switch">
        <button
          className={mode === "village" ? "active" : ""}
          onClick={() => setMode("village")}
        >
          🏘️ Village (mono-carte)
        </button>
        <button
          className={mode === "grid" ? "active" : ""}
          onClick={() => setMode("grid")}
        >
          🏛️ Réseau (multi-couches)
        </button>
      </nav>
      {mode === "village" ? <App /> : <GridApp />}
    </div>
  );
}
