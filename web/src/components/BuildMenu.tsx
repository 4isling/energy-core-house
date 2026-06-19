import { useState } from "react";
import type { GameApi } from "../useGame";
import {
  APPLIANCE_CATALOG,
  BUILD_TOOLS,
  RESIDENT_PROFILES,
  type BuildTool,
  type BuildingView,
} from "../types";

// Palette de construction (sélection d'un outil) + gestion des foyers.
// La pose se fait ensuite en cliquant une tuile de la carte.
export function BuildMenu({
  game,
  budget,
  selectedTool,
  onSelectTool,
}: {
  game: GameApi;
  budget: number;
  selectedTool: BuildTool | null;
  onSelectTool: (t: BuildTool | null) => void;
}) {
  const { buildings } = game;
  const [selected, setSelected] = useState<number | null>(null);
  const current = buildings.find((b) => b.id === selected) ?? buildings[0] ?? null;

  const energy = BUILD_TOOLS.filter((t) => t.category === "energy");
  const homes = BUILD_TOOLS.filter((t) => t.category === "building");

  return (
    <aside className="build-menu">
      <section>
        <h2>Construire</h2>
        <p className="muted">
          {selectedTool
            ? `Cliquez une tuile pour poser : ${selectedTool.label}.`
            : "Choisissez un élément, puis cliquez une tuile."}
        </p>
        <div className="palette">
          {energy.map((t) => (
            <ToolBtn key={t.id} tool={t} budget={budget} selected={selectedTool?.id === t.id} onSelect={onSelectTool} />
          ))}
        </div>
        <h3>Foyers</h3>
        <div className="palette">
          {homes.map((t) => (
            <ToolBtn key={t.id} tool={t} budget={budget} selected={selectedTool?.id === t.id} onSelect={onSelectTool} />
          ))}
        </div>
      </section>

      <section>
        <h2>Gérer un foyer</h2>
        {current ? (
          <ManageBuilding game={game} buildings={buildings} current={current} onSelect={setSelected} />
        ) : (
          <p className="muted">Construisez un foyer pour le gérer.</p>
        )}
      </section>
    </aside>
  );
}

function ToolBtn({
  tool,
  budget,
  selected,
  onSelect,
}: {
  tool: BuildTool;
  budget: number;
  selected: boolean;
  onSelect: (t: BuildTool | null) => void;
}) {
  const affordable = budget >= tool.cost;
  return (
    <button
      className={`tool-btn${selected ? " selected" : ""}`}
      disabled={!affordable}
      title={tool.detail}
      onClick={() => onSelect(selected ? null : tool)}
    >
      <span className="tool-emoji">{tool.emoji}</span>
      <span className="tool-label">{tool.label}</span>
      <span className="cost">{tool.cost.toLocaleString("fr-FR")} €</span>
    </button>
  );
}

function ManageBuilding({
  game,
  buildings,
  current,
  onSelect,
}: {
  game: GameApi;
  buildings: BuildingView[];
  current: BuildingView;
  onSelect: (id: number) => void;
}) {
  const [name, setName] = useState("Habitant");
  const [profile, setProfile] = useState(RESIDENT_PROFILES[0].code);

  return (
    <>
      <select
        className="building-select"
        value={current.id}
        onChange={(e) => onSelect(Number(e.target.value))}
      >
        {buildings.map((b) => (
          <option key={b.id} value={b.id}>
            {b.name} #{b.id} — {b.residents.length} hab.
          </option>
        ))}
      </select>

      <h3>Appareils</h3>
      <div className="catalog">
        {APPLIANCE_CATALOG.map((a) => (
          <button key={a.code} onClick={() => game.addApplianceTo(current.id, a.code)}>
            + {a.label}
            <span className="power">{a.power_kw} kW</span>
          </button>
        ))}
      </div>
      {current.appliances.length === 0 ? (
        <p className="muted">Aucun appareil installé.</p>
      ) : (
        <ul className="appliance-list">
          {current.appliances.map((a) => (
            <li key={a.id} className={a.on ? "on" : "off"}>
              <button onClick={() => game.toggleAppliance(a.id)}>
                <span className="dot" /> {a.name}
              </button>
              <span className="power">{a.power_kw} kW</span>
            </li>
          ))}
        </ul>
      )}

      <h3>Habitants</h3>
      <div className="resident-form">
        <input value={name} onChange={(e) => setName(e.target.value)} />
        <select value={profile} onChange={(e) => setProfile(e.target.value)}>
          {RESIDENT_PROFILES.map((p) => (
            <option key={p.code} value={p.code}>
              {p.label}
            </option>
          ))}
        </select>
        <button onClick={() => game.addResidentTo(current.id, name, profile)}>+ Ajouter</button>
      </div>
      <ul className="resident-list">
        {current.residents.map((r, i) => (
          <li key={i}>
            👤 {r.name} ({r.profile}) — confort {r.comfort.toFixed(0)} %
          </li>
        ))}
      </ul>
    </>
  );
}
